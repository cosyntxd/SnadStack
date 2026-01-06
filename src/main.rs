use bytemuck::{Pod, Zeroable};
use std::io::{Cursor, Write};
use std::{
    collections::{HashMap, VecDeque},
    mem,
    sync::Arc,
};
use wgpu::util::DeviceExt;
use winit::{
    dpi::PhysicalSize,
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
}; // For basic serialization
pub mod render;
// --- Constants ---
const TILE_SIZE: u32 = 256;
// Maximum number of physical textures we keep in VRAM (acting as a cache)
const MAX_PHYSICAL_TEXTURES: u32 = 256;

// --- Data Structures ---

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct PixelDiff {
    local_x: u8,
    local_y: u8,
    tile_id: u8, // Maps to the physical texture array index (0..127)
    r: u8,
    g: u8,
    b: u8,
    _padding: u16, // Ensures 8-byte alignment
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    screen_size: [f32; 2],
    padding: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct TileInstance {
    // World position of the tile
    position: [f32; 2],
    // Physical layer index (0..MAX_PHYSICAL_TEXTURES)
    texture_index: u32,
    padding: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct ComputeParams {
    count: u32,
    _pad: [u32; 3],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct ClearParams {
    texture_index: u32,
    _pad: [u32; 3],
}

// --- Shader Logic (WGSL) ---

const SHADER_SOURCE: &str = r#"
// --- Vertex Shader ---

struct CameraUniform {
    view_proj: mat4x4<f32>,
    screen_size: vec2<f32>,
    padding: vec2<f32>,
};
@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct InstanceInput {
    @location(2) tile_world_pos: vec2<f32>,
    @location(3) texture_index: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) texture_index: u32,
};

@vertex
fn vs_main(model: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    // Map unit quad to world space
    let world_pos = instance.tile_world_pos + (model.position * 256.0); 
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 0.0, 1.0);
    out.uv = model.uv;
    out.texture_index = instance.texture_index;
    return out;
}

// --- Fragment Shader ---

@group(1) @binding(0) var t_diffuse: texture_2d_array<f32>;
@group(1) @binding(1) var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, in.uv, in.texture_index);
}

// --- Compute Shader: Update World ---

struct PixelDiff {
    local_x: u32, // u8 in rust, read as u32 due to stride padding
    local_y: u32,
    tile_id: u32,
    r: u32,
    g: u32,
    b: u32,
    padding: u32, 
};

// We must manually unpack the struct because WGSL struct alignment can be tricky 
// with u8/u16. However, we used u8 in Rust. 
// Standard strategy: Use a u32 array and bitshift, OR align the Rust struct to u32s.
// The Rust struct provided is 8 bytes: 1,1,1,1,1,1,2. 
// In WGSL Storage Buffers, strict packing rules apply. 
// A safer way for this specific requested struct is to read raw u32s or vec2<u32>.
// But to keep it readable, let's treat the buffer as array<u32> (aliased) or unpack carefully.
// 
// Actually, for simplicity and robustness with the requested struct:
// Rust: [u8; 6] + [u16] = 8 bytes.
// WGSL: struct DiffPacked { data: vec2<u32> };
// We will decode bits.

struct DiffPacked {
    low: u32,  // x, y, tile, r
    high: u32, // g, b, pad
};

struct DiffBuffer {
    diffs: array<DiffPacked>,
};

struct Params {
    count: u32,
};

@group(0) @binding(0) var world_textures: texture_storage_2d_array<rgba8unorm, write>;
@group(0) @binding(1) var<storage, read> input_diffs: DiffBuffer;
@group(0) @binding(2) var<uniform> params: Params;

@compute @workgroup_size(64)
fn update_world(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    if (index >= params.count) { return; }
    
    let packed_data = input_diffs.diffs[index];
    
    // Unpack bits (Little Endian assumed for x86/WGPU standard)
    // low:  [r (8)] [tile (8)] [y (8)] [x (8)]
    // high: [pad (16)] [b (8)] [g (8)]
    
    let x       = (packed_data.low) & 0xFFu;
    let y       = (packed_data.low >> 8u) & 0xFFu;
    let tile_id = (packed_data.low >> 16u) & 0xFFu;
    let r_u     = (packed_data.low >> 24u) & 0xFFu;
    
    let g_u     = (packed_data.high) & 0xFFu;
    let b_u     = (packed_data.high >> 8u) & 0xFFu;
    
    let color = vec4<f32>(
        f32(r_u) / 255.0,
        f32(g_u) / 255.0,
        f32(b_u) / 255.0,
        1.0
    );

    textureStore(world_textures, vec2<i32>(i32(x), i32(y)), i32(tile_id), color);
}

