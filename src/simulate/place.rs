use super::{
    sharing::{UnsafeArc, UnsafeShared},
    world::{self, SimWorldApi},
};
use crate::simulator::cells::Cell;
use smallvec::smallvec;
use smallvec::SmallVec;
use std::{
    cell::UnsafeCell,
    fmt,
    marker::PhantomData,
    ptr::NonNull,
    sync::{mpsc::Sender, Arc, Condvar, Mutex, RwLock},
    vec,
};

const THREADING_START_THRESHOLD_WORK: usize = 100_000;
const THREADING_TOO_CHEAP: usize = 1_0000;
pub struct LineDrawTasks {
    tasks: Arc<RwLock<Vec<UnsafeShared<PlaceLineTask>>>>,
    blocking: Arc<(Mutex<bool>, Condvar)>,
}

impl LineDrawTasks {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(vec![])),
            blocking: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }
    pub fn add_task(&mut self, task: PlaceLineTask) {
        self.tasks.write().unwrap().push(UnsafeShared::new(task));
    }
    pub fn estimate_work(&self) -> usize {
        self.tasks
            .read()
            .unwrap()
            .iter()
            .map(|task| task.variant.estimate_compute_work())
            .sum()
    }
    fn conflicts_with_active_task(&self, task: &PlaceLineTask) -> bool {
        self.tasks
            .read()
            .unwrap()
            .iter()
            .any(|queued| !queued.unstarted() && queued.intersects(task))
    }

    pub fn remove_task(&mut self, _regions: Vec<BoundingBox>) -> Option<LineDrawTask> {
        loop {
            let mut tasks_inner = self.tasks.write().unwrap();
            let available_task = tasks_inner
                .iter_mut()
                .find(|task| task.unstarted() && !self.conflicts_with_active_task(task));
            if let Some(task) = available_task {
                return Some(LineDrawTask::new(task.get_ptr(), self.blocking.clone()));
            }
            if tasks_inner.iter().filter(|task| !task.unstarted()).count() == 0 {
                return None;
            }
            // release lock
            drop(tasks_inner);
            // wait for a wakeup
            let (lock, cvar) = &*self.blocking;
            let mut started = lock.lock().unwrap();
            while !*started {
                started = cvar.wait(started).unwrap();
            }
        }
    }
    pub fn delete_completed(&mut self) {
        self.tasks
            .write()
            .unwrap()
            .retain(|val| val.state() == TaskState::Completed)
    }
}

pub struct LineDrawTask {
    pub task: NonNull<PlaceLineTask>,
    waker: Arc<(Mutex<bool>, Condvar)>,
}

impl LineDrawTask {
    pub fn new(task: NonNull<PlaceLineTask>, waker: Arc<(Mutex<bool>, Condvar)>) -> Self {
        LineDrawTask { task, waker }
    }
    pub fn execute(mut self, world: &mut SimWorldApi) {
        unsafe { self.task.as_mut() }.run(world);
    }
    fn finish(&self) {
        let (lock, cvar) = self.waker.as_ref();
        let mut started = lock.lock().unwrap();
        *started = true;
        cvar.notify_one();
    }
}
impl Drop for LineDrawTask {
    fn drop(&mut self) {
        unsafe { self.task.as_mut().set_state(TaskState::Completed) };
        self.finish();
    }
}

#[derive(Clone, Debug)]
pub enum PlaceLineType {
    Rectangle(BoundingBox),
    Circle(Point<usize>, usize),
    Lines(Vec<Point<usize>>, usize),
}

