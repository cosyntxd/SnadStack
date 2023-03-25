use crate::cells::{Cell, CellType};
use crate::logic::simulate_steps;
use crate::world::World;
use std::cell::UnsafeCell;
use std::sync::Barrier;

// Each thread gets it's own copy
// Change shared grid for performance
pub struct CellsApi<'a> {
    pub pixels: &'a mut [u8],
    pub world: &'a mut World,
    pub barrier: &'a Barrier,
    pub none_cell: Cell,
    pub x: isize,
    pub y: isize,
}
impl<'a> CellsApi<'a> {
    pub fn new(shared: &'a mut SharedCellApi<'a>) -> CellsApi<'a> {
        Self {
            pixels: shared.pixels,
            world: shared.world,
            barrier: &shared.barrier,
            none_cell: Cell::new(CellType::None),
            x: 0,
            y: 0,
        }
    }
    pub fn simulate(&mut self, x1: isize, x2: isize) {
        for y in (0..self.world.height).rev() {
            for x in x1..x2 {
                self.set_position(x as usize, y as usize);
                simulate_steps(self)
            }
        }
    }
    pub fn set_position(&mut self, x: usize, y: usize) {
        self.x = x as isize;
        self.y = y as isize;
    }
    pub fn current(&mut self) -> &mut Cell {
        &mut self.world.grid[self.y as usize * self.world.width + self.x as usize]
    }
    pub fn in_bounds(&mut self, x: isize, y: isize) -> bool {
        y < self.world.height as isize && y >= 0 && x < self.world.width as isize && x >= 0
    }
    fn offset(&mut self, x: isize, y: isize) -> (isize, isize) {
        (self.x + x, self.y - y)
    }
    pub fn cell_by_offset(&mut self, x: isize, y: isize) -> &mut Cell {
        let (target_x, target_y) = self.offset(x, y);
        if !self.in_bounds(target_x, target_y) {
            self.none_cell = Cell::new(CellType::None);
            return &mut self.none_cell;
        }
        &mut self.world.grid[target_y as usize * self.world.width + target_x as usize]
    }
    pub fn swap_offset(&mut self, x: isize, y: isize) {
        let (target_x, target_y) = self.offset(x, y);
        if !self.in_bounds(target_x, target_y) {
            return;
        }
        let current_index = self.y as usize * self.world.width + self.x as usize;
        let target_index = target_y as usize * self.world.width + target_x as usize;

        // Stop material being simulated twice in a single frame
        if self.world.grid[current_index].updated == self.world.time {
            return;
        }
        self.world.grid[current_index].updated = self.world.time;
        self.world.grid[target_index].updated = self.world.time;

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
    pub barrier: Barrier,
}
impl<'a> SharedCellApi<'a> {
    pub fn new(world: &'a mut World, pixels: &'a mut [u8], threads: usize) -> Self {
        Self {
            world,
            pixels,
            barrier: Barrier::new(threads),
        }
    }
}
// Allow for sharing mutable data between threads
pub struct UnsafeShared<'a> {
    data: UnsafeCell<SharedCellApi<'a>>,
}

unsafe impl<'a> Send for UnsafeShared<'a> {}
unsafe impl<'a> Sync for UnsafeShared<'a> {}

impl<'a> UnsafeShared<'a> {
    pub const fn new(t: SharedCellApi<'a>) -> UnsafeShared<'a> {
        UnsafeShared {
            data: UnsafeCell::new(t),
        }
    }
    pub fn get_api(&self) -> CellsApi<'a> {
        CellsApi::new(unsafe { &mut *self.data.get() })
    }
}
