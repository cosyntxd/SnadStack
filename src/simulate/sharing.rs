use super::place::{BoundingBox, PlaceLineTask, TaskState};
use std::{
    cell::UnsafeCell,
    hint::black_box,
    mem::{self, ManuallyDrop},
    ops::{Deref, DerefMut},
    ptr::NonNull,
    sync::{
        atomic::{
            self, AtomicUsize,
            Ordering::{Acquire, Relaxed, Release},
        },
        mpsc::{self, Receiver, Sender},
        Arc, Condvar, Mutex,
    },
    time::Duration,
};

/// A pool of objects that can be taken from and when a thread is done with the object, it will be
/// returned for later reuse. Overall this should be a slight performance increase because it reuses
/// allocations and is a little nicer on the cache
pub struct ObjectPool<T> {
    inner: Arc<ObjectPoolInner<T>>,
}
impl<T> ObjectPool<T> {
    pub fn new(size: usize, func: impl Fn() -> T + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::new(ObjectPoolInner {
                objects: Mutex::new((0..size).map(|_| UnsafeArc::new(func())).collect()),
                generator: Box::new(func),
            }),
        }
    }
    pub fn pop(&self) -> Reusable<T> {
        Reusable::new(Arc::clone(&self.inner), self.inner.new_data())
    }
}
pub struct ObjectPoolInner<T> {
    objects: Mutex<Vec<UnsafeArc<T>>>,
    generator: Box<dyn Fn() -> T>,
}
impl<T> ObjectPoolInner<T> {
    pub fn push(&self, value: UnsafeArc<T>) {
        self.objects.lock().unwrap().push(value);
    }
    pub fn new_data(&self) -> UnsafeArc<T> {
        self.objects
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| UnsafeArc::new((self.generator)()))
    }
}
/// A resuable object that when dropped will be returned back to the pool for reuse
pub struct Reusable<T> {
    pool: Arc<ObjectPoolInner<T>>,
    data: ManuallyDrop<UnsafeArc<T>>,
}
unsafe impl<T> Send for Reusable<T> {}
unsafe impl<T> Sync for Reusable<T> {}
impl<T> Reusable<T> {
    pub fn new(pool: Arc<ObjectPoolInner<T>>, data: UnsafeArc<T>) -> Self {
        Self {
            pool,
            data: ManuallyDrop::new(data),
        }
    }
    fn take(&mut self) -> UnsafeArc<T> {
        unsafe { ManuallyDrop::take(&mut self.data) }
    }
    pub fn take_and_realloc(&mut self) -> Reusable<T> {
        let mut new = Self::new(self.pool.clone(), self.pool.new_data());
        mem::swap(self, &mut new);
        new
    }
}

impl<T> Deref for Reusable<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> DerefMut for Reusable<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.data
    }
}

impl<T> Drop for Reusable<T> {
    fn drop(&mut self) {
        let val = self.take();
        self.pool.push(val);
    }
}

// Everything below this point is a foot gun
pub struct UnsafeShared<T> {
    data: UnsafeCell<T>,
}

unsafe impl<T> Send for UnsafeShared<T> {}
unsafe impl<T> Sync for UnsafeShared<T> {}

impl<T> UnsafeShared<T> {
    pub fn new(t: T) -> UnsafeShared<T> {
        Self {
            data: UnsafeCell::new(t),
        }
    }
    pub fn get_mut(&self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }
    pub fn get_ptr(&self) -> NonNull<T> {
        unsafe { NonNull::new_unchecked(self.data.get()) }
    }
}

impl<T> Deref for UnsafeShared<T> {
    type Target = T;

    fn deref(&self) -> &T {
        black_box(self.get_mut())
    }
}
impl<T> DerefMut for UnsafeShared<T> {
    fn deref_mut(&mut self) -> &mut T {
        black_box(self.get_mut())
    }
}

/// Allows for multiple mutable references over type T. Be very careful with this because the rust
/// borrow checker can no longer guarante safety
pub struct UnsafeSharedInner<T> {
    data: T,
    references: AtomicUsize,
}
pub struct UnsafeArc<T> {
    data: NonNull<UnsafeSharedInner<T>>,
}

unsafe impl<T> Send for UnsafeArc<T> {}
unsafe impl<T> Sync for UnsafeArc<T> {}

impl<T> UnsafeArc<T> {
    pub fn new(data: T) -> Self {
        let x: Box<_> = Box::new(UnsafeSharedInner {
            data,
            references: AtomicUsize::new(1),
        });
        Self {
            data: Box::leak(x).into(),
        }
    }
    /// SAFETY: Rustc/llvm tends to make incorrect optimizations with this becasue it might
    /// incorrectly assume that because it has a mutable reference then it should be the only
    /// one. However this may not be correct, because there can exist multiple mutable
    /// references. So the value might be incorrectly optimized under the assumption that it is
    /// exclusive. See example that only exits in debug:
    /// ```
    /// let shared = UnsafeArc::new(false);
    ///
    /// let clone2 = shared.get_mut();
    /// let clone = shared.get_mut();
    /// std::thread::scope(|s| {
    ///     s.spawn(|| {
    ///         *clone2 = true;
    ///     });
    ///     while !*clone {}
    /// });
    /// ```
    /// On release builds it does not terminate, but on debug builds, it will. The easiest solution
    /// is to spam `black_box(...)` everywhere
    fn inner(&self) -> &mut UnsafeSharedInner<T> {
        black_box(unsafe { &mut *self.data.as_ptr() })
    }

    /// See `self.inner()` for safety info
    pub fn get_mut(&self) -> &mut T {
        &mut self.inner().data
    }
    pub fn clone(&self) -> UnsafeArc<T> {
        let old_count = self.inner().references.fetch_add(1, Relaxed);
        if old_count > 1024 {
            log::error!(
                "[UnsafeArc<{}>] Over 1024 references, potential memory leak in UnsafeShared",
                std::any::type_name::<T>()
            );
        }
        Self { data: self.data }
    }
}
impl<T> Deref for UnsafeArc<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner().data
    }
}
impl<T> DerefMut for UnsafeArc<T> {
    fn deref_mut(&mut self) -> &mut T {
        black_box(self.get_mut())
    }
}
impl<T> Drop for UnsafeArc<T> {
    fn drop(&mut self) {
        if self.inner().references.fetch_sub(1, Release) != 1 {
            return;
        }
        atomic::fence(Acquire);

        unsafe {
            drop(Box::from_raw(self.data.as_ptr()));
        }
    }
}
