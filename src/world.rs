use crate::bodies::PhysicsManager;
use crate::chunks::ChunkManager;
use crate::element::{CellType, Element};
use crate::render::{PixelDiff, TILE_SIZE};
use glam::Vec2;
use winit::dpi::PhysicalSize;

pub struct World {
    pub chunks: ChunkManager,
    pub physics: PhysicsManager,
    pub queued_pixels: Vec<PixelDiff>,
    pub ticks: u32,
    pub last_render_tick: u32,
    pub camera_pos: Vec2,
    pub screen_size: PhysicalSize<u32>,
    pub zoom: f32,
}

impl World {
    pub fn new() -> Self {
        Self {
            chunks: ChunkManager::new(),
            physics: PhysicsManager::new(Vec2::new(0.0, -9.81)),
            queued_pixels: Vec::new(),
            ticks: 0,
            last_render_tick: 0,
            camera_pos: Vec2::ZERO,
            screen_size: PhysicalSize::new(0, 0),
            zoom: 10.0,
        }
    }
    pub fn frame_random(&self) -> u32 {
        let mut state = self.ticks;
        state ^= state >> 16;
        state = state.wrapping_mul(0x85ebca6b);
        state ^= state >> 13;
        state = state.wrapping_mul(0xc2b2ae35);
        state ^= state >> 16;
        state
    }
    pub fn simulate_step(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);

        let mut active_coords: Vec<_> = self
            .chunks
            .visible_chunks
            .iter()
            .filter(|&&c| {
                self.chunks
                    .is_in_view(c, self.camera_pos, self.screen_size, self.zoom)
                    && self.chunks.active_mapping.contains_key(&c)
            })
            .cloned()
            .collect();
        active_coords.sort_by(|a, b| b.y.cmp(&a.y)); // process bottom chunks first

        let is_even = self.ticks % 2 == 0;

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

                    let Some(el) = self.chunks.get_element(world_x, world_y) else {
                        continue;
                    };

                    if el.update_time == self.ticks {
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
                                self.chunks.get_element(world_x + dx, world_y + dy)
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
                        let e1 = self.chunks.get_element(world_x, world_y).unwrap_or(Element::empty());
                        let e2 = self.chunks.get_element(target_x, target_y).unwrap_or(Element::empty());

                        self.chunks.set_element_with_diff(
                            world_x,
                            world_y,
                            e2,
                            &mut self.queued_pixels,
                            self.ticks,
                            self.last_render_tick,
                        );
                        self.chunks.set_element_with_diff(
                            target_x,
                            target_y,
                            e1,
                            &mut self.queued_pixels,
                            self.ticks,
                            self.last_render_tick,
                        );
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct PixelQueue {
    pub diffs: Vec<PixelDiff>,
    pub compute_time_ms: u64,
    pub tick: u64,
}

impl PixelQueue {
    pub fn new() -> Self {
        Self {

        }
    }
}

pub struct DiffQueueManager {
    cache: Vec<PixelQueue>,
    render: PixelQueue,
    completed: Vec<PixelQueue>,
}

impl DiffQueueManager {
    pub fn new() -> Self {
        Self {
            cache: Vec::new(),
            render: PixelQueue::new(),
            completed: Vec::new(),
        }
    }

    pub fn complete_simulation(&mut self) {
        let next_render = self.cache.pop().unwrap_or_else(|| PixelQueue {
            diffs: Vec::new(),
            compute_time_ms: 0,
            tick: 0,
        });
        let old_render = std::mem::replace(&mut self.render, next_render);
        self.completed.push(old_render);
    }

    pub fn complete_draw(&mut self) {
        for mut queue in self.completed.drain(..) {
            queue.diffs.clear();
            self.cache.push(queue);
        }
    }

    pub fn get_renderable(&self) -> &Vec<PixelQueue> {
        &self.completed
    }
}
