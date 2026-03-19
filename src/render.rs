use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3};
use std::{mem, sync::Arc};
use wgpu::{BlendState, util::DeviceExt};
use winit::{dpi::PhysicalSize, window::Window};

pub const TILE_SIZE: u32 = 256;
pub const MAX_PHYSICAL_TEXTURES: u32 = 256;
const MAX_DIFF_PER_FRAME: usize = 65536;
const MAX_INSTANCES: usize = MAX_PHYSICAL_TEXTURES as usize;

pub type TextureId = u8;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct PixelDiff {
    pub local_x: u8,
    pub local_y: u8,
    pub tile_id: TextureId,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub _padding: u8,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TileInstance {
    pub position: [f32; 2],
    pub _padding: [u8; 3],
    pub texture_id: TextureId,
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
struct ComputeParams {
    count: u32,
    _pad: [u32; 3],
}

const SHADER_SOURCE: &str = r#"
struct CameraUniform { view_proj: mat4x4<f32>, screen_size: vec2<f32>, padding: vec2<f32> };
@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct VertexInput { @location(0) position: vec2<f32>, @location(1) uv: vec2<f32> };
struct InstanceInput { @location(2) tile_world_pos: vec2<f32>, @location(3) texture_raw: u32 };
struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) @interpolate(flat) texture_index: u32 };

@vertex
fn vs_main(model: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = instance.tile_world_pos + (model.position * 256.0);
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 0.0, 1.0);
    out.uv = model.uv;

    out.texture_index = instance.texture_raw >> 24u;

    return out;
}

@group(1) @binding(0) var t_diffuse: texture_2d_array<f32>;
@group(1) @binding(1) var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, in.uv, in.texture_index);
}

struct DiffPacked { low: u32, high: u32 };
struct DiffBuffer { diffs: array<DiffPacked> };
struct Params { count: u32 };

@group(0) @binding(0) var world_textures: texture_storage_2d_array<rgba8unorm, write>;
@group(0) @binding(1) var<storage, read> input_diffs: DiffBuffer;
@group(0) @binding(2) var<uniform> params: Params;

@compute @workgroup_size(64)
fn update_world(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    if (index >= params.count) { return; }

    let packed_data = input_diffs.diffs[index];
    let x = (packed_data.low) & 0xFFu;
    let y = (packed_data.low >> 8u) & 0xFFu;
    let tile_id = (packed_data.low >> 16u) & 0xFFu;
    let r_u = (packed_data.low >> 24u) & 0xFFu;
    let g_u = (packed_data.high) & 0xFFu;
    let b_u = (packed_data.high >> 8u) & 0xFFu;
    let a_u = (packed_data.high >> 16u) & 0xFFu;

    let color = vec4<f32>(f32(r_u)/255.0, f32(g_u)/255.0, f32(b_u)/255.0, f32(a_u)/255.0);
    textureStore(world_textures, vec2<i32>(i32(x), i32(y)), i32(tile_id), color);
}
"#;

pub struct GpuScreenManager {
    surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: PhysicalSize<u32>,

    pub texture_array: wgpu::Texture,

    render_pipeline: wgpu::RenderPipeline,
    compute_pipeline: wgpu::ComputePipeline,
    // Removed clear_pipeline
    vertex_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    diff_buffer: wgpu::Buffer,
    diff_params_buffer: wgpu::Buffer,
    // Removed clear_params_buffer
    compute_bind_group: wgpu::BindGroup,
    camera_bind_group: wgpu::BindGroup,
    texture_bind_group: wgpu::BindGroup,
}

