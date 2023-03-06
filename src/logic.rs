use crate::{api::CellsAPI, cells::CellType::*};
pub fn simulate_steps(api: &mut CellsAPI) {
    api.advance_time();
    for y in (0..api.world.height).rev() {
        for x in 0..api.world.width {
            api.set_position(x, y);
            if api.current().material != Air && api.current().material != Stone {
                let x = fastrand::isize(-1..=1);
                if api.cell_by_offset(0, -1).material == Air {
                    api.swap_offset(0, -1)
                } else if api.cell_by_offset(x, -1).material == Air {
                    api.swap_offset(x, -1);
                }
            }
        }
    }
}
