use glam::Vec2;
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
    pub current_coord: ChunkCoord,
    pub elements: Box<[Element; CHUNK_ELEMENTS]>,
    pub sim_regions: [u8; 32], // 32*8 = 16px^2 regions
    pub last_visible_frame: u64,
}

impl Default for PhysicalSlot {
    fn default() -> Self {
        Self {
            current_coord: ChunkCoord { x: 0, y: 0 },
            last_visible_frame: 0,
            elements: vec![Element::empty(); CHUNK_ELEMENTS]
                .into_boxed_slice()
                .try_into()
                .unwrap(),
            sim_regions: [0xFF; 32],
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
    pub active_mapping: [Option<(ChunkCoord, TextureId)>; 4096],
    pub visible_chunks: Vec<ChunkCoord>,
    pub frame_counter: u64,
}

impl ChunkManager {
    pub fn new() -> Self {
        let physical_slots = std::array::from_fn(|_| PhysicalSlot::default());

        Self {
            physical_slots,
            active_mapping: [None; 4096],
            visible_chunks: Vec::new(),
            frame_counter: 0,
        }
    }
    #[inline(never)]

    fn map_index(x: i32, y: i32) -> usize {
        ((x & 63) as usize) | (((y & 63) as usize) << 6)
    }
    #[inline(never)]

    pub fn has_chunk(&self, coord: ChunkCoord) -> bool {
        let idx = Self::map_index(coord.x, coord.y);
        if let Some((c, _)) = self.active_mapping[idx] {
            c == coord
        } else {
            false
        }
    }

    #[inline(never)]
    pub fn get_chunk_slot(&self, coord: ChunkCoord) -> Option<TextureId> {
        let idx = Self::map_index(coord.x, coord.y);
        if let Some((c, id)) = self.active_mapping[idx] {
            if c == coord {
                return Some(id);
            }
        }
        None
    }
    #[inline(never)]

    pub fn update(
        &mut self,
        camera_pos: Vec2,
        screen_size: PhysicalSize<u32>,
        zoom: f32,
    ) -> Vec<ChunkCommand> {
        self.frame_counter += 1;

        let pad = 1;
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

            let safe_radius = ((MAX_PHYSICAL_TEXTURES as f32).sqrt() / 2.0).floor() as i32 - 1;
            let center_cx = (camera_pos.x / TILE_SIZE as f32).floor() as i32;
            let center_cy = (camera_pos.y / TILE_SIZE as f32).floor() as i32;

            min_cx = center_cx - safe_radius;
            max_cx = center_cx + safe_radius;
            min_cy = center_cy - safe_radius;
            max_cy = center_cy + safe_radius;
        }

        let mut commands = Vec::new();
        let mut newly_visible = Vec::with_capacity(128);
        self.visible_chunks.clear();

        for y in min_cy..=max_cy {
            for x in min_cx..=max_cx {
                let coord = ChunkCoord { x, y };
                self.visible_chunks.push(coord);

                let idx = Self::map_index(x, y);
                let mut found = false;
                if let Some((c, id)) = self.active_mapping[idx] {
                    if c == coord {
                        self.physical_slots[id as usize].last_visible_frame = self.frame_counter;
                        found = true;
                    }
                }

                if !found {
                    newly_visible.push(coord);
                }
            }
        }

        for slot_id in 0..MAX_PHYSICAL_TEXTURES {
            let slot = &mut self.physical_slots[slot_id as usize];

            let coord = slot.current_coord;
            commands.push(ChunkCommand::EvictedFromGpu { coord });

            let idx = Self::map_index(coord.x, coord.y);
            if let Some((c, _)) = self.active_mapping[idx] {
                if c == coord {
                    self.active_mapping[idx] = None;
                }
            }
        }

        for coord in newly_visible {
            let slot_id = self.find_best_slot();

            let slot = &mut self.physical_slots[slot_id as usize];

            let old_coord = slot.current_coord;
            commands.push(ChunkCommand::EvictedFromGpu { coord: old_coord });
            let old_idx = Self::map_index(old_coord.x, old_coord.y);
            if let Some((c, _)) = self.active_mapping[old_idx] {
                if c == old_coord {
                    self.active_mapping[old_idx] = None;
                }
            }

            slot.current_coord = coord;
            slot.last_visible_frame = self.frame_counter;

            let idx = Self::map_index(coord.x, coord.y);
            self.active_mapping[idx] = Some((coord, slot_id));

            commands.push(ChunkCommand::UploadToGpu {
                physical_id: slot_id,
                coord,
            });
        }

        commands
    }
    #[inline(never)]

    fn find_best_slot(&self) -> TextureId {
        let mut lru_id = 0;
        let mut min_frame = u64::MAX;

        for (i, slot) in self.physical_slots.iter().enumerate() {
            if slot.last_visible_frame < min_frame {
                min_frame = slot.last_visible_frame;
                lru_id = i;
            }
        }

        lru_id as TextureId
    }
    #[inline(never)]

    pub fn get_instances(&self) -> Vec<TileInstance> {
        let mut instances = Vec::with_capacity(256);
        for (i, slot) in self.physical_slots.iter().enumerate() {
            instances.push(TileInstance {
                position: [
                    slot.current_coord.x as f32 * TILE_SIZE as f32,
                    slot.current_coord.y as f32 * TILE_SIZE as f32,
                ],
                _padding: [0; 3],
                texture_id: i as TextureId,
            });
        }
        instances
    }
    #[inline(never)]

    pub fn get_element(&self, world_x: i32, world_y: i32) -> Option<Element> {
        let chunk_x = world_x >> 8;
        let chunk_y = world_y >> 8;

        let idx = Self::map_index(chunk_x, chunk_y);
        if let Some((c, slot_id)) = self.active_mapping[idx] {
            if c.x == chunk_x && c.y == chunk_y {
                let slot = &self.physical_slots[slot_id as usize];
                let local_x = (world_x & 255) as usize;
                let local_y = (world_y & 255) as usize;
                return Some(slot.elements[local_y * TILE_SIZE as usize + local_x]);
            }
        }
        None
    }

    #[inline(never)]
    pub fn set_element(
        &mut self,
        world_x: i32,
        world_y: i32,
        element: Element,
    ) -> Option<(TextureId, u8, u8)> {
        let chunk_x = world_x >> 8;
        let chunk_y = world_y >> 8;

        let idx = Self::map_index(chunk_x, chunk_y);
        if let Some((c, slot_id)) = self.active_mapping[idx] {
            if c.x == chunk_x && c.y == chunk_y {
                let slot = &mut self.physical_slots[slot_id as usize];
                let local_x = (world_x & 255) as usize;
                let local_y = (world_y & 255) as usize;

                slot.elements[local_y * TILE_SIZE as usize + local_x] = element;

                return Some((slot_id as TextureId, local_x as u8, local_y as u8));
            }
        }
        None
    }
    #[inline(never)]

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
                diffs[element.update_index as usize] = PixelDiff {
                    local_x: lx,
                    local_y: ly,
                    tile_id: t_id,
                    r: element.rgb[0],
                    g: element.rgb[1],
                    b: element.rgb[2],
                    a: if matches!(element.material, crate::element::CellType::Air) {
                        0
                    } else {
                        255
                    },
                    _padding: 0,
                };
            }
        } else {
            let diff_idx = diffs.len() as u32;
            element.update_index = diff_idx;
            if let Some((t_id, lx, ly)) = self.set_element(world_x, world_y, element) {
                diffs.push(PixelDiff {
                    local_x: lx,
                    local_y: ly,
                    tile_id: t_id,
                    r: element.rgb[0],
                    g: element.rgb[1],
                    b: element.rgb[2],
                    a: if matches!(element.material, crate::element::CellType::Air) {
                        0
                    } else {
                        255
                    },
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
    #[inline(never)]

    pub fn is_in_view(
        &self,
        coord: ChunkCoord,
        camera_pos: Vec2,
        screen_size: PhysicalSize<u32>,
        zoom: f32,
    ) -> bool {
        let view_w = (screen_size.width as f32 / zoom) * 0.5;
        let view_h = (screen_size.height as f32 / zoom) * 0.5;

        let min_cx = ((camera_pos.x - view_w) / TILE_SIZE as f32).floor() as i32;
        let max_cx = ((camera_pos.x + view_w) / TILE_SIZE as f32).floor() as i32;
        let min_cy = ((camera_pos.y - view_h) / TILE_SIZE as f32).floor() as i32;
        let max_cy = ((camera_pos.y + view_h) / TILE_SIZE as f32).floor() as i32;

        coord.x >= min_cx && coord.x <= max_cx && coord.y >= min_cy && coord.y <= max_cy
    }
}
