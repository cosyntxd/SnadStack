use crate::{api::CellsAPI, cells::CellType::*};
pub fn simulate_steps(api: &mut CellsAPI) {
    match api.current().material {
        Sand => {
            let x = fastrand::isize(-1..=1);
            if api.cell_by_offset(0, -1).material == Air
                || api.cell_by_offset(0, -1).material == Water && fastrand::bool()
            {
                api.swap_offset(0, -1)
            } else if api.cell_by_offset(x, -1).material == Air
                || api.cell_by_offset(x, -1).material == Water && fastrand::bool()
            {
                api.swap_offset(x, -1);
            }
        }
        Water => {
            let x = fastrand::isize(-3..=3);
            if api.cell_by_offset(0, -1).material == Air {
                api.swap_offset(0, -1)
            } else if api.cell_by_offset(x, -1).material == Air {
                api.swap_offset(x, -1);
            } else if api.cell_by_offset(x, 0).material == Air {
                api.swap_offset(x, 0);
            }
        }
        Stone => {}
        _ => {}
    }
}
