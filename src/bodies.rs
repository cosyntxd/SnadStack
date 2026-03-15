use crate::element::Element;
use glam::Vec2;
use rapier2d::math::{Real, Vector};
use rapier2d::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

static BODY_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub enum PhysicsForm {
    Rigid {
        rigid_body: RigidBodyHandle,
        collider: ColliderHandle,
    },
    Soft {
        nodes: Vec<RigidBodyHandle>,
        colliders: Vec<ColliderHandle>,
        springs: Vec<ImpulseJointHandle>,
    },
}

pub struct SimulatableBody {
    pub id: u64,
    pub form: PhysicsForm,

    pub width: usize,
    pub height: usize,

    pub x_center_offset: f32,
    pub y_center_offset: f32,

    pub elements: Vec<Element>,
}

impl SimulatableBody {
    pub fn new(
        form: PhysicsForm,
        width: usize,
        height: usize,
        x_center_offset: f32,
        y_center_offset: f32,
        elements: Vec<Element>,
    ) -> Self {
        Self {
            id: BODY_ID_COUNTER.fetch_add(1, Ordering::Relaxed),
            form,
            width,
            height,
            x_center_offset,
            y_center_offset,
            elements,
        }
    }

    #[inline]
    pub fn get_element(&self, local_x: usize, local_y: usize) -> Option<&Element> {
        if local_x < self.width && local_y < self.height {
            Some(&self.elements[local_y * self.width + local_x])
        } else {
            None
        }
    }

    #[inline]
    pub fn get_element_mut(&mut self, local_x: usize, local_y: usize) -> Option<&mut Element> {
        if local_x < self.width && local_y < self.height {
            Some(&mut self.elements[local_y * self.width + local_x])
        } else {
            None
        }
    }

    pub fn get_world_coords(
        &self,
        local_x: usize,
        local_y: usize,
        rigid_body_set: &RigidBodySet,
    ) -> Option<(i32, i32)> {
        match &self.form {
            PhysicsForm::Rigid { rigid_body, .. } => {
                let body = rigid_body_set.get(*rigid_body)?;
                let pos = body.position();

                let lx = (local_x as f32) - self.x_center_offset;
                let ly = (local_y as f32) - self.y_center_offset;

                let local_pt = Vector::new(lx, ly);
                let world_pt = pos * local_pt;

                Some((world_pt.x.round() as i32, world_pt.y.round() as i32))
            }
            PhysicsForm::Soft { .. } => {
                // big todo here
                None
            }
        }
    }

    pub fn find_connected_components(&mut self) -> Vec<Vec<Element>> {
        let mut visited = vec![false; self.width * self.height];
        let mut islands = Vec::new();

        for y in 0..self.height {
            for x in 0..self.width {
                let idx = y * self.width + x;
                if !matches!(self.elements[idx].material, crate::element::CellType::Air)
                    && !visited[idx]
                {
                    // Start BFS for a new island
                    let mut island_elements = Vec::with_capacity(self.width * self.height);
                    for _ in 0..(self.width * self.height) {
                        island_elements.push(Element::empty());
                    }

                    let mut queue = vec![(x, y)];
                    visited[idx] = true;

                    while let Some((cx, cy)) = queue.pop() {
                        let c_idx = cy * self.width + cx;

                        island_elements[c_idx] =
                            std::mem::replace(&mut self.elements[c_idx], Element::empty());

                        let neighbors = [(0, 1), (1, 0), (0, -1), (-1, 0)];
                        for (dx, dy) in neighbors {
                            let nx = cx as i32 + dx;
                            let ny = cy as i32 + dy;
                            if nx >= 0
                                && nx < self.width as i32
                                && ny >= 0
                                && ny < self.height as i32
                            {
                                let n_idx = ny as usize * self.width + nx as usize;
                                if !matches!(
                                    self.elements[n_idx].material,
                                    crate::element::CellType::Air
                                ) && !visited[n_idx]
                                {
                                    visited[n_idx] = true;
                                    queue.push((nx as usize, ny as usize));
                                }
                            }
                        }
                    }
                    islands.push(island_elements);
                }
            }
        }

        islands
    }

    /// Gift Wrapping (Jarvis March) algorithm.
    pub fn compute_convex_hull(width: usize, height: usize, elements: &[Element]) -> Vec<Vector> {
        let mut points = Vec::new();

        for y in 0..height {
            for x in 0..width {
                if !matches!(
                    elements[y * width + x].material,
                    crate::element::CellType::Air
                ) {
                    points.push(Vector::new(x as f32, y as f32));
                }
            }
        }

        if points.len() <= 3 {
            return points;
        }

        let mut hull = Vec::new();

        // Find leftmost point
        let mut leftmost = 0;
        for i in 1..points.len() {
            if points[i].x < points[leftmost].x {
                leftmost = i;
            }
        }

        let mut p = leftmost;
        loop {
            hull.push(points[p]);
            let mut q = (p + 1) % points.len();
            for i in 0..points.len() {
                // Cross product for orientation
                let val = (points[i].y - points[p].y) * (points[q].x - points[i].x)
                    - (points[i].x - points[p].x) * (points[q].y - points[i].y);

                // more counter-clockwise
                if val < 0.0 {
                    q = i;
                }
            }
            p = q;
            if p == leftmost || hull.len() > points.len() {
                break;
            }
        }

        hull
    }
}