impl PlaceLineType {
    pub fn bounding_box(&self) -> BoundingBox {
        match self {
            PlaceLineType::Rectangle(bounding_box) => bounding_box.clone(),
            PlaceLineType::Circle(point, radius) => {
                BoundingBox::point(point.isize()).expand(*radius)
            }
            PlaceLineType::Lines(points, radius) => {
                assert!(points.len() >= 2);
                let mut bounds = BoundingBox::point(points[0].isize());
                for point in points {
                    bounds.merge(&BoundingBox::point(point.isize()));
                }
                bounds.expand(*radius)
            }
        }
    }
    pub fn estimate_compute_work(&self) -> usize {
        self.bounding_box().area()
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TaskPlaceAction {
    PlaceCell(Cell),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TaskState {
    Unstarted,
    InProgress,
    Completed,
}

#[derive(Clone, Debug)]
pub struct PlaceLineTask {
    dirty_rect: BoundingBox,
    variant: PlaceLineType,
    action: TaskPlaceAction,
    state: TaskState,
}
impl PlaceLineTask {
    pub fn new(variant: PlaceLineType, action: TaskPlaceAction) -> Self {
        Self {
            dirty_rect: variant.bounding_box(),
            variant,
            action,
            state: TaskState::Unstarted,
        }
    }
    pub fn run(&mut self, world: &mut SimWorldApi) {
        debug_assert!(self.state == TaskState::InProgress);
        if self.dirty_rect.verify_corners() == false {
            panic!("{:?}", self.dirty_rect);
        }
        match &self.variant {
            PlaceLineType::Rectangle(bounding_box) => {
                for y in bounding_box.bottom_left.y..bounding_box.top_right.y {
                    for x in bounding_box.bottom_left.x..bounding_box.top_right.x {
                        world[(y as usize, x as usize)] = Cell::new();
                    }
                }
            }
            PlaceLineType::Circle(point, radius) => {
                let point = point.isize();
                for (y, x) in BoundingBox::point(point).expand(*radius).iter_2d() {
                    if ((point.y - y).pow(2) + (point.x - x).pow(2)) as usize <= radius.pow(2) {
                        world[(y as usize, x as usize)] = Cell::new();
                    }
                }
            }
            // todo!() slow as fuck
            PlaceLineType::Lines(vec, radius) => {
                while let Some(window) = vec.windows(2).next() {
                    let (p1, p2) = (window[0], window[1]);
                    for (px, py) in
                        bresenham::Bresenham::new(p1.isize().inner(), p2.isize().inner())
                    {
                        for (y, x) in BoundingBox::point(Point::new(px, py))
                            .expand(*radius)
                            .iter_2d()
                        {
                            if ((py - y).pow(2) + (px - x).pow(2)) as usize <= radius.pow(2) {
                                world[(y as usize, x as usize)] = Cell::new();
                            }
                        }
                    }
                }
            }
        }
        self.set_state(TaskState::Completed)
    }
    pub fn intersects(&self, other: &PlaceLineTask) -> bool {
        self.dirty_rect.intersects(&other.dirty_rect)
    }
    pub fn set_state(&mut self, state: TaskState) {
        self.state = state;
    }
    pub fn state(&self) -> TaskState {
        self.state
    }
    pub fn unstarted(&self) -> bool {
        self.state == TaskState::Unstarted
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundingBox {
    bottom_left: Point<isize>,
    top_right: Point<isize>,
}

impl BoundingBox {
    pub fn new(bl: Point<isize>, tr: Point<isize>) -> Self {
        BoundingBox {
            bottom_left: bl,
            top_right: tr,
        }
    }
    pub fn point(point: Point<isize>) -> Self {
        BoundingBox {
            bottom_left: point,
            top_right: point,
        }
    }
    pub fn merge(&mut self, other: &BoundingBox) {
        self.bottom_left.x = self.bottom_left.x.min(other.bottom_left.x);
        self.bottom_left.y = self.bottom_left.y.min(other.bottom_left.y);

        self.top_right.x = self.top_right.x.max(other.top_right.x);
        self.top_right.y = self.top_right.y.max(other.top_right.y);
    }
    pub fn verify_corners(&self) -> bool {
        let x = self.bottom_left.x <= self.top_right.x;
        let y = self.bottom_left.y <= self.top_right.y;
        x && y
    }
    pub fn intersects(&self, other: &BoundingBox) -> bool {
        let x = self.bottom_left.x < other.top_right.x && self.top_right.x > other.bottom_left.x;
        let y = self.bottom_left.y < other.top_right.y && self.top_right.y > other.bottom_left.y;
        x && y
    }
    pub fn expand(&self, amount: usize) -> BoundingBox {
        let amount = amount as isize;
        BoundingBox::new(
            Point::new(
                self.bottom_left.x.saturating_sub(amount),
                self.bottom_left.y.saturating_sub(amount),
            ),
            Point::new(
                self.bottom_left.x.saturating_sub(amount),
                self.bottom_left.y.saturating_sub(amount),
            ),
        )
    }
    pub fn clamp(&mut self, bounds: &BoundingBox) -> bool {
        let start = self.clone();
        self.top_right.y = self
            .top_right
            .y
            .clamp(bounds.bottom_left.y, bounds.top_right.y);
        self.bottom_left.y = self
            .bottom_left
            .y
            .clamp(bounds.bottom_left.y, bounds.top_right.y);

        self.top_right.x = self
            .top_right
            .x
            .clamp(bounds.bottom_left.x, bounds.top_right.x);
        self.bottom_left.x = self
            .bottom_left
            .x
            .clamp(bounds.bottom_left.x, bounds.top_right.x);
        start != *self
    }
    pub fn iter_2d<'a>(&'a self) -> impl Iterator<Item = (isize, isize)> + 'a {
        (self.bottom_left.y..self.top_right.y)
            .flat_map(move |y| (self.bottom_left.x..self.top_right.x).map(move |x| (x, y)))
    }
    pub fn area(&self) -> usize {
        let w = self.top_right.x - self.bottom_left.x;
        let h = self.top_right.y - self.bottom_left.y;
        (w * h) as usize
    }
    pub fn verticies(&self, expand: f32) -> [[(f32, f32); 3]; 2] {
        let left_x = self.bottom_left.x as f32 - expand;
        let bottom_y = self.bottom_left.y as f32 - expand;
        let right_x = self.top_right.x as f32 + expand;
        let top_y = self.top_right.y as f32 + expand;

        let triangle_1 = [
            (right_x, bottom_y), // BR
            (left_x, top_y),     // TL
            (left_x, bottom_y),  // BL
        ];

        let triangle_2 = [
            (right_x, bottom_y), // BR
            (left_x, top_y),     // TL
            (right_x, top_y),    // TR
        ];

        [triangle_1, triangle_2]
    }
}

/// Bottom left origin
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Point<T> {
    pub x: T,
    pub y: T,
}
impl<T: Copy> Point<T> {
    #[inline]
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
    pub fn inner(&self) -> (T, T) {
        (self.x, self.y)
    }
}
impl Point<usize> {
    pub fn isize(self) -> Point<isize> {
        Point {
            x: self.x as isize,
            y: self.y as isize,
        }
    }
}
impl Point<isize> {
    pub fn usize(self) -> Point<usize> {
        Point {
            x: self.x as usize,
            y: self.y as usize,
        }
    }
}
impl<T: fmt::Display> fmt::Display for Point<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Point {{ x: {}, y: {} }}", self.x, self.y)
    }
}
