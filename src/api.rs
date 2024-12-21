use crate::cells::{Cell, CellType};
use crate::logic::simulate_steps;
use crate::world::World;
use std::cell::UnsafeCell;
use std::sync::Barrier;

const ITERATION_ORD_X: &str = match option_env!("SNAD_SIM_ORD_X") {
    Some(e) => e,
    None => "LINEAR",
};
const ITERATION_ORD_Y: &str = match option_env!("SNAD_SIM_ORD_Y") {
    Some(e) => e,
    None => "REVERSED",
};
// Each thread gets it's own copy
// Change shared grid for performance
pub struct CellsApi<'a> {
    pub pixels: &'a mut [u8],
    pub world: &'a mut World,
    pub main_thread_barrier: &'a Barrier,
    pub synchronize_barrier: &'a Barrier,
    pub none_cell: Cell,
    pub x: isize,
    pub y: isize,
}
impl<'a> CellsApi<'a> {
    pub fn new(shared: &'a mut SharedCellApi<'a>) -> CellsApi<'a> {
        Self {
            pixels: shared.pixels,
            world: shared.world,
            main_thread_barrier: &shared.main_barrier,
            synchronize_barrier: &shared.sync_barrier,
            none_cell: Cell::new(CellType::None),
            x: 0,
            y: 0,
        }
    }
    pub fn sync_threads(&mut self) {
        self.synchronize_barrier.wait();
    }
    pub fn wait_start(&mut self) {
        self.main_thread_barrier.wait();
    }
    #[inline]
    fn iter_axis(&mut self, iter_type: &'static str, start: usize, length: usize) -> Vec<usize> {
        let base = start..length;
        let mut result: Vec<usize> = if iter_type.starts_with("REVERSED") {
            base.rev().collect()
        } else if iter_type.starts_with("LINEAR") {
            base.collect()
        } else {
            panic!("{iter_type} is not a valid iteration type")
        };
        if iter_type.ends_with("SHUFFLED") {
            fastrand::shuffle(&mut result)
        }
        result
    }
    pub fn simulate(&mut self, x1: usize, x2: usize) {
        let iter_y = self.iter_axis(ITERATION_ORD_Y, 0, self.world.height);
        let iter_x = self.iter_axis(ITERATION_ORD_X, x1, x2);
        for y in iter_y {
            for x in &iter_x {
                self.set_position(*x, y);
                simulate_steps(self)
            }
        }
    }
    pub fn set_position(&mut self, x: usize, y: usize) {
        self.x = x as isize;
        self.y = y as isize;
    }
    pub fn set_cell(&mut self, x: isize, y: isize, cell: CellType) {
        let cell = Cell::new(cell);
        *self.cell_by_offset(x, y) = cell;
        let (x, y) = self.offset(x, y);
        let index = (self.world.width as isize * y + x) as usize;
        self.pixels[index * 4..index * 4 + 3].copy_from_slice(&cell.rgb);
    }
    #[inline]
    pub fn current(&mut self) -> &mut Cell {
        &mut self.world[(self.y as usize, self.x as usize)]
    }
    #[inline]
    pub fn in_bounds(&mut self, x: isize, y: isize) -> bool {
        y < self.world.height as isize && y >= 0 && x < self.world.width as isize && x >= 0
    }
    #[inline]
    fn offset(&mut self, x: isize, y: isize) -> (isize, isize) {
        (self.x + x, self.y - y)
    }
    #[inline]
    pub fn cell_by_offset(&mut self, x: isize, y: isize) -> &mut Cell {
        let (target_x, target_y) = self.offset(x, y);
        if !self.in_bounds(target_x, target_y) {
            self.none_cell = Cell::new(CellType::None);
            return &mut self.none_cell;
        }
        &mut self.world[(target_y as usize, target_x as usize)]
    }
    pub fn swap_offset(&mut self, x: isize, y: isize) {
        let (target_x, target_y) = self.offset(x, y);
        if !self.in_bounds(target_x, target_y) {
            return;
        }
        let current_index = self.y as usize * self.world.width + self.x as usize;
        let target_index = target_y as usize * self.world.width + target_x as usize;

        // Stop material being simulated twice in a single frame
        if self.world[current_index].updated == self.world.time {
            return;
        }
        self.world[current_index].updated = self.world.time;
        self.world[target_index].updated = self.world.time;
        // Slightly slower now, may be quicker when pixels are only stored in pixels
        // unsafe {
        //     let ptr_index = self.pixels.as_mut_ptr().add(current_index * 4);
        //     let ptr_target = self.pixels.as_mut_ptr().add(target_index * 4);

        //     std::ptr::swap_nonoverlapping(ptr_index, ptr_target, 3);
        // }
        self.pixels[target_index * 4..target_index * 4 + 3]
            .copy_from_slice(&self.world.grid[current_index].rgb);
        self.pixels[current_index * 4..current_index * 4 + 3]
            .copy_from_slice(&self.world.grid[target_index].rgb);
        self.world.grid.swap(current_index, target_index);
    }
    pub fn advance_time(&mut self) {
        self.world.time = self.world.time.wrapping_add(1)
    }
}

// Shared data between threads, preventing the race condition is up to the job of the programmer
pub struct SharedCellApi<'a> {
    pub pixels: &'a mut [u8],
    pub world: &'a mut World,
    pub main_barrier: Barrier,
    pub sync_barrier: Barrier,
}
impl<'a> SharedCellApi<'a> {
    pub fn new(world: &'a mut World, pixels: &'a mut [u8], threads: usize) -> Self {
        Self {
            world,
            pixels,
            main_barrier: Barrier::new(threads + 1),
            sync_barrier: Barrier::new(threads + 1),
        }
    }
}
// Allow for sharing mutable data between threads

pub struct UnsafeShared<'a, T> {
    data: UnsafeCell<T>,
    _marker: std::marker::PhantomData<&'a T>,
}

unsafe impl<'a, T> Send for UnsafeShared<'a, T> {}
unsafe impl<'a, T> Sync for UnsafeShared<'a, T> {}

impl<'a, T> UnsafeShared<'a, T> {
    pub fn new(t: T) -> UnsafeShared<'a, T> {
        Self {
            data: UnsafeCell::new(t),
            _marker: std::marker::PhantomData,
        }
    }
    pub fn get_api(&self) -> &'a mut T {
        unsafe { &mut *self.data.get() }
    }
}
