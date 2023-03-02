use crate::{
    cells::{Cell, CellType},
    logic::simulate_steps,
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
        let grid: Vec<Cell> = vec![Default::default(); (width * height) as usize];

        self.width = width as usize;
        self.height = height as usize;

        self.grid = grid;
    }

    pub fn place_circle(&mut self, x: usize, y: usize, radius: isize, material: CellType, place: bool) {
        let diameter = radius*2;
        for index_y in 0..diameter {
            for index_x in 0..diameter {
                let distance_squared = (index_x - radius).pow(2) + (index_y - radius).pow(2);
                let in_circle = distance_squared < radius.pow(2);
                if in_circle {
                    let x = (index_x+x as isize - radius).clamp(0, self.width as isize -1) as usize;
                    let y = (index_y+y as isize - radius).clamp(0, self.height as isize -1) as usize;
                    let mut cell = &mut self.grid[y * self.width + x];
                    if place {
                        *cell = Cell::new(material);
                    }
                    cell.selected = true; 
                }
            }
        }
    }
    pub fn simulate(&mut self, steps: u8) {
        simulate_steps(self, steps)
    }
    pub fn render(&mut self, pixels: &mut [u8]) {
        for y in 0..self.height {
            for x in 0..self.width {
                let index = x + y * self.width;
                let cell = &mut self.grid[index];
                let pixel = &mut pixels[4 * index..4 * index + 4];
                pixel[0..3].copy_from_slice(&cell.rgb);
                pixel[3] = 255 - (cell.selected as u8 * 96);
                cell.selected = false;
            }
        }
    }
}