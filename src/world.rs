use crate::{
    api::{SharedCellApi, UnsafeShared},
    cells::{Cell, CellType},
};
use std::{sync::Arc, thread};

pub struct World {
    pub grid: Vec<Cell>,
    pub density: u32,
    pub width: usize,
    pub height: usize,
    pub time: u16,
}
impl World {
    pub fn new(width: i32, height: i32, density: u32) -> Self {
        let height = (height as usize) / density as usize;
        let width = (width as usize) / density as usize;
        let grid = vec![Default::default(); width * height];
        Self {
            grid,
            density,
            width,
            height,
            time: 0,
        }
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        let mut grid: Vec<Cell> = vec![Default::default(); (width * height) as usize];
        self.grid
            .chunks_exact(self.width)
            .zip(grid.chunks_exact_mut(width as usize))
            .for_each(|(old, new)| {
                for (new_element, old_element) in new.iter_mut().zip(old.iter()) {
                    *new_element = *old_element;
                }
            });
        self.width = width as usize;
        self.height = height as usize;

        self.grid = grid;
    }

    pub fn place_circle(
        &mut self,
        x: usize,
        y: usize,
        radius: isize,
        material: CellType,
        place: bool,
        pixels: &mut [u8],
    ) {
        let diameter = radius * 2;
        for index_y in 0..diameter {
            for index_x in 0..diameter {
                let distance_squared = (index_x - radius).pow(2) + (index_y - radius).pow(2);
                let in_circle = distance_squared < radius.pow(2);
                if in_circle {
                    let x =
                        (index_x + x as isize - radius).clamp(0, self.width as isize - 1) as usize;
                    let y =
                        (index_y + y as isize - radius).clamp(0, self.height as isize - 1) as usize;
                    let index = y * self.width + x;
                    let cell = &mut self.grid[index];
                    if place {
                        *cell = Cell::new(material);
                        pixels[index * 4..index * 4 + 3].copy_from_slice(&cell.rgb)
                    }
                    // (*cell).selected = true; // New rendering system breaks this
                }
            }
        }
    }
    pub fn simulate(&mut self, steps: u16, pixels: &mut [u8]) {
        let arc_api = Arc::new(UnsafeShared::new(SharedCellApi::new(self, pixels)));
        let arc_0 = Arc::clone(&arc_api);

        let mut api = arc_0.get_api();
        api.advance_time();

        thread::scope(move |s| {
            let arc_1 = Arc::clone(&arc_api);
            let arc_2 = Arc::clone(&arc_api);

            s.spawn(move || {
                let mut api = arc_1.get_api();
                let pos = api.world.width as isize / 2;
                api.simulate(0, pos)
            });

            s.spawn(move || {
                let mut api = arc_2.get_api();
                let pos = api.world.width as isize / 2;
                api.simulate(pos, api.world.width as isize)
            });
        });
    }

    pub fn render(&mut self, pixels: &mut [u8]) {
        for y in 0..self.height {
            for x in 0..self.width {
                let index = y * self.width + x;
                let cell = &mut self.grid[index];
                let pixel = &mut pixels[index * 4..index * 4 + 4];
                pixel[0..3].copy_from_slice(&cell.rgb);
                pixel[3] = 255 - (cell.selected as u8 * 96);
                cell.selected = false;
            }
        }
    }
}
