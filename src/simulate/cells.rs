include!(concat!(env!("OUT_DIR"), "/cell_settings.rs"));

// #[repr(packed)]
#[derive(Clone, Copy)]
pub struct Cell {
    pub material: CellType,
    pub rgb: [u8; 3],
    pub selected: bool,
    pub updated: u8,
    pub discolored: bool,
    pub health: u16,
    pub lifespan: u16,
}
impl Cell {
    pub fn new(material: CellType) -> Self {
        let rgb = Self::rgb_ranges(material);
        Self {
            material,
            rgb,
            selected: false,
            updated: 0,
            // velocity_x: 0,
            // velocity_y: 0,
            health: 0,
            discolored: false,
            lifespan: 0,
            // position_x: 0,
            // position_y: 0,
        }
    }

    pub fn rgb_ranges(material: CellType) -> [u8; 3] {
        let random_variance = fastrand::i16(0..=100);

        let [rgb_start, rgb_end] = material.color();

        let mut rgb = [0, 0, 0];
        rgb.iter_mut().enumerate().for_each(|(index, value)| {
            let rgba_diff = rgb_end[index] as i16 - rgb_start[index] as i16;
            let rgba_change = rgba_diff as i16 * random_variance / 100;
            *value = (rgb_start[index] as i16 + rgba_change as i16) as u8;
        });
        rgb
    }
    pub fn cfg_switch(&mut self, bool: bool) {
        *self = match bool {
            _ => Self::new(CellType::Air),
            _ => *self,
        };
    }
}
