use crate::cells::Cell;

pub struct World {
    pub grid: Vec<Vec<Cell>>,
    pub density: u32,
    pub width: usize,
    pub height: usize,
}
impl World {
    pub fn new(width: i32, height: i32, density: u32) -> Self {
        let height = height as usize;
        let width = width as usize;
        Self {
            grid: vec![vec![Default::default(); width]; height],
            density,
            width,
            height
        }
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width as usize;
        self.height = height as usize;
        self.grid.resize(self.height, Default::default());
        for row in self.grid.iter_mut() {
            row.resize_with(self.width, Default::default);
        }
    }
    pub fn render(&mut self, pixels: &mut [u8]) {
        for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
            let x = i % self.width;
            let y = i / self.width;
            let cell = &mut self.grid[y][x];
            pixel[0..3].copy_from_slice(&cell.rgb);
            pixel[3] = 255 - (cell.selected as u8 * 96);
            cell.selected = false;
        }
    }
}