// --- Compute Shader: Clear Tile ---

struct ClearParams {
    texture_index: u32,
};

@group(0) @binding(3) var<uniform> clear_params: ClearParams;

@compute @workgroup_size(16, 16)
fn clear_tile(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;
    textureStore(world_textures, vec2<i32>(i32(x), i32(y)), i32(clear_params.texture_index), vec4<f32>(0.0));
}
"#;

// --- Serialization & Chunk Management ---

/// Stores the pixel data for a chunk in system RAM.
/// In a real app, `data` might be loaded from disk on demand.
#[derive(Clone)]
struct ChunkStore {
    // Stores (R, G, B) tuples or indices. For simplicity, just storing a list of drawn pixels.
    // If it were a bitmap, it would be 256*256*4 bytes.
    // Here we act as a sparce storage for the demo.
    pixels: Vec<PixelDiff>,
}

impl ChunkStore {
    fn new() -> Self {
        Self { pixels: Vec::new() }
    }
}

struct ChunkManager {
    // Maps World Chunk Coordinates -> Physical Texture Index (0..127)
    active_chunks: HashMap<(i32, i32), u32>,
    // Tracks which physical slots are free
    free_slots: VecDeque<u32>,
    // Maps World Chunk Coordinates -> Saved Data (RAM/Disk)
    saved_chunks: HashMap<(i32, i32), ChunkStore>,

    // To generate the instance buffer
    instances: Vec<TileInstance>,
}

