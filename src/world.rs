use crate::{
    api::{CellsApi, SharedCellApi, UnsafeShared},
    cells::{Cell, CellType},
    input::CenterLocation,
};
use std::{
    ops::{Index, IndexMut},
    sync::Arc,
    thread,
};

pub struct World {
    pub grid: Vec<Cell>,
    pub density: u32,
    pub width: usize,
    pub height: usize,
    pub time: u8,
}
impl World {
    pub fn new(width: i32, height: i32, density: u32) -> Self {
        let height = (height as usize) / density as usize;
        let width = (width as usize) / density as usize;
        let grid = vec![Cell::new(CellType::Air); width * height];
        Self {
            grid,
            density,
            width,
            height,
            time: 0,
        }
    }
    pub fn resize(&mut self, width: usize, height: usize, offsets: CenterLocation) {
        let mut new_grid = vec![Cell::new(CellType::Air); (width * height) as usize];
        // TODO: smarter resize
        self.grid
            .chunks_exact(self.width)
            .zip(new_grid.chunks_exact_mut(width as usize))
            .for_each(|(old, new)| {
                for (new_element, old_element) in new.iter_mut().zip(old.iter()) {
                    *new_element = *old_element;
                }
            });
        self.grid = new_grid;
        self.width = width;
        self.height = height;
    }

    pub fn draw_thick_line(
        &mut self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        radius: isize,
        material: CellType,
        place: bool,
        hover: bool,
        pixels: &mut [u8],
    ) {
        let line =
            bresenham::Bresenham::new((x1 as isize, y1 as isize), (x2 as isize, y2 as isize))
                .chain(std::iter::once((x2 as isize, y2 as isize))) // Manually add endpoint
                .collect::<Vec<(isize, isize)>>();

        let diameter = radius * 2;
        for index_y in 0..diameter {
            for index_x in 0..diameter {
                let distance_non_sqrt = (index_x - radius).pow(2) + (index_y - radius).pow(2);

                if distance_non_sqrt >= radius.pow(2) {
                    continue;
                }
                let math = ((x2 - x1) as i32) * (index_x - radius) as i32
                    + ((y2 - y1) as i32 * (index_y - radius) as i32)
                    < 0;
                let corner = math || radius == 1;

                if (distance_non_sqrt as f64).sqrt() + 1.5 >= radius as f64 && corner {
                    for point in &line {
                        let x = (point.0 as isize + index_x - radius)
                            .clamp(0, self.width as isize - 1)
                            as usize;

                        let y = (point.1 as isize + index_y - radius)
                            .clamp(0, self.height as isize - 1)
                            as usize;

                        self.place_tile(x, y, material, pixels, hover, place)
                    }
                } else {
                    let x =
                        (x2 as isize + index_x - radius).clamp(0, self.width as isize - 1) as usize;
                    let y = (y2 as isize + index_y - radius).clamp(0, self.height as isize - 1)
                        as usize;

                    self.place_tile(x, y, material, pixels, hover, place)
                }
            }
        }
    }
    fn place_tile(
        &mut self,
        x: usize,
        y: usize,
        material: CellType,
        pixels: &mut [u8],
        hover: bool,
        place: bool,
    ) {
        let index = y * self.width + x;
        let cell = &mut self[index];
        unsafe {
            if place {
                *cell = Cell::new(material);
                pixels
                    .get_unchecked_mut(index * 4..index * 4 + 3)
                    .copy_from_slice(&cell.rgb);
            }
            if hover {
                pixels[index * 4 + 3] = 200;
            } else {
                pixels[index * 4 + 3] = 255;
            }
        }
    }
    pub fn simulate(&mut self, steps: u16, pixels: &mut [u8]) {
        let width = self.width; // Will complain about use after borrow without this
        let left_edge_random = fastrand::usize(0..width / 16 + 1);
        let chunk_width = width as usize / 6;
        let parallelize_chunks = (left_edge_random..width).step_by(chunk_width);
        let arc_api = Arc::new(UnsafeShared::new(SharedCellApi::new(
            self,
            pixels,
            parallelize_chunks.len(),
        )));
        // At low sizes, the overhead of managing multiple threads becomes too large.
        // Also, wasm threads are weird, so I prefer to not deal with them
        if width < 100 || cfg!(target_arch = "wasm32") {
            for _ in 0..steps {
                let mut api = CellsApi::new(arc_api.get_api());
                api.advance_time();
                api.simulate(0, width);
            }
        } else {
            // Main thread, in charge of advancing the time
            let arc_api = Arc::clone(&arc_api);
            thread::scope(|s| {
                s.spawn(|| {
                    for _ in 0..steps {
                        let mut api = CellsApi::new(arc_api.get_api());
                        api.advance_time();
                        api.wait_start();
                        api.simulate(0, left_edge_random);
                        api.sync_threads();
                    }
                });
                for chunk_start in parallelize_chunks {
                    let chunk_end = (chunk_start + chunk_width).min(width);
                    let is_rightmost = chunk_end == width;
                    let arc_api = Arc::clone(&arc_api);
                    s.spawn(move || {
                        for _ in 0..steps {
                            let mut api = CellsApi::new(arc_api.get_api());
                            api.wait_start();
                            if is_rightmost {
                                api.simulate(chunk_end - 15, chunk_end);
                                api.sync_threads();
                                api.simulate(chunk_start, chunk_end - 15);
                            } else {
                                api.sync_threads();
                                api.simulate(chunk_start, chunk_end);
                            }
                        }
                    });
                }
            });
        }
    }
    pub fn render(&mut self, pixels: &mut [u8]) {
        for y in 0..self.height {
            for x in 0..self.width {
                let index = y * self.width + x;
                let cell = &mut self[index];
                let pixel = &mut pixels[index * 4..index * 4 + 4];
                pixel[0..3].copy_from_slice(&cell.rgb);
                pixel[3] = 255;
            }
        }
    }
}
impl Index<(usize, usize)> for World {
    type Output = Cell;
    fn index(&self, index: (usize, usize)) -> &Self::Output {
        let (y, x) = index;
        unsafe { self.grid.get_unchecked(y * self.width + x) }
    }
}

impl IndexMut<(usize, usize)> for World {
    fn index_mut(&mut self, index: (usize, usize)) -> &mut Self::Output {
        let (y, x) = index;
        unsafe { self.grid.get_unchecked_mut(y * self.width + x) }
    }
}
// Many places use a single index for lookups for better readability
// Much less confusing when multiple locations are invovled
impl Index<usize> for World {
    type Output = Cell;
    fn index(&self, index: usize) -> &Self::Output {
        unsafe { self.grid.get_unchecked(index) }
    }
}
impl IndexMut<usize> for World {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        unsafe { self.grid.get_unchecked_mut(index) }
    }
}
