use crate::render::{PixelDiff, TileInstance};


#[derive(Clone)]
struct ChunkStore {
    pixels: Vec<PixelDiff>,
}
impl ChunkStore {
    fn new() -> Self {
        Self { pixels: Vec::new() }
    }
}

pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}
pub struct ChunkData {
    pub pixels: Vec<PixelDiff>,
    pub instance: Option<TileInstance>,
}

pub struct ChunkManager {
    pub entries: []
    
    
    
    pub active_chunks: HashMap<ChunkCoord, ChunkId>, // World Coords -> Texture ID
    pub free_slots: VecDeque<u32>,
    pub saved_chunks: HashMap<(i32, i32), ChunkStore>, // World Coords -> RAM/Disk
    pub instances: Vec<TileInstance>,
}

impl ChunkManager {
    pub fn new() -> Self {
        let mut free_slots = VecDeque::new();
        for i in 0..MAX_PHYSICAL_TEXTURES {
            free_slots.push_back(i);
        }
        Self {
            active_chunks: HashMap::new(),
            free_slots,
            saved_chunks: HashMap::new(),
            instances: Vec::new(),
        }
    }

    pub fn update_visible_bounds(
        &mut self,
        camera_pos: glam::Vec2,
        screen_size: PhysicalSize<u32>,
        zoom: f32,
    ) -> Vec<(u32, (i32, i32))> {
        self.instances.clear();
        let view_w = screen_size.width as f32 / zoom;
        let view_h = screen_size.height as f32 / zoom;
        let chunk_size = TILE_SIZE as f32;

        let left = (camera_pos.x / chunk_size).floor() as i32 - 1;
        let top = (camera_pos.y / chunk_size).floor() as i32 - 1;
        let right = ((camera_pos.x + view_w) / chunk_size).ceil() as i32 + 1;
        let bottom = ((camera_pos.y + view_h) / chunk_size).ceil() as i32 + 1;

        let mut chunks_to_load = Vec::new();
        let center = glam::Vec2::new((left + right) as f32 / 2.0, (top + bottom) as f32 / 2.0);

        for y in top..bottom {
            for x in left..right {
                if !self.active_chunks.contains_key(&(x, y)) {
                    let slot = if let Some(s) = self.free_slots.pop_front() {
                        s
                    } else {
                        // Eviction logic
                        let (&evict_coords, &evict_slot) = self
                            .active_chunks
                            .iter()
                            .filter(|(&k, _)| {
                                k.0 < left || k.0 >= right || k.1 < top || k.1 >= bottom
                            })
                            .max_by(|(k1, _), (k2, _)| {
                                let d1 = (k1.0 as f32 - center.x).powi(2)
                                    + (k1.1 as f32 - center.y).powi(2);
                                let d2 = (k2.0 as f32 - center.x).powi(2)
                                    + (k2.1 as f32 - center.y).powi(2);
                                d1.partial_cmp(&d2).unwrap()
                            })
                            .expect("No slots available!");
                        self.active_chunks.remove(&evict_coords);
                        evict_slot
                    };
                    self.active_chunks.insert((x, y), slot);
                    chunks_to_load.push((slot, (x, y)));
                }

                if let Some(&tex_idx) = self.active_chunks.get(&(x, y)) {
                    self.instances.push(TileInstance {
                        position: [x as f32 * chunk_size, y as f32 * chunk_size],
                        texture_index: tex_idx,
                        padding: 0,
                    });
                }
            }
        }
        chunks_to_load
    }

    pub fn save_pixels(&mut self, diffs: &[PixelDiff]) {
        for diff in diffs {
            // Reverse lookup: tile_id -> chunk coord
            let chunk_coord = self
                .active_chunks
                .iter()
                .find(|(_, &val)| val == diff.tile_id as u32)
                .map(|(key, _)| *key);

            if let Some(coord) = chunk_coord {
                let store = self
                    .saved_chunks
                    .entry(coord)
                    .or_insert_with(ChunkStore::new);
                store.pixels.push(*diff);
            }
        }
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
