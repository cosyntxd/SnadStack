mod chunks;
mod render;

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::thread;

use chunks::{ChunkCommand, ChunkCoord, ChunkManager, CHUNK_BYTE_SIZE};
use glam::Vec2;
use render::{GpuScreenManager, PixelDiff, TILE_SIZE};
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

const DATA_DIR: &str = "world_data";

struct ChunkRequest {
    coord: ChunkCoord,
}

struct ChunkResult {
    coord: ChunkCoord,
    data: Vec<u8>,
}

fn main() {
    env_logger::init();

    if !Path::new(DATA_DIR).exists() {
        fs::create_dir(DATA_DIR).expect("Failed to create world directory");
    }

    let (tx_req, rx_req) = mpsc::channel::<ChunkRequest>();
    let (tx_res, rx_res) = mpsc::channel::<ChunkResult>();

    thread::spawn(move || {
        while let Ok(req) = rx_req.recv() {
            let mut buffer = vec![0u8; CHUNK_BYTE_SIZE];
            
            if !load_chunk_from_disk(req.coord, &mut buffer) {
                generate_procedural_chunk(req.coord, &mut buffer);
            }

            let _ = tx_res.send(ChunkResult {
                coord: req.coord,
                data: buffer,
            });
        }
    });

    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Sand")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
            .build(&event_loop)
            .unwrap(),
    );

    let mut gpu_manager = pollster::block_on(GpuScreenManager::new(window.clone()));
    let mut chunk_manager = ChunkManager::new();
    let mut pending_uploads: HashMap<ChunkCoord, (u32, Arc<[u8; 262144]>)> = HashMap::new();

    let mut camera_pos = Vec2::ZERO; 
    let mut zoom = 1.0;
    
    let mut mouse_pos = PhysicalPosition::new(0.0, 0.0);
    let mut is_drawing = false;
    let mut is_panning = false;
    let mut last_mouse_pos = PhysicalPosition::new(0.0, 0.0);

    let mut frame_count = 0;
    let mut last_frame_time = std::time::Instant::now();

    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => target.exit(),
                
                WindowEvent::Resized(new_size) => {
                    gpu_manager.resize(new_size);
                }

                WindowEvent::CursorMoved { position, .. } => {
                    mouse_pos = position;
                    
                    if is_panning {
                        let dx = position.x - last_mouse_pos.x;
                        let dy = position.y - last_mouse_pos.y;
                        
                        camera_pos.x -= dx as f32 / zoom;
                        camera_pos.y -= dy as f32 / zoom;
                    }
                    
                    last_mouse_pos = position;
                }

                WindowEvent::MouseInput { state, button, .. } => {
                    match button {
                        MouseButton::Left => {
                            is_drawing = state == ElementState::Pressed;
                        }
                        MouseButton::Middle | MouseButton::Right => {
                            is_panning = state == ElementState::Pressed;
                        }
                        _ => {}
                    }
                }
                
                WindowEvent::MouseWheel { delta, .. } => {
                    let scroll_y = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.01,
                    };

                    let zoom_sensitivity = 0.1;
                    let target_zoom = zoom * (1.0 + scroll_y * zoom_sensitivity);
                    let min_zoom = gpu_manager.size.width as f32 / 3500.0; 
                    let max_zoom = 5.0;

                    zoom = target_zoom.clamp(min_zoom, max_zoom);
                }
                
                WindowEvent::RedrawRequested => {
                    while let Ok(result) = rx_res.try_recv() {
                        if let Some((physical_id, engine_buffer_arc)) = pending_uploads.remove(&result.coord) {
                            
                            // SAFETY: good enough
                            unsafe {
                                let dest_ptr = engine_buffer_arc.as_ptr() as *mut u8;
                                std::ptr::copy_nonoverlapping(result.data.as_ptr(), dest_ptr, CHUNK_BYTE_SIZE);
                                
                                let slice = std::slice::from_raw_parts(dest_ptr, CHUNK_BYTE_SIZE);
                                gpu_manager.update_tile(physical_id as u8, slice);
                            }
                        }
                    }

                    let view_width = gpu_manager.size.width as f32 / zoom;
                    let view_height = gpu_manager.size.height as f32 / zoom;
                    
                    let top_left_cam = Vec2::new(
                        camera_pos.x - view_width / 2.0,
                        camera_pos.y - view_height / 2.0
                    );

                    gpu_manager.update_camera(top_left_cam, zoom);
                    let commands = chunk_manager.update(camera_pos, gpu_manager.size, zoom);
                    
                    for command in commands {
                        match command {
                            ChunkCommand::UploadToGpu { physical_id, coord, data } => {
                                pending_uploads.insert(coord, (physical_id as u32, data));
                                tx_req.send(ChunkRequest { coord }).unwrap();
                            }
                            ChunkCommand::EvictedFromGpu { coord, data } => {
                                pending_uploads.remove(&coord);
                                save_chunk_to_disk(coord, &*data);
                                chunk_manager.release_chunk(coord);
                            }
                        }
                    }

                    if is_drawing {
                        let diffs = handle_drawing(
                            &mut chunk_manager, 
                            &gpu_manager, 
                            mouse_pos, 
                            top_left_cam, 
                            zoom
                        );
                        if !diffs.is_empty() {
                            gpu_manager.apply_diffs(&diffs);
                        }
                    }

                    let instances = chunk_manager.get_instances();
                    gpu_manager.upload_instances(&instances);
                    
                    match gpu_manager.render(instances.len() as u32) {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => gpu_manager.resize(gpu_manager.size),
                        Err(wgpu::SurfaceError::OutOfMemory) => target.exit(),
                        Err(e) => eprintln!("{:?}", e),
                    }

                    frame_count += 1;
                    if last_frame_time.elapsed().as_secs() >= 1 {
                        println!("FPS: {}", frame_count);
                        frame_count = 0;
                        last_frame_time = std::time::Instant::now();
                    }
                }
                _ => {}
            },
            Event::AboutToWait => {
                window.request_redraw();
            }
            _ => {}
        }
    });
}