impl ChunkManager {
    fn new() -> Self {
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

    /// Determines which chunks need to be visible and assigns texture slots.
    /// Returns a list of (TextureIndex, ChunkCoord) that need to be cleared/loaded.
    fn update_visible_bounds(
        &mut self,
        camera_pos: glam::Vec2,
        screen_size: PhysicalSize<u32>,
        zoom: f32,
    ) -> Vec<(u32, (i32, i32))> {
        self.instances.clear();

        // 1. Calculate Covering Grid
        // Ensure we cover enough area even when zoomed out
        let view_w = screen_size.width as f32 / zoom;
        let view_h = screen_size.height as f32 / zoom;

        let chunk_size = TILE_SIZE as f32;

        let left_chunk = (camera_pos.x / chunk_size).floor() as i32;
        let top_chunk = (camera_pos.y / chunk_size).floor() as i32;

        let right_chunk = ((camera_pos.x + view_w) / chunk_size).ceil() as i32;
        let bottom_chunk = ((camera_pos.y + view_h) / chunk_size).ceil() as i32;

        // Add 1 tile padding to prevent edge flickering
        let start_x = left_chunk - 1;
        let end_x = right_chunk + 1;
        let start_y = top_chunk - 1;
        let end_y = bottom_chunk + 1;

        let mut required_chunks = Vec::new();
        let mut chunks_to_load = Vec::new();

        // 2. Identification Phase
        for y in start_y..end_y {
            for x in start_x..end_x {
                required_chunks.push((x, y));

                // If this chunk is not currently active
                if !self.active_chunks.contains_key(&(x, y)) {
                    // We need a slot.
                    let slot = if let Some(s) = self.free_slots.pop_front() {
                        s
                    } else {
                        // Eviction: Find a chunk that is FARTHEST from center
                        // Simple heuristic: just pick the first one not in required set
                        // (Ideally use LRU, but for this demo, random eviction of non-visible is fine)
                        let center = glam::Vec2::new(
                            (start_x + end_x) as f32 / 2.0,
                            (start_y + end_y) as f32 / 2.0,
                        );

                        let (&evict_coords, &evict_slot) = self.active_chunks.iter()
                            .filter(|(&k, _)| k.0 < start_x || k.0 >= end_x || k.1 < start_y || k.1 >= end_y)
                            .max_by(|(k1, _), (k2, _)| {
                                let d1 = (k1.0 as f32 - center.x).powi(2) + (k1.1 as f32 - center.y).powi(2);
                                let d2 = (k2.0 as f32 - center.x).powi(2) + (k2.1 as f32 - center.y).powi(2);
                                d1.partial_cmp(&d2).unwrap()
                            })
                            .expect("No slots available and all chunks visible! Increase MAX_PHYSICAL_TEXTURES.");

                        // "Unload" logic: In a full app, you might save dirty state here.
                        // Since we save on write, we just drop the mapping.
                        self.active_chunks.remove(&evict_coords);
                        evict_slot
                    };

                    self.active_chunks.insert((x, y), slot);
                    chunks_to_load.push((slot, (x, y)));
                }

                // Add to instance list for rendering
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

    // Basic Serialization Logic
    fn save_pixels(&mut self, diffs: &[PixelDiff], current_cam_chunk: (i32, i32)) {
        for diff in diffs {
            // Reconstruct World Coordinate of the chunk based on the diff's assigned physical texture
            // This is a reverse lookup. In a real engine, PixelDiff would carry world coords,
            // but the prompt structure uses tile_id. We need to find which world chunk owns this tile_id.

            // Optimization: Since we usually draw near the camera, we check active chunks.
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
                // Here is where you would also write to disk if desired
                // let _ = bincode::serialize(&store.pixels);
            }
        }
    }

    // Get all pixels needed to restore a specific chunk
    fn load_chunk_pixels(&self, coord: (i32, i32), target_tile_id: u8) -> Vec<PixelDiff> {
        if let Some(store) = self.saved_chunks.get(&coord) {
            // Remap the stored pixels to the new target physical texture ID
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

// --- GPU Manager ---

struct GpuScreenManager {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,

    render_pipeline: wgpu::RenderPipeline,
    compute_pipeline: wgpu::ComputePipeline,
    clear_pipeline: wgpu::ComputePipeline,

    vertex_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,

    diff_buffer: wgpu::Buffer,
    diff_params_buffer: wgpu::Buffer,
    clear_params_buffer: wgpu::Buffer,
    compute_bind_group: wgpu::BindGroup,

    camera_bind_group: wgpu::BindGroup,
    texture_bind_group: wgpu::BindGroup,

    camera_pos: glam::Vec2,
    zoom: f32,

    // Max capacity for buffers
    instance_capacity: usize,
    diff_capacity: usize,
}

impl GpuScreenManager {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window).unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: wgpu::Features::TEXTURE_BINDING_ARRAY
                    | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
                label: None,
            })
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let texture_format = surface_caps.formats[0];
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: texture_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Immediate,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // --- Shader ---
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
        });

        // --- Resources ---
        let vertex_data: &[f32] = &[
            0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let instance_capacity = 1024; // Support up to 1024 chunks visible
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Buffer"),
            size: (instance_capacity * mem::size_of::<TileInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create Texture Array (Cache)
        let texture_array = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("World Texture Array"),
            size: wgpu::Extent3d {
                width: TILE_SIZE,
                height: TILE_SIZE,
                depth_or_array_layers: MAX_PHYSICAL_TEXTURES,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let texture_view = texture_array.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera Buffer"),
            size: mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- Compute Resources ---
        let diff_capacity = 65536; // Max pixel updates per frame
        let diff_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Diff Buffer"),
            size: (diff_capacity * mem::size_of::<PixelDiff>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let diff_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Diff Params"),
            size: mem::size_of::<ComputeParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let clear_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Clear Params"),
            size: mem::size_of::<ClearParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- Layouts & BindGroups ---
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
            label: Some("camera_layout"),
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("camera_bg"),
        });

        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
            label: Some("texture_layout"),
        });
        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("texture_bg"),
        });

        let compute_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
            label: Some("compute_layout"),
        });
        let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &compute_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: diff_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: diff_params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: clear_params_buffer.as_entire_binding(),
                },
            ],
            label: Some("compute_bg"),
        });

        // --- Pipelines ---
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Layout"),
                bind_group_layouts: &[&camera_layout, &texture_layout],
                push_constant_ranges: &[],
            });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render PL"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: 16,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute {
                                offset: 0,
                                shader_location: 0,
                                format: wgpu::VertexFormat::Float32x2,
                            },
                            wgpu::VertexAttribute {
                                offset: 8,
                                shader_location: 1,
                                format: wgpu::VertexFormat::Float32x2,
                            },
                        ],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: 16,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute {
                                offset: 0,
                                shader_location: 2,
                                format: wgpu::VertexFormat::Float32x2,
                            },
                            wgpu::VertexAttribute {
                                offset: 8,
                                shader_location: 3,
                                format: wgpu::VertexFormat::Uint32,
                            },
                        ],
                    },
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Compute Layout"),
                bind_group_layouts: &[&compute_layout],
                push_constant_ranges: &[],
            });
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Compute PL"),
            layout: Some(&compute_pipeline_layout),
            module: &shader,
            entry_point: Some("update_world"),
            compilation_options: Default::default(),
            cache: None,
        });
        let clear_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Clear PL"),
            layout: Some(&compute_pipeline_layout),
            module: &shader,
            entry_point: Some("clear_tile"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            surface,
            device,
            queue,
            config,
            size,
            render_pipeline,
            compute_pipeline,
            clear_pipeline,
            vertex_buffer,
            instance_buffer,
            camera_buffer,
            diff_buffer,
            diff_params_buffer,
            clear_params_buffer,
            compute_bind_group,
            camera_bind_group,
            texture_bind_group,
            camera_pos: glam::Vec2::new(0.0, 0.0),
            zoom: 1.0,
            instance_capacity,
            diff_capacity,
        }
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn update_camera(&mut self) {
        let width = self.size.width as f32;
        let height = self.size.height as f32;
        let projection =
            glam::Mat4::orthographic_rh(0.0, width / self.zoom, height / self.zoom, 0.0, -1.0, 1.0);
        let view = glam::Mat4::from_translation(glam::Vec3::new(
            -self.camera_pos.x,
            -self.camera_pos.y,
            0.0,
        ));
        let view_proj = projection * view;
        let uniform = CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
            screen_size: [width, height],
            padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    fn upload_instances(&mut self, instances: &[TileInstance]) {
        if instances.len() > self.instance_capacity {
            println!("Warning: More instances than capacity. Resize buffer needed.");
            return;
        }
        self.queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
    }

    // Clears specific texture layers (when a chunk is repurposed)
    fn clear_chunks(&self, texture_indices: &[u32]) {
        if texture_indices.is_empty() {
            return;
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Clear Enc"),
            });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            cpass.set_pipeline(&self.clear_pipeline);
            cpass.set_bind_group(0, &self.compute_bind_group, &[]);
            for &idx in texture_indices {
                let params = ClearParams {
                    texture_index: idx,
                    _pad: [0; 3],
                };
                self.queue.write_buffer(
                    &self.clear_params_buffer,
                    0,
                    bytemuck::cast_slice(&[params]),
                );
                cpass.dispatch_workgroups(16, 16, 1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    fn apply_diffs(&mut self, diffs: &[PixelDiff]) {
        if diffs.is_empty() {
            return;
        }
        // Simple batching: if diffs > capacity, split it. (Here we assume it fits for demo)
        let count = diffs.len().min(self.diff_capacity);

        self.queue
            .write_buffer(&self.diff_buffer, 0, bytemuck::cast_slice(&diffs[0..count]));
        let params = ComputeParams {
            count: count as u32,
            _pad: [0; 3],
        };
        self.queue
            .write_buffer(&self.diff_params_buffer, 0, bytemuck::cast_slice(&[params]));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            cpass.set_pipeline(&self.compute_pipeline);
            cpass.set_bind_group(0, &self.compute_bind_group, &[]);
            cpass.dispatch_workgroups((count as u32 + 63) / 64, 1, 1);
        }
        self.queue.submit(Some(encoder.finish()));
    }

    fn render(&mut self, instance_count: u32) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rpass.set_pipeline(&self.render_pipeline);
            rpass.set_bind_group(0, &self.camera_bind_group, &[]);
            rpass.set_bind_group(1, &self.texture_bind_group, &[]);
            rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            rpass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            rpass.draw(0..4, 0..instance_count);
        }
        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }
}