impl GpuScreenManager {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window).unwrap();
        // 1. Enumerate all available adapters on the system
        let mut selected_adapter = None;
        for adapter in instance.enumerate_adapters(wgpu::Backends::all()).await {
            let info = adapter.get_info();
            println!("{:?}", info);
            // 2. Check for the NVIDIA vendor ID (0x10DE)
            // You can also check the string: info.name.to_lowercase().contains("nvidia")
            if info.vendor == 0x10DE {
                selected_adapter = Some(adapter);
                break;
            }
        }

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
                ..Default::default()
            })
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let present_mode = if surface_caps.present_modes.contains(&wgpu::PresentMode::Immediate) {
            wgpu::PresentMode::Immediate
        } else {
            surface_caps.present_modes[0]
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_caps.formats[0],
            width: size.width,
            height: size.height,
            present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 3, // Allow the CPU to run 1 frame ahead of the GPU
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
        });

        let vertex_data: &[f32] = &[
            0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Buffer"),
            size: (MAX_INSTANCES * mem::size_of::<TileInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

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
        let diff_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Diff Buffer"),
            size: (MAX_DIFF_PER_FRAME * mem::size_of::<PixelDiff>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let diff_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Diff Params"),
            size: mem::size_of::<ComputeParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

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
            ],
            label: Some("compute_bg"),
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Layout"),
                bind_group_layouts: &[&camera_layout, &texture_layout],
                immediate_size: 0,

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
                        array_stride: 12,
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
                    blend: Some(BlendState::ALPHA_BLENDING),
                    // blend: None, // idk if alpha is needed
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Compute Layout"),
                bind_group_layouts: &[&compute_layout],
                immediate_size: 0,

            });
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Compute PL"),
            layout: Some(&compute_pipeline_layout),
            module: &shader,
            entry_point: Some("update_world"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            surface,
            device,
            queue,
            config,
            size,
            texture_array,
            render_pipeline,
            compute_pipeline,
            vertex_buffer,
            instance_buffer,
            camera_buffer,
            diff_buffer,
            diff_params_buffer,
            compute_bind_group,
            camera_bind_group,
            texture_bind_group,
        }
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn update_camera(&mut self, pos: Vec2, zoom: f32) {
        let width = self.size.width as f32;
        let height = self.size.height as f32;
        let projection = Mat4::orthographic_rh(0.0, width / zoom, height / zoom, 0.0, -1.0, 1.0);
        let view = Mat4::from_translation(Vec3::new(-pos.x, -pos.y, 0.0));
        let view_proj = projection * view;
        let uniform = CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
            screen_size: [width, height],
            padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    pub fn upload_instances(&mut self, instances: &[TileInstance]) {
        if instances.len() > MAX_INSTANCES {
            println!("Warning: Too many instances");
            return;
        }
        self.queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
    }

    pub fn update_tile(&self, tile_id: TextureId, data: &[u8]) {
        let expected_size = (TILE_SIZE * TILE_SIZE * 4) as usize;
        if data.len() != expected_size {
            return;
        }

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture_array,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: tile_id as u32,
                },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TILE_SIZE * 4),
                rows_per_image: Some(TILE_SIZE),
            },
            wgpu::Extent3d {
                width: TILE_SIZE,
                height: TILE_SIZE,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn render(
        &mut self,
        instance_count: u32,
        diffs: &[PixelDiff],
    ) -> Result<wgpu::SubmissionIndex, wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let diff_count = diffs.len();
        if diff_count > 0 {
            assert!(diff_count < MAX_DIFF_PER_FRAME);
            self.queue
                .write_buffer(&self.diff_buffer, 0, bytemuck::cast_slice(diffs));
            let params = ComputeParams {
                count: diff_count as u32,
                _pad: [0; 3],
            };
            self.queue
                .write_buffer(&self.diff_params_buffer, 0, bytemuck::cast_slice(&[params]));
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // 1. Compute Pass
        if diff_count > 0 {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            cpass.set_pipeline(&self.compute_pipeline);
            cpass.set_bind_group(0, &self.compute_bind_group, &[]);
            cpass.dispatch_workgroups((diff_count as u32 + 63) / 64, 1, 1);
        }

        // 2. Render Pass
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
                multiview_mask: None,
            });
            rpass.set_pipeline(&self.render_pipeline);
            rpass.set_bind_group(0, &self.camera_bind_group, &[]);
            rpass.set_bind_group(1, &self.texture_bind_group, &[]);
            rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            rpass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            rpass.draw(0..4, 0..instance_count);
        }

        let submission = self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(submission)
    }
}
