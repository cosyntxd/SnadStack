use crate::{
    api::CellsApi,
    cells::CellType::{self, *},
};
pub fn simulate_steps(api: &mut CellsApi) {
    for i in 0..2 {}
    match api.current().material {
        Sand => {
            let x = fastrand::isize(-1..=1);
            if api.cell_by_offset(0, -1).material == Air
                || matches!(api.cell_by_offset(0, -1).material, Water | Oil) && fastrand::bool()
            {
                api.swap_offset(0, -1)
            } else if api.cell_by_offset(x, -1).material == Air
                || matches!(api.cell_by_offset(x, -1).material, Water | Oil) && fastrand::bool()
            {
                api.swap_offset(x, -1);
            }
        }
        Water => {
            let x = fastrand::isize(-5..=5);
            if matches!(api.cell_by_offset(0, -1).material, Air | Gas)
                || (api.cell_by_offset(0, -1).material == Oil && fastrand::bool())
            {
                api.swap_offset(0, -1)
            } else if matches!(api.cell_by_offset(x, -1).material, Air | Oil | Gas | Fire) {
                api.swap_offset(x, -1);
            } else if matches!(api.cell_by_offset(x, 0).material, Air | Oil | Gas | Fire) {
                api.swap_offset(x, 0);
            }
        }
        Cloner => {
            let mut to_clone = Air;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let target = api.cell_by_offset(dx, dy).material;
                    if target == Air {
                        api.set_cell(dx, dy, to_clone);
                    } else if !matches!(target, Cloner | Stone | Gas) {
                        to_clone = target
                    }
                }
            }
        }
        Wood => {
            if api.current().discolored == true {
                let dx = fastrand::isize(-15..15) / 8;
                let dy = fastrand::isize(-15..15) / 8;
                if fastrand::u8(0..8) < 3 {
                    if api.cell_by_offset(dx, dy).material == Air {
                        api.set_cell(dx, dy, Fire)
                    }
                }
            }
        }
        Fire => {
            let mut c = api.current();
            c.health += 1;
            if c.health > fastrand::u16(16..2048) {
                api.set_cell(0, 0, Gas);
            } else {
                let dx = fastrand::isize(-15..=15) / 8;
                let dy = fastrand::isize(-20..=25) / 8;
                let target = api.cell_by_offset(dx, dy);
                if target.material == Water {
                    api.set_cell(0, 0, Gas);
                    return;
                }
                if matches!(target.material, Wood | Oil) {
                    target.health += 5;
                    if target.health > fastrand::u16(1..512) {
                        api.set_cell(dx, dy, Fire);
                    } else {
                        target.discolored = true;
                    }
                }
                if api.cell_by_offset(dx, dy).material == Air {
                    api.swap_offset(dx, dy);
                }
            }
        }
        Oil => {
            let x = fastrand::isize(-5..=5);
            if api.current().discolored == true {
                api.current().health += 1;
                let dx = fastrand::isize(-15..=15) / 8;
                let dy = fastrand::isize(-15..=15) / 8;
                if fastrand::u8(0..8) < 3 {
                    if api.cell_by_offset(dx, dy).material == Air {
                        api.set_cell(dx, dy, Fire)
                    }
                }
            }
            if matches!(api.cell_by_offset(0, -1).material, Air | Fire | Gas) {
                api.swap_offset(0, -1)
            } else if matches!(api.cell_by_offset(x, -1).material, Air | Fire | Gas) {
                api.swap_offset(x, -1);
            } else if matches!(api.cell_by_offset(x, 0).material, Air | Fire | Gas) {
                api.swap_offset(x, 0);
            }
        }
        Gas => {
            let mut c = api.current();
            c.health += 1;
            if c.health > fastrand::u16(8..1024) {
                api.set_cell(0, 0, Air);
            } else {
                let dx = fastrand::isize(-9..=9) / 8;
                let dy = fastrand::isize(-8..26) / 8;
                let target = api.cell_by_offset(dx, dy);
                if target.material == Air {
                    api.swap_offset(dx, dy);
                    return;
                }
                if target.material == Fire {
                    api.set_cell(0, 0, Air)
                }
            }
        }
        Lava => {
            let mut c = api.current();
            let x = fastrand::isize(-4..=4);
            if api.cell_by_offset(0, -1).material == Air {
                api.swap_offset(0, -1)
            } else if matches!(api.cell_by_offset(x, -1).material, Air) {
                api.swap_offset(x, -1);
            } else if matches!(api.cell_by_offset(x, 0).material, Air) {
                api.swap_offset(x, 0);
            }
            for y in -1..=1 {
                for x in -1..=1 {
                    match api.cell_by_offset(x as isize, y as isize).material {
                        Water => api.set_cell(x, y, Stone),
                        _ => {}
                    }
                }
            }
            if api.cell_by_offset(0, 1).material == Air {
                if fastrand::usize(0..16) == 0 {
                    api.set_cell(0, 1, Fire);
                    api.cell_by_offset(0, 1).health = 100;
                }
            }
        }
        _ => {}
    }
}