fn get_chunk_path(coord: ChunkCoord) -> std::path::PathBuf {
    Path::new(DATA_DIR).join(format!("chunk_{}_{}.bin", coord.x, coord.y))
}

fn load_chunk_from_disk(coord: ChunkCoord, buffer: &mut [u8]) -> bool {
    let path = get_chunk_path(coord);
    if path.exists() {
        if let Ok(mut file) = File::open(path) {
            if file.read_exact(buffer).is_ok() {
                return true;
            }
        }
    }
    false
}

fn save_chunk_to_disk(coord: ChunkCoord, data: &[u8]) {
    let path = get_chunk_path(coord);
    if let Ok(mut file) = File::create(path) {
        let _ = file.write_all(data);
    }
}

fn generate_procedural_chunk(coord: ChunkCoord, buffer: &mut [u8]) {
    for y in 0..TILE_SIZE as i32 {
        for x in 0..TILE_SIZE as i32 {
            let world_x = (coord.x * TILE_SIZE as i32) + x;
            let world_y = (coord.y * TILE_SIZE as i32) + y;

            let v1 = (world_x as f32 * 0.01).sin();
            let v2 = (world_y as f32 * 0.01).cos();
            let v3 = ((world_x as f32 * 0.005) + (world_y as f32 * 0.005)).sin();

            let val = ((v1 + v2 + v3) / 3.0 * 127.0 + 128.0) as u8;

            let idx = ((y * TILE_SIZE as i32 + x) * 4) as usize;
            buffer[idx] = val;         
            buffer[idx + 1] = val;     
            buffer[idx + 2] = val / 2; 
            buffer[idx + 3] = 255;     
        }
    }
}

fn handle_drawing(
    chunk_manager: &mut ChunkManager, 
    _gpu_manager: &GpuScreenManager,
    mouse_pos: PhysicalPosition<f64>,
    top_left_camera: Vec2, 
    zoom: f32
) -> Vec<PixelDiff> {
    let mut diffs = Vec::new();

    let world_x = top_left_camera.x + (mouse_pos.x as f32 / zoom);
    let world_y = top_left_camera.y + (mouse_pos.y as f32 / zoom);

    let chunk_x = (world_x / TILE_SIZE as f32).floor() as i32;
    let chunk_y = (world_y / TILE_SIZE as f32).floor() as i32;
    let coord = ChunkCoord { x: chunk_x, y: chunk_y };

    let local_x = (world_x as i32).rem_euclid(TILE_SIZE as i32);
    let local_y = (world_y as i32).rem_euclid(TILE_SIZE as i32);

    if let Some(chunk_data_arc) = chunk_manager.cpu_backed_buffer.get(&coord) {
        if let Some(&texture_id) = chunk_manager.active_mapping.get(&coord) {
            
            let data = unsafe { 
                let ptr = chunk_data_arc.as_ptr() as *mut u8;
                std::slice::from_raw_parts_mut(ptr, CHUNK_BYTE_SIZE) 
            };

            let brush_size = 5;
            for dy in -brush_size..=brush_size {
                for dx in -brush_size..=brush_size {
                    if dx*dx + dy*dy > brush_size*brush_size { continue; }

                    let lx = local_x + dx;
                    let ly = local_y + dy;

                    if lx >= 0 && lx < TILE_SIZE as i32 && ly >= 0 && ly < TILE_SIZE as i32 {
                        let idx = ((ly * TILE_SIZE as i32 + lx) * 4) as usize;
                        
                        data[idx] = 255;
                        data[idx+1] = 0;
                        data[idx+2] = 0;
                        data[idx+3] = 255;

                        diffs.push(PixelDiff {
                            local_x: lx as u8,
                            local_y: ly as u8,
                            tile_id: texture_id,
                            r: 255, g: 0, b: 0, _padding: 0,
                        });
                    }
                }
            }
        }
    }

    diffs
}
