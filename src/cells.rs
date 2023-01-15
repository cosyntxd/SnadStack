#[derive(Clone, Copy)]
pub struct Cell {
    pub material: CellType,
    pub rgb: [u8; 3],
    pub selected: bool,
}
impl Default for Cell {
    fn default() -> Self {
        Self {
            material: CellType::Air,
            rgb: Self::rgb_ranges(CellType::Air),
            selected: false,
        }
    }
}

impl Cell {
    pub fn new(material: CellType) -> Self {
        let rgb = Self::rgb_ranges(material);
        Self {
            material,
            rgb,
            ..Default::default()
        }
    }
    pub fn rgb_ranges(material: CellType) -> ([u8; 3]){
        match material {
            CellType::Air =>   [125, 201, 253],
            CellType::Sand =>  [220, 180, 116],
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum CellType {
    Air,
    Sand,
}