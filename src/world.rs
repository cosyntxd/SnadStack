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

    pub fn resolve_rigid_grid_collisions(&mut self) {
        for body in &mut self.physics.active_bodies {
            let rb_handle = match &body.form {
                crate::bodies::PhysicsForm::Rigid { rigid_body, .. } => *rigid_body,
                _ => continue,
            };

            let Some(rb) = self.physics.rigid_body_set.get(rb_handle) else { continue; };
            let pos = rb.translation();
            let angle = rb.rotation().angle();
            let cos_a = angle.cos();
            let sin_a = angle.sin();

            let mut total_normal = rapier2d::math::Vector::new(0.0, 0.0);
            let mut total_contact_points = rapier2d::math::Vector::new(0.0, 0.0);
            let mut hit_count = 0;

            // Helper to check if a grid cell is solid
            let is_solid = |x: i32, y: i32, chunks: &crate::chunks::ChunkManager| -> f32 {
                if let Some(el) = chunks.get_element(x, y) {
                    if !matches!(el.material, crate::element::CellType::Air | crate::element::CellType::Water) {
                        return 1.0;
                    }
                }
                0.0
            };

            // Sample internal points to detect collision depth/normal
            for ly in (0..body.height).step_by(1) {
                for lx in (0..body.width).step_by(1) {
                    if let Some(el) = body.get_element(lx, ly) {
                        if !matches!(el.material, crate::element::CellType::Air) {
                            // Transform to world
                            let l_x = (lx as f32) - body.x_center_offset;
                            let l_y = (ly as f32) - body.y_center_offset;
                            let wx = pos.x + l_x * cos_a - l_y * sin_a;
                            let wy = pos.y + l_x * sin_a + l_y * cos_a;

                            let world_x = wx.round() as i32;
                            let world_y = wy.round() as i32;

                            // Check grid
                            if is_solid(world_x, world_y, &self.chunks) > 0.0 {
                                // Compute gradient to find surface normal
                                // Normal points AWAY from the solid mass
                                let nx = is_solid(world_x - 1, world_y, &self.chunks) - is_solid(world_x + 1, world_y, &self.chunks);
                                let ny = is_solid(world_x, world_y - 1, &self.chunks) - is_solid(world_x, world_y + 1, &self.chunks);

                                let mut normal = rapier2d::math::Vector::new(nx, ny);
                                if normal.x == 0.0 && normal.y == 0.0 {
                                    // If surrounded, push towards body center as fallback
                                    normal = rapier2d::math::Vector::new(pos.x - wx, pos.y - wy);
                                }

                                total_normal += normal;
                                total_contact_points += rapier2d::math::Vector::new(wx, wy);
                                hit_count += 1;
                            }
                        }
                    }
                }
            }

            if hit_count > 0 {
                let avg_contact_point = total_contact_points / (hit_count as f32);

                let mut final_normal = total_normal;
                if final_normal.x != 0.0 || final_normal.y != 0.0 {
                    final_normal = final_normal.normalize();
                } else {
                    // Fallback straight up
                    final_normal = rapier2d::math::Vector::new(0.0, -1.0);
                }

                if let Some(rb) = self.physics.rigid_body_set.get_mut(rb_handle) {
                    // --- REALISTIC COLLISION SOLVER ---
                    // Tunable parameters (matching Rapier's standard physical properties)
                    let restitution = 0.3;     // Bounciness (0.0 to 1.0)
                    let friction = 0.6;        // Surface sliding friction

                    // Stiffness acts as a penalty force to combat your 600.0 gravity sinking
                    let stiffness = 250.0;

                    let r = rapier2d::math::Vector::new(avg_contact_point.x - pos.x, avg_contact_point.y - pos.y);

                    let v_lin = rb.linvel();
                    let omega = rb.angvel();

                    // Point velocity: v + w x r
                    let v_pt = rapier2d::math::Vector::new(
                        v_lin.x - omega * r.y,
                        v_lin.y + omega * r.x
                    );

                    let v_n = v_pt.dot(final_normal);

                    let mass = rb.mass();
                    let inertia = rb.mass_properties().local_mprops.principal_inertia();
                    let m_inv = if mass > 0.0 { 1.0 / mass } else { 0.0 };
                    let i_inv = if inertia > 0.0 { 1.0 / inertia } else { 0.0 };

                    let r_cross_n = r.x * final_normal.y - r.y * final_normal.x;
                    let effective_mass_n = m_inv + r_cross_n * r_cross_n * i_inv;

                    // --- RIGID BAUMGARTE SOLVER ---
                    let dt = 1.0 / 50.0;
                    let inv_dt = 50.0;
                    let restitution = 0.1; // Low bounce
                    let friction_coeff = 0.6;

                    let mut j_normal = 0.0;

                    // 1. Resolve Velocity (Bounce)
                    if v_n < 0.0 {
                        let bounce = if v_n.abs() > 20.0 { restitution } else { 0.0 };
                        j_normal += -(1.0 + bounce) * v_n / effective_mass_n;
                    }

                    // 2. Resolve Penetration (Baumgarte Stabilization)
                    // Push the body out based on overlap depth, but via velocity impulses
                    let penetration_depth = (hit_count as f32).sqrt();
                    let slop = 0.2; // Tiny allowable overlap
                    let allowed_penetration = (penetration_depth - slop).max(0.0);

                    // Resolve 30% of the penetration per frame
                    let bias_factor = 0.3;
                    let bias_v = bias_factor * allowed_penetration * inv_dt;
                    j_normal += bias_v / effective_mass_n;

                    let normal_impulse = final_normal * j_normal;
                    rb.apply_impulse_at_point(normal_impulse, avg_contact_point.into(), true);

                    // --- FRICTION SOLVER ---
                    let tangent = rapier2d::math::Vector::new(-final_normal.y, final_normal.x);
                    let v_t = v_pt.dot(tangent);

                    if v_t.abs() > 0.001 {
                        let r_cross_t = r.x * tangent.y - r.y * tangent.x;
                        let effective_mass_t = m_inv + r_cross_t * r_cross_t * i_inv;

                        let j_tangent = -v_t / effective_mass_t;

                        // Include resting gravity force so bodies don't slide on flat ground
                        let resting_j = mass * 600.0 * dt;
                        let max_friction = friction_coeff * (j_normal.abs() + resting_j);

                        let j_tangent_clamped = j_tangent.clamp(-max_friction, max_friction);

                        let friction_impulse = tangent * j_tangent_clamped;
                        rb.apply_impulse_at_point(friction_impulse, avg_contact_point.into(), true);
                    }

                    // --- SETTLE / SLEEPING ---
                    // If barely moving, strongly dampen to help Rapier put it to sleep
                    if rb.linvel().length() < 10.0 && rb.angvel().abs() < 0.5 {
                        let mut final_v = rb.linvel().clone();
                        final_v *= 0.5;
                        rb.set_linvel(final_v, true);

                        let mut final_av = rb.angvel();
                        final_av *= 0.5;
                        rb.set_angvel(final_av, true);
                    }
                }
            }
        }
    }

    pub fn simulate_step(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);

        self.unrasterize_bodies();
        self.physics.step();
        self.resolve_rigid_grid_collisions();
        self.rasterize_bodies();

        // if self.ticks % 50 == 0 && !self.physics.active_bodies.is_empty() {
        //     for body in &self.physics.active_bodies {
        //         if let Some((pos, _)) = body.get_world_transform(&self.physics.rigid_body_set) {
        //             println!("Tick {}: Body {} at ({:.2}, {:.2})", self.ticks, body.id, pos.x, pos.y);
        //         }
        //     }
        // }

        let mut active_coords: Vec<_> = self
            .chunks
            .visible_chunks
            .iter()
            .filter(|&&c| {
                self.chunks
                    .is_in_view(c, self.camera_pos, self.screen_size, self.zoom)
                    && self.chunks.has_chunk(c)
            })
            .cloned()
            .collect();
        active_coords.sort_by(|a, b| b.y.cmp(&a.y)); // process bottom chunks first

        let is_even = self.ticks % 2 == 0;

        for coord in &active_coords {
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
                        let below_vy = self.chunks.get_element(world_x, world_y + 1)
                            .map(|t| t.vy as i32)
                            .unwrap_or(i8::MAX as i32);
                        if !can_move_down && !can_move_s1 && !can_move_s2 {

                            if new_el.vy != 0 || new_el.sub_y != 0 {
                                new_el.vy = 0;
                                new_el.sub_y = 0;
                                self.chunks.set_element_with_diff(world_x, world_y, new_el, &mut self.queued_pixels, self.ticks, self.last_render_tick);
                            }
                            continue;
                        }

                        // 600 px/s^2 -> 24 units per tick (with scale=100)
                        // prevent top from falling faster than elements below (clump falling together)

                        // if below_vy - 24 >= el.vy as i32 {
                            new_el.vy = new_el.vy.saturating_add(24).min(1000);
                        // }

                        // new_el.vy = new_el.vy.saturating_add(24).min(1000);
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

                        if hit_ground && below_vy < new_el.vy as i32 {
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