// --- App State ---

struct App {
    gpu: GpuScreenManager,
    chunk_manager: ChunkManager,

    // Input state
    mouse_pressed: bool,
    mouse_pos: glam::Vec2,
    last_mouse_pos: glam::Vec2,
    pan_btn: bool,
}

impl App {
    async fn new(window: Arc<Window>) -> Self {
        let gpu = GpuScreenManager::new(window).await;
        let chunk_manager = ChunkManager::new();
        Self {
            gpu,
            chunk_manager,
            mouse_pressed: false,
            mouse_pos: glam::Vec2::ZERO,
            last_mouse_pos: glam::Vec2::ZERO,
            pan_btn: false,
        }
    }

    fn update_and_render(&mut self) {
        // 1. Update Camera
        self.gpu.update_camera();

        // 2. Calculate Visible Chunks (Dynamic Sizing Fix)
        // This calculates exactly which chunks are needed to cover the view, addressing the resize issue.
        let new_chunks = self.chunk_manager.update_visible_bounds(
            self.gpu.camera_pos,
            self.gpu.size,
            self.gpu.zoom,
        );

        // 3. Process Newly Loaded Chunks
        // If a new physical slot was assigned, clear it on GPU and load saved pixels if any
        let mut load_diffs = Vec::new();
        let mut chunks_to_clear = Vec::new();

        for (slot, coords) in new_chunks {
            chunks_to_clear.push(slot);
            // Deserialize/Load: Check if we have data for this world coord
            let restored_pixels = self.chunk_manager.load_chunk_pixels(coords, slot as u8);
            load_diffs.extend(restored_pixels);
        }

        // Clear textures that were reassigned
        self.gpu.clear_chunks(&chunks_to_clear);

        // Upload restored pixels
        if !load_diffs.is_empty() {
            self.gpu.apply_diffs(&load_diffs);
        }

        // Upload Instance Buffer
        self.gpu.upload_instances(&self.chunk_manager.instances);

        // 4. Input Drawing
        let mut new_diffs = Vec::new();
        if self.mouse_pressed {
            let world_x = self.gpu.camera_pos.x + (self.mouse_pos.x / self.gpu.zoom);
            let world_y = self.gpu.camera_pos.y + (self.mouse_pos.y / self.gpu.zoom);

            // Draw a 5x5 block
            for dy in 0..5 {
                for dx in 0..5 {
                    let wx = (world_x + dx as f32) as i32;
                    let wy = (world_y + dy as f32) as i32;

                    let cx = (wx as f32 / TILE_SIZE as f32).floor() as i32;
                    let cy = (wy as f32 / TILE_SIZE as f32).floor() as i32;

                    // Only draw if this chunk is currently loaded/visible
                    if let Some(&tile_id) = self.chunk_manager.active_chunks.get(&(cx, cy)) {
                        let local_x = (wx.rem_euclid(TILE_SIZE as i32)) as u8;
                        let local_y = (wy.rem_euclid(TILE_SIZE as i32)) as u8;

                        new_diffs.push(PixelDiff {
                            local_x,
                            local_y,
                            tile_id: tile_id as u8,
                            r: 255,
                            g: 0,
                            b: 255,
                            _padding: 0,
                        });
                    }
                }
            }
        }

        // 5. Apply User Changes
        if !new_diffs.is_empty() {
            // Save to CPU store (Serialize)
            let center_chunk_x = (self.gpu.camera_pos.x / TILE_SIZE as f32).floor() as i32;
            let center_chunk_y = (self.gpu.camera_pos.y / TILE_SIZE as f32).floor() as i32;
            self.chunk_manager
                .save_pixels(&new_diffs, (center_chunk_x, center_chunk_y));

            // Send to GPU
            self.gpu.apply_diffs(&new_diffs);
        }

        // 6. Render
        let _ = self.gpu.render(self.chunk_manager.instances.len() as u32);
    }
}

