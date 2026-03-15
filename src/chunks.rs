use glam::Vec2;
use rustc_hash::FxHashMap;
use winit::dpi::PhysicalSize;

use crate::element::Element;
use crate::render::{PixelDiff, TextureId, TileInstance, MAX_PHYSICAL_TEXTURES, TILE_SIZE};

pub const CHUNK_BYTE_SIZE: usize = (TILE_SIZE * TILE_SIZE * 4) as usize;
pub const CHUNK_ELEMENTS: usize = (TILE_SIZE * TILE_SIZE) as usize;

#[derive(Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone)]
pub struct PhysicalSlot {
    pub current_coord: Option<ChunkCoord>,
    pub last_visible_frame: u64,
    pub is_empty: bool,
    pub pixels: Box<[u8; CHUNK_BYTE_SIZE]>,
    pub elements: Box<[Element; CHUNK_ELEMENTS]>,
}

impl Default for PhysicalSlot {
    fn default() -> Self {
        Self {
            current_coord: None,
            last_visible_frame: 0,
            is_empty: true,
            pixels: vec![0; CHUNK_BYTE_SIZE]
                .into_boxed_slice()
                .try_into()
                .unwrap(),
            elements: vec![Element::empty(); CHUNK_ELEMENTS]
                .into_boxed_slice()
                .try_into()
                .unwrap(),
        }
    }
}

pub enum ChunkCommand {
    UploadToGpu {
        physical_id: TextureId,
        coord: ChunkCoord,
    },
    EvictedFromGpu {
        coord: ChunkCoord,
    },
}

pub struct ChunkManager {
    pub physical_slots: [PhysicalSlot; MAX_PHYSICAL_TEXTURES as usize],
    pub active_mapping: FxHashMap<ChunkCoord, TextureId>,
    pub visible_chunks: Vec<ChunkCoord>,
    pub frame_counter: u64,
}

impl ChunkManager {
    pub fn new() -> Self {
        let physical_slots = std::array::from_fn(|_| PhysicalSlot::default());

        Self {
            physical_slots,
            active_mapping: FxHashMap::default(),
            visible_chunks: Vec::new(),
            frame_counter: 0,
        }
    }

    pub fn update(
        &mut self,
        camera_pos: Vec2,
        screen_size: PhysicalSize<u32>,
        zoom: f32,
    ) -> Vec<ChunkCommand> {
        self.frame_counter += 1;

        let pad = 2;
        let view_w = (screen_size.width as f32 / zoom) * 0.5;
        let view_h = (screen_size.height as f32 / zoom) * 0.5;

        let mut min_cx = ((camera_pos.x - view_w) / TILE_SIZE as f32).floor() as i32 - pad;
        let mut max_cx = ((camera_pos.x + view_w) / TILE_SIZE as f32).floor() as i32 + pad;
        let mut min_cy = ((camera_pos.y - view_h) / TILE_SIZE as f32).floor() as i32 - pad;
        let mut max_cy = ((camera_pos.y + view_h) / TILE_SIZE as f32).floor() as i32 + pad;

        let visible_w = (max_cx - min_cx + 1).max(0) as u32;
        let visible_h = (max_cy - min_cy + 1).max(0) as u32;
        let total_visible = visible_w * visible_h;

        if total_visible > MAX_PHYSICAL_TEXTURES {
            log::error!(
                "Too many chunks in view! Requested {}x{} ({}), maximum allowed is {}",
                visible_w,
                visible_h,
                total_visible,
                MAX_PHYSICAL_TEXTURES
            );

            // Clamp to a safe square around the camera center
            let safe_radius = ((MAX_PHYSICAL_TEXTURES as f32).sqrt() / 2.0).floor() as i32 - 1;
            let center_cx = (camera_pos.x / TILE_SIZE as f32).floor() as i32;
            let center_cy = (camera_pos.y / TILE_SIZE as f32).floor() as i32;

            min_cx = center_cx - safe_radius;
            max_cx = center_cx + safe_radius;
            min_cy = center_cy - safe_radius;
            max_cy = center_cy + safe_radius;
        }

        let mut commands = Vec::new();
        let mut needed_coords = Vec::with_capacity(16);
        self.visible_chunks.clear();

        // Mark all currently active chunks as out-of-bounds. We'll unmark them if they are still visible.
        let mut to_evict: Vec<ChunkCoord> = self.active_mapping.keys().cloned().collect();

        for y in min_cy..=max_cy {
            for x in min_cx..=max_cx {
                let coord = ChunkCoord { x, y };
                self.visible_chunks.push(coord);

                if let Some(&id) = self.active_mapping.get(&coord) {
                    self.physical_slots[id as usize].last_visible_frame = self.frame_counter;
                    if let Some(pos) = to_evict.iter().position(|&c| c == coord) {
                        to_evict.swap_remove(pos);
                    }
                } else {
                    needed_coords.push(coord);
                }
            }
        }

        // Explicitly evict chunks that are no longer visible so we free up PhysicalSlots immediately
        for evict_coord in to_evict {
            if let Some(id) = self.active_mapping.remove(&evict_coord) {
                self.physical_slots[id as usize].is_empty = true;
                commands.push(ChunkCommand::EvictedFromGpu { coord: evict_coord });
            }
        }

        if needed_coords.is_empty() {
            return commands;
        }

        for coord in needed_coords {
            let slot_id = self.find_best_slot();

            let slot = &mut self.physical_slots[slot_id as usize];

            if !slot.is_empty {
                let old_coord = slot.current_coord.unwrap();
                commands.push(ChunkCommand::EvictedFromGpu { coord: old_coord });
                self.active_mapping.remove(&old_coord);
            }

            slot.current_coord = Some(coord);
            slot.last_visible_frame = self.frame_counter;
            slot.is_empty = false;

            self.active_mapping.insert(coord, slot_id);

            commands.push(ChunkCommand::UploadToGpu {
                physical_id: slot_id,
                coord,
            });
        }

        commands
    }

