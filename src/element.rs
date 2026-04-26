pub struct ElementConfig {
    pub name: String,
    pub rgb_start: [u8; 3],
    pub rgb_end: [u8; 3],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CellType {
    Air,
    Sand,
    Water,
    Brick,
    Stone,
}

// Discreet cell to represent everything in the engine. Any type of fluid, solid
// or gas is either natively represented as this or converted if meant to ineract
// with other elements in the environment.
#[derive(Clone, Copy, Debug)]
pub struct Element {
    pub rgb: [u8; 3],
    pub material: CellType,

    // physics
    // Velocity is stepped with a modified Bresenham line algorithm
    // |vx|, vy| < 128 = 16 px/tick (8 sub pixels)
    // |sub_x|, |sub_y| < 8
    pub vx: i8,
    pub vy: i8,
    pub sub_x: i8,
    pub sub_y: i8,

    // game
    pub health: u8,
    pub temperature: f32,

    // hacky
    pub update_time: u32,
    pub update_index: u32, // if a cell moved multiple times, track the diff index
    pub body_id: u32,
}

impl Element {
    pub fn empty() -> Self {
        Self {
            rgb: [0, 0, 0],
            material: CellType::Air,
            vx: 0,
            vy: 0,
            sub_x: 0,
            sub_y: 0,
            health: 0,
            temperature: 0.0,
            update_time: 0,
            update_index: 0,
            body_id: 0,
        }
    }
}