// --- Entry Point ---

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Sand Serialization")
            .build(&event_loop)
            .unwrap(),
    );
    let mut app = pollster::block_on(App::new(window.clone()));

    let _ = event_loop.run(move |event, target| match event {
        Event::WindowEvent { event, window_id } if window_id == window.id() => match event {
            WindowEvent::CloseRequested => target.exit(),
            WindowEvent::RedrawRequested => app.update_and_render(),
            WindowEvent::Resized(size) => app.gpu.resize(size),
            WindowEvent::MouseInput { state, button, .. } => match button {
                MouseButton::Left => app.mouse_pressed = state == ElementState::Pressed,
                MouseButton::Right => app.pan_btn = state == ElementState::Pressed,
                _ => {}
            },
            WindowEvent::CursorMoved { position, .. } => {
                let new_pos = glam::Vec2::new(position.x as f32, position.y as f32);
                if app.pan_btn {
                    let delta = app.last_mouse_pos - new_pos;
                    app.gpu.camera_pos += delta / app.gpu.zoom;
                }
                app.mouse_pos = new_pos;
                app.last_mouse_pos = new_pos;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let change = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                app.gpu.zoom = (app.gpu.zoom + change * 0.1).clamp(0.1, 5.0);
            }
            _ => {}
        },
        Event::AboutToWait => window.request_redraw(),
        _ => {}
    });
}