    fn find_best_slot(&self) -> TextureId {
        let mut lru_id = 0;
        let mut min_frame = u64::MAX;

        for (i, slot) in self.physical_slots.iter().enumerate() {
            if slot.is_empty {
                return i as TextureId;
            }
            if slot.last_visible_frame < min_frame {
                min_frame = slot.last_visible_frame;
                lru_id = i;
            }
        }

        lru_id as TextureId
    }

    pub fn get_instances(&self) -> Vec<TileInstance> {
        let mut instances = Vec::with_capacity(self.active_mapping.len());
        for (coord, &id) in &self.active_mapping {
            instances.push(TileInstance {
                position: [
                    coord.x as f32 * TILE_SIZE as f32,
                    coord.y as f32 * TILE_SIZE as f32,
                ],
                _padding: [0; 3],
                texture_id: id,
            });
        }
        instances
    }

    #[inline]
    pub fn get_element(&self, world_x: i32, world_y: i32) -> Option<Element> {
        let chunk_x = world_x.div_euclid(TILE_SIZE as i32);
        let chunk_y = world_y.div_euclid(TILE_SIZE as i32);
        let coord = ChunkCoord {
            x: chunk_x,
            y: chunk_y,
        };

        if let Some(&slot_id) = self.active_mapping.get(&coord) {
            let slot = &self.physical_slots[slot_id as usize];
            if slot.current_coord == Some(coord) {
                let local_x = world_x.rem_euclid(TILE_SIZE as i32) as usize;
                let local_y = world_y.rem_euclid(TILE_SIZE as i32) as usize;
                return Some(slot.elements[local_y * TILE_SIZE as usize + local_x]);
            }
        }
        None
    }

