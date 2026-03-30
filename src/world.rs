use crate::bodies::PhysicsManager;
use crate::chunks::ChunkManager;
use crate::element::CellType;
use crate::render::{PixelDiff, TILE_SIZE};
use glam::Vec2;
use winit::dpi::PhysicalSize;

pub struct WorldState {
    chunks: ChunkManager,
    physics: PhysicsManager,
    queued_pixels: Vec<PixelDiff>,
    ticks: u32,
    camera_pos: Vec2,
    screen_size: PhysicalSize<u32>,
    zoom: f32,
}
impl WorldState {
    pub fn new() -> Self {
        Self {
            chunks: ChunkManager::new(),
            physics: PhysicsManager::new(),
            queued_pixels: Vec::new(),
            ticks: 0,
            camera_pos: Vec2::ZERO,
            screen_size: PhysicalSize::new(0, 0),
            zoom: 1.0,
        }
    }

}

pub struct CompleteWorld {
    pub tick: u32,
}

impl CompleteWorld {
    pub fn new() -> Self {
        Self { tick: 0 }
    }

    pub fn simulate_step(
        &mut self,
        chunk_manager: &mut ChunkManager,
        camera_pos: Vec2,
        screen_size: PhysicalSize<u32>,
        zoom: f32,
        diffs: &mut Vec<PixelDiff>,
        last_render_tick: u32,
    ) {
        self.tick = self.tick.wrapping_add(1);

        let mut active_coords: Vec<_> = chunk_manager
            .visible_chunks
            .iter()
            .filter(|&&c| {
                chunk_manager.is_in_view(c, camera_pos, screen_size, zoom)
                    && chunk_manager.active_mapping.contains_key(&c)
            })
            .cloned()
            .collect();
        active_coords.sort_by(|a, b| b.y.cmp(&a.y)); // process bottom chunks first

        let is_even = self.tick % 2 == 0;

        for coord in active_coords {
            let base_x = coord.x * TILE_SIZE as i32;
            let base_y = coord.y * TILE_SIZE as i32;

            for ly in (0..TILE_SIZE as i32).rev() {
                for i in 0..TILE_SIZE as i32 {
                    let lx = if is_even {
                        i
                    } else {
                        (TILE_SIZE as i32 - 1) - i
                    };

                    let world_x = base_x + lx;
                    let world_y = base_y + ly;

                    let Some(el) = chunk_manager.get_element(world_x, world_y) else {
                        continue;
                    };

                    if el.update_time == self.tick {
                        continue;
                    }
                    if matches!(el.material, CellType::Air | CellType::Brick) {
                        continue;
                    }

                    let mut moved = false;
                    let mut target_x = world_x;
                    let mut target_y = world_y;

                    if matches!(el.material, CellType::Sand) {
                        let dir = if is_even { 1 } else { -1 };
                        let options = [(0, 1), (-dir, 1), (dir, 1)];

                        for (dx, dy) in options {
                            if let Some(target) =
                                chunk_manager.get_element(world_x + dx, world_y + dy)
                            {
                                if matches!(target.material, CellType::Air | CellType::Water) {
                                    target_x = world_x + dx;
                                    target_y = world_y + dy;
                                    moved = true;
                                    break;
                                }
                            }
                        }
                    }

                    if moved {
                        chunk_manager.swap_elements(
                            world_x,
                            world_y,
                            target_x,
                            target_y,
                            diffs,
                            self.tick,
                            last_render_tick,
                        );
                    }
                }
            }
        }
    }
}
