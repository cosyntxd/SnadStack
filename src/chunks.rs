use glam::Vec2;
use rustc_hash::FxHashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use winit::dpi::PhysicalSize;

use crate::render::{TextureId, TileInstance, MAX_PHYSICAL_TEXTURES, TILE_SIZE};

// 256 * 256 * 4 bytes (RGBA)
pub const CHUNK_AREA: usize = (TILE_SIZE * TILE_SIZE) as usize;
pub const CHUNK_BYTE_SIZE: usize = CHUNK_AREA * 4;

const BUFFER_POOL_SIZE: usize = 32;

#[derive(Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}

pub type ChunkBuffer = Arc<[u8; CHUNK_BYTE_SIZE]>;

#[derive(Clone)]
pub struct PhysicalSlot {
    pub current_coord: Option<ChunkCoord>,
    pub last_visible_frame: u64,
    pub is_empty: bool,
}

impl Default for PhysicalSlot {
    fn default() -> Self {
        Self {
            current_coord: None,
            last_visible_frame: 0,
            is_empty: true,
        }
    }
}

pub enum ChunkCommand {
    UploadToGpu {
        physical_id: TextureId,
        coord: ChunkCoord,
        data: ChunkBuffer,
    },
    EvictedFromGpu {
        coord: ChunkCoord,
        data: ChunkBuffer,
    },
}

struct BufferPool {
    pool: VecDeque<ChunkBuffer>,
}

impl BufferPool {
    fn new(capacity: usize) -> Self {
        let mut pool = VecDeque::with_capacity(capacity);
        for _ in 0..capacity {
            pool.push_back(Arc::new([0u8; CHUNK_BYTE_SIZE]));
        }
        Self { pool }
    }

    fn acquire(&mut self) -> ChunkBuffer {
        self.pool
            .pop_front()
            .unwrap_or_else(|| Arc::new([0u8; CHUNK_BYTE_SIZE]))
    }

    fn release(&mut self, buffer: ChunkBuffer) {
        if self.pool.len() < BUFFER_POOL_SIZE {
            self.pool.push_back(buffer);
        }
    }
}

pub struct ChunkManager {
    pub physical_slots: [PhysicalSlot; MAX_PHYSICAL_TEXTURES as usize],
    pub active_mapping: FxHashMap<ChunkCoord, TextureId>,
    pub cpu_backed_buffer: FxHashMap<ChunkCoord, ChunkBuffer>,
    // todo: bad optimization?
    cached_instances: Vec<TileInstance>,
    dirty: bool,

    buffer_pool: BufferPool,
    frame_counter: u64,
}

impl ChunkManager {
    pub fn new() -> Self {
        let physical_slots = std::array::from_fn(|_| PhysicalSlot::default());

        Self {
            physical_slots,
            active_mapping: FxHashMap::default(),
            cpu_backed_buffer: FxHashMap::default(),
            cached_instances: Vec::with_capacity(MAX_PHYSICAL_TEXTURES as usize),
            dirty: false,
            buffer_pool: BufferPool::new(BUFFER_POOL_SIZE),
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

        let pad = 1;
        let view_w = (screen_size.width as f32 / zoom) * 0.5;
        let view_h = (screen_size.height as f32 / zoom) * 0.5;

        let min_cx = ((camera_pos.x - view_w) / TILE_SIZE as f32).floor() as i32 - pad;
        let max_cx = ((camera_pos.x + view_w) / TILE_SIZE as f32).floor() as i32 + pad;
        let min_cy = ((camera_pos.y - view_h) / TILE_SIZE as f32).floor() as i32 - pad;
        let max_cy = ((camera_pos.y + view_h) / TILE_SIZE as f32).floor() as i32 + pad;

        let mut commands = Vec::new();
        let mut needed_coords = Vec::with_capacity(16);

        for y in min_cy..=max_cy {
            for x in min_cx..=max_cx {
                let coord = ChunkCoord { x, y };

                if let Some(&id) = self.active_mapping.get(&coord) {
                    self.physical_slots[id as usize].last_visible_frame = self.frame_counter;
                } else {
                    needed_coords.push(coord);
                }
            }
        }

        if needed_coords.is_empty() {
            return commands;
        }

        for coord in needed_coords {
            let slot_id = self.find_best_slot();

            if !self.physical_slots[slot_id as usize].is_empty {
                let old_coord = self.physical_slots[slot_id as usize].current_coord.unwrap();

                if let Some(data) = self.cpu_backed_buffer.get(&old_coord).cloned() {
                    commands.push(ChunkCommand::EvictedFromGpu {
                        coord: old_coord,
                        data,
                    });
                }

                self.active_mapping.remove(&old_coord);
                self.dirty = true;
            }

            let data = if let Some(existing_data) = self.cpu_backed_buffer.get(&coord) {
                existing_data.clone()
            } else {
                let buf = self.buffer_pool.acquire();
                self.cpu_backed_buffer.insert(coord, buf.clone());
                buf
            };

            self.physical_slots[slot_id as usize] = PhysicalSlot {
                current_coord: Some(coord),
                last_visible_frame: self.frame_counter,
                is_empty: false,
            };
            self.active_mapping.insert(coord, slot_id);
            self.dirty = true; // todo: bad optimization

            commands.push(ChunkCommand::UploadToGpu {
                physical_id: slot_id,
                coord,
                data,
            });
        }

        if self.dirty {
            self.rebuild_instances();
            self.dirty = false;
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

    fn rebuild_instances(&mut self) {
        self.cached_instances.clear();
        for (coord, &id) in &self.active_mapping {
            self.cached_instances.push(TileInstance {
                position: [
                    coord.x as f32 * TILE_SIZE as f32,
                    coord.y as f32 * TILE_SIZE as f32,
                ],
                _padding: [0; 3],
                texture_id: id,
            });
        }
    }

    pub fn get_instances(&self) -> &[TileInstance] {
        &self.cached_instances
    }

    pub fn release_chunk(&mut self, coord: ChunkCoord) {
        if let Some(buf) = self.cpu_backed_buffer.remove(&coord) {
            self.buffer_pool.release(buf);
        }
    }
}