pub struct PhysicsManager {
    pub rigid_body_set: RigidBodySet,
    pub collider_set: ColliderSet,
    pub integration_parameters: IntegrationParameters,
    pub physics_pipeline: PhysicsPipeline,
    pub island_manager: IslandManager,
    pub broad_phase: BroadPhaseBvh,
    pub narrow_phase: NarrowPhase,
    pub impulse_joint_set: ImpulseJointSet,
    pub multibody_joint_set: MultibodyJointSet,
    pub ccd_solver: CCDSolver,
    pub gravity: Vec2,

    pub active_bodies: Vec<SimulatableBody>,
}

impl Default for PhysicsManager {
    fn default() -> Self {
        Self::new(Vec2::new(0.0, -9.81))
    }
}

impl PhysicsManager {
    pub fn new(gravity: Vec2) -> Self {
        Self {
            rigid_body_set: RigidBodySet::new(),
            collider_set: ColliderSet::new(),
            integration_parameters: IntegrationParameters::default(),
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: BroadPhaseBvh::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joint_set: ImpulseJointSet::new(),
            multibody_joint_set: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            gravity,
            active_bodies: Vec::new(),
        }
    }

    pub fn step(&mut self) {
        self.physics_pipeline.step(
            Vector::new(self.gravity.x, self.gravity.y),
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.ccd_solver,
            &(),
            &(),
        );
    }

    pub fn spawn_rigid_from_pixels(
        &mut self,
        world_x: f32,
        world_y: f32,
        width: usize,
        height: usize,
        elements: Vec<Element>,
    ) -> Option<&mut SimulatableBody> {
        let hull_points = SimulatableBody::compute_convex_hull(width, height, &elements);
        if hull_points.len() < 3 {
            return None;
        } // Not enough points to make a stable polygon

        // Find center of mass offset
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        for pt in &hull_points {
            if pt.x < min_x {
                min_x = pt.x;
            }
            if pt.x > max_x {
                max_x = pt.x;
            }
            if pt.y < min_y {
                min_y = pt.y;
            }
            if pt.y > max_y {
                max_y = pt.y;
            }
        }

        let cx = (min_x + max_x) / 2.0;
        let cy = (min_y + max_y) / 2.0;
=
        let local_hull: Vec<Vector> = hull_points
            .into_iter()
            .map(|pt| Vector::new(pt.x - cx, pt.y - cy))
            .collect();

        let collider = ColliderBuilder::convex_hull(&local_hull)
            .unwrap_or_else(|| ColliderBuilder::ball(1.0)) // Fallback if collinear/degenerate
            .restitution(0.3)
            .friction(0.5)
            .density(1.0)
            .build();

        let rigid_body = RigidBodyBuilder::dynamic()
            .translation(Vector::new(world_x, world_y))
            .build();

        let rb_handle = self.rigid_body_set.insert(rigid_body);
        let coll_handle =
            self.collider_set
                .insert_with_parent(collider, rb_handle, &mut self.rigid_body_set);

        let form = PhysicsForm::Rigid {
            rigid_body: rb_handle,
            collider: coll_handle,
        };

        let body = SimulatableBody::new(form, width, height, cx, cy, elements);

        self.active_bodies.push(body);
        self.active_bodies.last_mut()
    }

    pub fn spawn_dynamic_rect(
        &mut self,
        world_x: f32,
        world_y: f32,
        width: usize,
        height: usize,
    ) -> &mut SimulatableBody {
        let hx = width as f32 / 2.0;
        let hy = height as f32 / 2.0;

        let rigid_body = RigidBodyBuilder::dynamic()
            .translation(Vector::new(world_x, world_y))
            .build();

        let collider = ColliderBuilder::cuboid(hx, hy)
            .restitution(0.3)
            .friction(0.5)
            .density(1.0)
            .build();

        let rb_handle = self.rigid_body_set.insert(rigid_body);
        let coll_handle =
            self.collider_set
                .insert_with_parent(collider, rb_handle, &mut self.rigid_body_set);

        let mut elements = Vec::with_capacity(width * height);
        for _ in 0..(width * height) {
            elements.push(Element::empty()); // Empty initially
        }

        let form = PhysicsForm::Rigid {
            rigid_body: rb_handle,
            collider: coll_handle,
        };

        let body = SimulatableBody::new(form, width, height, hx, hy, elements);

        self.active_bodies.push(body);
        self.active_bodies.last_mut().unwrap()
    }

    pub fn spawn_static_rect(&mut self, world_x: f32, world_y: f32, width: f32, height: f32) {
        let hx = width / 2.0;
        let hy = height / 2.0;

        let rigid_body = RigidBodyBuilder::fixed()
            .translation(Vector::new(world_x, world_y))
            .build();

        let collider = ColliderBuilder::cuboid(hx, hy).build();

        let rb_handle = self.rigid_body_set.insert(rigid_body);
        self.collider_set
            .insert_with_parent(collider, rb_handle, &mut self.rigid_body_set);
    }
}
