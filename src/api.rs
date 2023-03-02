use std::ptr;
use crate::cells::{
    Cell,
    NONE_CELL
};
use crate::world::World;

pub struct CellsAPI<'a> {
    pub x: isize,
    pub y: isize,
    pub width: isize,
    pub height: isize,
    pub world: &'a mut World,
}
impl<'a> CellsAPI<'a> {
    pub fn new(world: &'a mut World) -> Self{
        Self {
            x: 0,
            y: 0,
            width: world.width as isize,
            height: world.height as isize,
            world,
        }
    }
    pub fn set_position(&mut self, x: usize, y: usize) {
        self.x = x as isize;
        self.y = y as isize;
    }
    pub fn current(&mut self) -> &mut Cell {
        &mut self.world.grid[self.y as usize * self.world.width + self.x as usize]
    }
    pub fn in_bounds(&mut self, x: isize, y: isize) -> bool{
        y < self.height && y >= 0 && x < self.width && x >= 0
    }
    fn offset(&mut self, x: isize, y: isize) -> (isize, isize) {
        (self.x + x, self.y - y)
    }
    pub fn cell_by_offset(&mut self, x: isize, y: isize) -> &Cell {
        let (target_x, target_y) = self.offset(x, y);
        if !self.in_bounds(target_x, target_y)  {
            return &NONE_CELL;
        }
        &mut self.world.grid[target_y as usize * self.world.width + target_x as usize]
    }
    pub fn swap_offset(&mut self, x: isize, y: isize){
        let (target_x, target_y) = self.offset(x, y);
        if !self.in_bounds(target_x, target_y)  {
            return
        }
        unsafe {
            let current: *mut Cell = &mut self.world.grid[self.y as usize * self.world.width + self.x as usize];
            let target: *mut Cell = &mut self.world.grid[target_y as usize * self.world.width + target_x as usize];
            // Stop material being simulated twice in a single frame
            if (*current).updated == self.world.time{
                return
            }
            (*current).updated = self.world.time;
            (*target).updated = self.world.time;
            ptr::swap(current, target);
        }
    }
    pub fn advance_time(&mut self) {
        self.world.time = self.world.time.wrapping_add(1)
    }
}