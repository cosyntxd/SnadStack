use std::collections::HashMap;

use winit::dpi::PhysicalSize;

use crate::render::{PixelDiff, TILE_SIZE, TileInstance};

#[derive(Default, Clone)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}
#[derive(Default, Clone)]
pub struct ChunkData {
    pub pixels: Vec<PixelDiff>,
    pub instance: Option<TileInstance>,
    pub location: Option<ChunkCoord>,
    pub used: bool,
}

pub struct ChunkManager {
    pub entries: [ChunkData; 255],
    pub active_chunks: HashMap<ChunkCoord, u8>,
}

impl ChunkManager {
    pub fn new() -> Self {
        let empty_chunk = ChunkData::default();
        Self { entries: [empty_chunk.clone(); 255], active_chunks: HashMap::new() }
    }
    pub fn update_visible_bounds(
        &mut self,
        camera_pos: glam::Vec2,
        screen_size: PhysicalSize<u32>,
        zoom: f32,
    ) {
    }

    pub fn save_pixels(&mut self, diffs: &[PixelDiff]) {
    }

    pub fn load_chunk_pixels(&self, coord: (i32, i32), target_tile_id: u8) -> Vec<PixelDiff> {
        if let Some(store) = self.saved_chunks.get(&coord) {
            store
                .pixels
                .iter()
                .map(|p| PixelDiff {
                    tile_id: target_tile_id,
                    ..*p
                })
                .collect()
        } else {
            Vec::new()
        }
    }
}
