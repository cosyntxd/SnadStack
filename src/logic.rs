use crate::{api::CellsAPI, cells::CellType::*, world::World};
pub fn simulate_steps(world: &mut World, steps: u8) {
    let mut api = CellsAPI::new(world);
    let rng = fastrand::Rng::new();
    for _ in 0..steps {
        api.advance_time();
        for y in (0..api.world.height).rev() {
            for x in 0..api.world.width {
                api.set_position(x, y);
                if api.current().material != Air && api.current().material != Stone {
                    let x = rng.isize(-1..=1);
                    if api.cell_by_offset(0, -1).material == Air {
                        api.swap_offset(0, -1)
                    } else if api.cell_by_offset(x, -1).material == Air {
                        api.swap_offset(x, -1);
                    }
                }
            }
        }
    }
}
