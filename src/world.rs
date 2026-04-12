use crate::bodies::PhysicsManager;
use crate::chunks::ChunkManager;
use crate::element::{CellType, Element};
use crate::render::{PixelDiff, TILE_SIZE};
use glam::Vec2;
use winit::dpi::PhysicalSize;


pub enum WorldUserActions {
    ApplyPhysicsForce {  },
    Place
}

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
            physics: PhysicsManager::new(Vec2::new(0.0, 600.0)),
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

    pub fn unrasterize_bodies(&mut self) {
        for body in &mut self.physics.active_bodies {
            for &(wx, wy) in &body.last_rasterized {
                if let Some(el) = self.chunks.get_element(wx, wy) {
                    if el.body_id == body.id {
                        let empty = Element::empty();
                        self.chunks.set_element_with_diff(
                            wx,
                            wy,
                            empty,
                            &mut self.queued_pixels,
                            self.ticks,
                            self.last_render_tick,
                        );
                    }
                }
            }
            body.last_rasterized.clear();
        }
    }

    pub fn rasterize_bodies(&mut self) {
        let mut updates = Vec::new();

        for body in &mut self.physics.active_bodies {
            if let Some((pos, angle)) = body.get_world_transform(&self.physics.rigid_body_set) {
                let cos_a = angle.cos();
                let sin_a = angle.sin();

                // world bounding box of the rotated body
                let w = body.width as f32;
                let h = body.height as f32;

                let corners = [
                    (0.0, 0.0),
                    (w, 0.0),
                    (0.0, h),
                    (w, h),
                ];

                let mut min_wx = f32::MAX;
                let mut max_wx = f32::MIN;
                let mut min_wy = f32::MAX;
                let mut max_wy = f32::MIN;

                for (cx, cy) in corners {
                    let local_x = cx - body.x_center_offset;
                    let local_y = cy - body.y_center_offset;

                    let world_x = pos.x + local_x * cos_a - local_y * sin_a;
                    let world_y = pos.y + local_x * sin_a + local_y * cos_a;

                    min_wx = min_wx.min(world_x);
                    max_wx = max_wx.max(world_x);
                    min_wy = min_wy.min(world_y);
                    max_wy = max_wy.max(world_y);
                }

                let start_x = min_wx.floor() as i32;
                let end_x = max_wx.ceil() as i32;
                let start_y = min_wy.floor() as i32;
                let end_y = max_wy.ceil() as i32;

                // inverse mapping)
                for wy in start_y..=end_y {
                    for wx in start_x..=end_x {
                        // Transform world coordinate back to local space
                        let dx = (wx as f32) - pos.x;
                        let dy = (wy as f32) - pos.y;

                        // cos(-a) = cos(a), sin(-a) = -sin(a)
                        let local_x_f = dx * cos_a + dy * sin_a + body.x_center_offset;
                        let local_y_f = -dx * sin_a + dy * cos_a + body.y_center_offset;

                        let lx = local_x_f.round() as i32;
                        let ly = local_y_f.round() as i32;

                        if lx >= 0 && lx < body.width as i32 && ly >= 0 && ly < body.height as i32 {
                            let el = body.elements[ly as usize * body.width + lx as usize];
                            if !matches!(el.material, CellType::Air) {
                                let mut new_el = el;
                                new_el.body_id = body.id;
                                updates.push((body.id, wx, wy, new_el));
                            }
                        }
                    }
                }
            }
        }

        for (b_id, wx, wy, el) in updates {
            let existing = self.chunks.get_element(wx, wy);
            if let Some(ex_el) = existing {

                // dont overwrite
                if ex_el.body_id == 0 && !matches!(ex_el.material, CellType::Air) {
                    continue;
                }

                self.chunks.set_element_with_diff(
                    wx,
                    wy,
                    el,
                    &mut self.queued_pixels,
                    self.ticks,
                    self.last_render_tick,
                );

                if let Some(body) = self.physics.active_bodies.iter_mut().find(|b| b.id == b_id) {
                    body.last_rasterized.push((wx, wy));
                }
            }
        }
    }

    pub fn update_chunk_colliders(&mut self) {
        let mut updates = Vec::new();
        for (&coord, &slot_id) in &self.chunks.active_mapping {
            let slot = &self.chunks.physical_slots[slot_id as usize];
            if slot.needs_collider_update {
                updates.push((coord, slot_id));
            }
        }

        for (coord, slot_id) in updates {
            let mut colliders = Vec::new();
            {
                let slot = &mut self.chunks.physical_slots[slot_id as usize];
                slot.needs_collider_update = false;

                if let Some(rb_handle) = slot.static_body {
                    self.physics.rigid_body_set.remove(
                        rb_handle,
                        &mut self.physics.island_manager,
                        &mut self.physics.collider_set,
                        &mut self.physics.impulse_joint_set,
                        &mut self.physics.multibody_joint_set,
                        true,
                    );
                    slot.static_body = None;
                }

                for y in 0..TILE_SIZE as i32 {
                    let mut span_start = None;
                    for x in 0..TILE_SIZE as i32 {
                        let idx = (y * TILE_SIZE as i32 + x) as usize;
                        let el = &slot.elements[idx];
                        let is_solid = !matches!(el.material, CellType::Air) && el.body_id == 0;

                        if is_solid {
                            if span_start.is_none() {
                                span_start = Some(x);
                            }
                        } else {
                            if let Some(start_x) = span_start {
                                let end_x = x - 1;
                                let width = (end_x - start_x + 1) as f32;
                                let hx = width / 2.0;
                                let hy = 0.5;

                                let cx = coord.x as f32 * TILE_SIZE as f32 + start_x as f32 + hx;
                                let cy = coord.y as f32 * TILE_SIZE as f32 + y as f32 + hy;

                                let collider = rapier2d::prelude::ColliderBuilder::cuboid(hx, hy)
                                    .translation(rapier2d::prelude::Vector::new(cx, cy))
                                    .build();
                                colliders.push(collider);

                                span_start = None;
                            }
                        }
                    }
                    if let Some(start_x) = span_start {
                        let end_x = TILE_SIZE as i32 - 1;
                        let width = (end_x - start_x + 1) as f32;
                        let hx = width / 2.0;
                        let hy = 0.5;

                        let cx = coord.x as f32 * TILE_SIZE as f32 + start_x as f32 + hx;
                        let cy = coord.y as f32 * TILE_SIZE as f32 + y as f32 + hy;

                        let collider = rapier2d::prelude::ColliderBuilder::cuboid(hx, hy)
                            .translation(rapier2d::prelude::Vector::new(cx, cy))
                            .build();
                        colliders.push(collider);
                    }
                }
            }

            if !colliders.is_empty() {
                let rb = rapier2d::prelude::RigidBodyBuilder::fixed().build();
                let rb_handle = self.physics.rigid_body_set.insert(rb);
                for coll in colliders {
                    self.physics.collider_set.insert_with_parent(
                        coll,
                        rb_handle,
                        &mut self.physics.rigid_body_set,
                    );
                }
                let slot = &mut self.chunks.physical_slots[slot_id as usize];
                slot.static_body = Some(rb_handle);
            }
        }
    }

    pub fn simulate_step(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);

        self.unrasterize_bodies();
        self.physics.step();
        self.rasterize_bodies();
        self.update_chunk_colliders();

        if self.ticks % 50 == 0 && !self.physics.active_bodies.is_empty() {
            for body in &self.physics.active_bodies {
                if let Some((pos, _)) = body.get_world_transform(&self.physics.rigid_body_set) {
                    println!("Tick {}: Body {} at ({:.2}, {:.2})", self.ticks, body.id, pos.x, pos.y);
                }
            }
        }

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

                    if el.body_id != 0 {
                        continue;
                    }

                    if el.update_time == self.ticks {
                        continue;
                    }
                    if matches!(el.material, CellType::Air | CellType::Brick) {
                        continue;
                    }

                    if matches!(el.material, CellType::Sand) {
                        let mut new_el = el;
                        let dir = if is_even { 1 } else { -1 };

                        let mut can_move_down = false;
                        if let Some(t) = self.chunks.get_element(world_x, world_y + 1) {
                            if matches!(t.material, CellType::Air | CellType::Water) { can_move_down = true; }
                        }
                        let mut can_move_s1 = false;
                        if let Some(t) = self.chunks.get_element(world_x - dir, world_y + 1) {
                            if matches!(t.material, CellType::Air | CellType::Water) { can_move_s1 = true; }
                        }
                        let mut can_move_s2 = false;
                        if let Some(t) = self.chunks.get_element(world_x + dir, world_y + 1) {
                            if matches!(t.material, CellType::Air | CellType::Water) { can_move_s2 = true; }
                        }

                        if !can_move_down && !can_move_s1 && !can_move_s2 {
                            if new_el.vy != 0 || new_el.sub_y != 0 {
                                new_el.vy = 0;
                                new_el.sub_y = 0;
                                self.chunks.set_element_with_diff(world_x, world_y, new_el, &mut self.queued_pixels, self.ticks, self.last_render_tick);
                            }
                            continue;
                        }

                        // 600 px/s^2 -> 24 units per tick (with scale=100)
                        new_el.vy = new_el.vy.saturating_add(24).min(1000);
                        let total_y = new_el.vy as i32 + new_el.sub_y as i32;
                        let move_y = total_y / 100;
                        new_el.sub_y = (total_y % 100) as i8;

                        if move_y == 0 {
                            self.chunks.set_element_with_diff(world_x, world_y, new_el, &mut self.queued_pixels, self.ticks, self.last_render_tick);
                            continue;
                        }

                        let mut target_x = world_x;
                        let mut target_y = world_y;
                        let mut hit_ground = false;

                        // down
                        for _ in 0..move_y {
                            let mut stepped = false;

                            if let Some(t) = self.chunks.get_element(target_x, target_y + 1) {
                                if matches!(t.material, CellType::Air | CellType::Water) {
                                    target_y += 1;
                                    stepped = true;
                                }
                            }

                            // diagonals
                            if !stepped {
                                let options = [(-dir, 1), (dir, 1)];
                                for (dx, dy) in options {
                                    if let Some(t) = self.chunks.get_element(target_x + dx, target_y + dy) {
                                        if matches!(t.material, CellType::Air | CellType::Water) {
                                            target_x += dx;
                                            target_y += dy;
                                            stepped = true;
                                            new_el.vy = (new_el.vy / 2).max(100); // sliding fric
                                            break;
                                        }
                                    }
                                }
                            }

                            if !stepped {
                                hit_ground = true;
                                break;
                            }
                        }

                        if hit_ground {
                            new_el.vy = 0;
                            new_el.sub_y = 0;
                        }

                        let e2 = self.chunks.get_element(target_x, target_y).unwrap_or(Element::empty());
                        self.chunks.set_element_with_diff(world_x, world_y, e2, &mut self.queued_pixels, self.ticks, self.last_render_tick);
                        self.chunks.set_element_with_diff(target_x, target_y, new_el, &mut self.queued_pixels, self.ticks, self.last_render_tick);
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
            diffs: Vec::new(),
            compute_time_ms: 0,
            tick: 0,
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
        let next_render = self.cache.pop().unwrap_or_else(PixelQueue::new);
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
