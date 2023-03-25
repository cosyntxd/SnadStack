#[derive(Clone, Copy)]
pub struct Cell {
    pub material: CellType,
    pub rgb: [u8; 3],
    pub selected: bool,
    pub updated: u16,
}
impl Default for Cell {
    fn default() -> Self {
        Self {
            material: CellType::Air,
            rgb: Self::rgb_ranges(CellType::Air),
            selected: false,
            updated: 0,
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
    pub fn rgb_ranges(material: CellType) -> ([u8; 3]) {
        let random_variance = fastrand::i16(0..=100);

        let (rgb_start, rgb_end) = match material {
            CellType::Air => ([125, 201, 255], [125, 201, 255]),
            CellType::Water => ([76, 153, 243], [104, 175, 253]),
            CellType::Sand => ([220, 180, 116], [204, 164, 100]),
            CellType::Stone => ([131, 143, 134], [110, 122, 113]),
            _ => ([255, 155, 61], [117, 36, 81]),
        };

        let mut rgb = [0, 0, 0];
        rgb.iter_mut().enumerate().for_each(|(index, value)| {
            let rgba_diff = rgb_end[index] - rgb_start[index];
            let rgba_change = rgba_diff * random_variance / 100;
            *value = (rgb_start[index] + rgba_change) as u8;
        });
        rgb
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CellType {
    None,
    Air,
    Water,
    Sand,
    Stone,
}