    #[inline]
    pub fn set_element(
        &mut self,
        world_x: i32,
        world_y: i32,
        element: Element,
    ) -> Option<(TextureId, u8, u8)> {
        let chunk_x = world_x.div_euclid(TILE_SIZE as i32);
        let chunk_y = world_y.div_euclid(TILE_SIZE as i32);
        let coord = ChunkCoord {
            x: chunk_x,
            y: chunk_y,
        };

        if let Some(&slot_id) = self.active_mapping.get(&coord) {
            let slot = &mut self.physical_slots[slot_id as usize];
            if slot.current_coord == Some(coord) {
                let local_x = world_x.rem_euclid(TILE_SIZE as i32) as usize;
                let local_y = world_y.rem_euclid(TILE_SIZE as i32) as usize;

                slot.elements[local_y * TILE_SIZE as usize + local_x] = element;

                let idx = (local_y * TILE_SIZE as usize + local_x) * 4;
                if matches!(element.material, crate::element::CellType::Air) {
                    slot.pixels[idx] = 0;
                    slot.pixels[idx + 1] = 0;
                    slot.pixels[idx + 2] = 0;
                    slot.pixels[idx + 3] = 0;
                } else {
                    slot.pixels[idx] = element.rgb[0];
                    slot.pixels[idx + 1] = element.rgb[1];
                    slot.pixels[idx + 2] = element.rgb[2];
                    slot.pixels[idx + 3] = 255;
                }

                return Some((slot_id, local_x as u8, local_y as u8));
            }
        }
        None
    }

    pub fn set_element_with_diff(
        &mut self,
        world_x: i32,
        world_y: i32,
        mut element: Element,
        diffs: &mut Vec<PixelDiff>,
        current_time: u32,
        last_render_tick: u32,
    ) {
        let old_el = self
            .get_element(world_x, world_y)
            .unwrap_or(Element::empty());
        let had_diff = old_el.update_time > last_render_tick;

        element.update_time = current_time;
        if had_diff {
            element.update_index = old_el.update_index;
            if let Some((t_id, lx, ly)) = self.set_element(world_x, world_y, element) {
                let p = &self.physical_slots[t_id as usize].pixels
                    [((ly as usize * TILE_SIZE as usize) + lx as usize) * 4..];
                diffs[element.update_index as usize] = PixelDiff {
                    local_x: lx,
                    local_y: ly,
                    tile_id: t_id,
                    r: p[0],
                    g: p[1],
                    b: p[2],
                    a: p[3],
                    _padding: 0,
                };
            }
        } else {
            let diff_idx = diffs.len() as u32;
            element.update_index = diff_idx;
            if let Some((t_id, lx, ly)) = self.set_element(world_x, world_y, element) {
                let p = &self.physical_slots[t_id as usize].pixels
                    [((ly as usize * TILE_SIZE as usize) + lx as usize) * 4..];
                diffs.push(PixelDiff {
                    local_x: lx,
                    local_y: ly,
                    tile_id: t_id,
                    r: p[0],
                    g: p[1],
                    b: p[2],
                    a: p[3],
                    _padding: 0,
                });
            }
        }
    }

    pub fn swap_elements(
        &mut self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        diffs: &mut Vec<PixelDiff>,
        current_time: u32,
        last_render_tick: u32,
    ) {
        let e1 = self.get_element(x1, y1).unwrap_or(Element::empty());
        let e2 = self.get_element(x2, y2).unwrap_or(Element::empty());

        self.set_element_with_diff(x1, y1, e2, diffs, current_time, last_render_tick);
        self.set_element_with_diff(x2, y2, e1, diffs, current_time, last_render_tick);
    }

    pub fn is_in_view(
        &self,
        coord: ChunkCoord,
        camera_pos: Vec2,
        screen_size: PhysicalSize<u32>,
        zoom: f32,
    ) -> bool {
        let view_w = (screen_size.width as f32 / zoom) * 0.5;
        let view_h = (screen_size.height as f32 / zoom) * 0.5;

        // No pad here, true strict view bounds
        let min_cx = ((camera_pos.x - view_w) / TILE_SIZE as f32).floor() as i32;
        let max_cx = ((camera_pos.x + view_w) / TILE_SIZE as f32).floor() as i32;
        let min_cy = ((camera_pos.y - view_h) / TILE_SIZE as f32).floor() as i32;
        let max_cy = ((camera_pos.y + view_h) / TILE_SIZE as f32).floor() as i32;

        coord.x >= min_cx && coord.x <= max_cx && coord.y >= min_cy && coord.y <= max_cy
    }
}
