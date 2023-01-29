use crate::{
    api::CellsAPI,
    world::World,
    cells::CellType::*,
};

pub fn simulate_steps(world: &mut World, steps: u8) {
    let mut api = CellsAPI::new(world);
    for _ in 0..steps {
        api.advance_time();
        for y in (0..api.world.height).rev() {
            for x in 0..api.world.width {
                api.set_position(x, y);
                if api.current().material != Air {
                    let y = match api.cell_by_offset(0, -1).material {
                        Air => -1,
                        _ => 0,
                    };
                    api.swap_offset(0, y)
                }
            }
        }
    }
}