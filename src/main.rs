mod bodies;
mod chunks;
mod element;
mod render;
mod world;

use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use chunks::{ChunkCommand, ChunkCoord, CHUNK_BYTE_SIZE, CHUNK_ELEMENTS};
use element::{CellType, Element};
use glam::Vec2;
use render::{GpuScreenManager, TILE_SIZE};
use world::World;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowBuilder;

struct ChunkRequest {
    coord: ChunkCoord,
}

struct ChunkResult {
    coord: ChunkCoord,
    pixels: Box<[u8; CHUNK_BYTE_SIZE]>,
    elements: Box<[Element; CHUNK_ELEMENTS]>,
}

fn main() {
    env_logger::init();

    let (tx_req, rx_req) = mpsc::channel::<ChunkRequest>();
    let (tx_res, rx_res) = mpsc::channel::<ChunkResult>();

    let rx_req = Arc::new(Mutex::new(rx_req));

    // Background thread pool for generating chunks
    for _ in 0..4 {
        let rx_req = Arc::clone(&rx_req);
        let tx_res = tx_res.clone();
        thread::spawn(move || loop {
            let req = {
                let rx = rx_req.lock().unwrap();
                rx.recv()
            };

            match req {
                Ok(req) => {
                    let mut pixels: Box<[u8; CHUNK_BYTE_SIZE]> = vec![0u8; CHUNK_BYTE_SIZE]
                        .into_boxed_slice()
                        .try_into()
                        .unwrap();
                    let mut elements: Box<[Element; CHUNK_ELEMENTS]> =
                        vec![Element::empty(); CHUNK_ELEMENTS]
                            .into_boxed_slice()
                            .try_into()
                            .unwrap();
                    generate_procedural_chunk(req.coord, pixels.as_mut(), elements.as_mut());
                    let _ = tx_res.send(ChunkResult {
                        coord: req.coord,
                        pixels,
                        elements,
                    });
                }
                Err(_) => break, // Channel closed
            }
        });
    }

    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Sand")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
            .build(&event_loop)
            .unwrap(),
    );

    let mut gpu_manager = pollster::block_on(GpuScreenManager::new(window.clone()));
    let mut world = World::new();

    let mut mouse_pos = PhysicalPosition::new(0.0, 0.0);
    let mut is_drawing = false;
    let mut place_material = CellType::Sand;
    let mut is_panning = false;
    let mut last_mouse_pos = PhysicalPosition::new(0.0, 0.0);

    let mut frame_count = 0;
    let mut last_frame_time = std::time::Instant::now();

    let mut last_tick_time = std::time::Instant::now();
    let tick_rate = std::time::Duration::from_secs_f64(1.0 / 50.0);

    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => target.exit(),

                WindowEvent::Resized(new_size) => {
                    gpu_manager.resize(new_size);
                    world.screen_size = new_size;
                }

                WindowEvent::CursorMoved { position, .. } => {
                    mouse_pos = position;

                    if is_panning {
                        let dx = position.x - last_mouse_pos.x;
                        let dy = position.y - last_mouse_pos.y;

                        world.camera_pos.x -= dx as f32 / world.zoom;
                        world.camera_pos.y -= dy as f32 / world.zoom;
                    }

                    last_mouse_pos = position;
                }

                WindowEvent::MouseInput { state, button, .. } => match button {
                    MouseButton::Left => {
                        is_drawing = state == ElementState::Pressed;
                        if is_drawing {
                            place_material = CellType::Sand;
                        }
                    }
                    MouseButton::Right => {
                        is_drawing = state == ElementState::Pressed;
                        if is_drawing {
                            place_material = CellType::Air;
                        }
                    }
                    MouseButton::Middle => {
                        is_panning = state == ElementState::Pressed;
                    }
                    _ => {}
                },

                WindowEvent::KeyboardInput { event, .. } => {
                    if event.state == ElementState::Pressed {
                        if let PhysicalKey::Code(KeyCode::KeyQ) = event.physical_key {
                            let world_x = world.camera_pos.x
                                - ((gpu_manager.size.width as f32 / world.zoom) * 0.5)
                                + (mouse_pos.x as f32 / world.zoom);
                            let world_y = world.camera_pos.y
                                - ((gpu_manager.size.height as f32 / world.zoom) * 0.5)
                                + (mouse_pos.y as f32 / world.zoom);

                            let width = 20;
                            let height = 10;
                            let mut elements = Vec::with_capacity(width * height);
                            for _ in 0..(width * height) {
                                let mut el = Element::empty();
                                el.material = CellType::Stone;
                                el.rgb = [100, 100, 100];
                                elements.push(el);
                            }

                            world.physics.spawn_rigid_from_pixels(
                                world_x,
                                world_y,
                                width,
                                height,
                                elements,
                            );
                        }

                        if let PhysicalKey::Code(KeyCode::KeyB) = event.physical_key {
                            let world_x = world.camera_pos.x
                                - ((gpu_manager.size.width as f32 / world.zoom) * 0.5)
                                + (mouse_pos.x as f32 / world.zoom);
                            let world_y = world.camera_pos.y
                                - ((gpu_manager.size.height as f32 / world.zoom) * 0.5)
                                + (mouse_pos.y as f32 / world.zoom);
                            let base_x = world_x as i32;
                            let base_y = world_y as i32;

                            for y in 0..20 {
                                for x in 0..50 {
                                    let mut el = Element::empty();
                                    el.material = CellType::Brick;
                                    el.rgb = [180, 40, 40];
                                    world.chunks.set_element_with_diff(
                                        base_x + x,
                                        base_y + y,
                                        el,
                                        &mut world.queued_pixels,
                                        world.ticks,
                                        world.last_render_tick,
                                    );
                                }
                            }
                        }
                    }
                }

                WindowEvent::MouseWheel { delta, .. } => {
                    let scroll_y = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.01,
                    };

                    let zoom_sensitivity = 0.1;
                    let target_zoom = world.zoom * (1.0 + scroll_y * zoom_sensitivity);
                    let min_zoom = gpu_manager.size.width as f32 / 4300.0;
                    let max_zoom = 4.0;

                    world.zoom = target_zoom.clamp(min_zoom, max_zoom);
                }

                WindowEvent::RedrawRequested => {
                    // 1. Process Loaded Chunks
                    while let Ok(result) = rx_res.try_recv() {
                        if let Some(physical_id) = world.chunks.get_chunk_slot(result.coord)
                        {
                            let slot = &mut world.chunks.physical_slots[physical_id as usize];
                            // Ensure the slot hasn't been reassigned to another coordinate while this one generated
                            if slot.current_coord == Some(result.coord) {
                                slot.elements = result.elements;
                                gpu_manager.update_tile(physical_id, result.pixels.as_ref());
                            }
                        }
                    }

                    // 2. Update Camera View
                    let view_width = gpu_manager.size.width as f32 / world.zoom;
                    let view_height = gpu_manager.size.height as f32 / world.zoom;

                    let top_left_cam = Vec2::new(
                        world.camera_pos.x - view_width / 2.0,
                        world.camera_pos.y - view_height / 2.0,
                    );

                    gpu_manager.update_camera(top_left_cam, world.zoom);

                    // 3. Update Chunk Visibility
                    let commands = world.chunks.update(world.camera_pos, gpu_manager.size, world.zoom);

                    for command in commands {
                        match command {
                            ChunkCommand::UploadToGpu {
                                physical_id: _,
                                coord,
                            } => {
                                tx_req.send(ChunkRequest { coord }).unwrap();
                            }
                            ChunkCommand::EvictedFromGpu { .. } => {
                                // Eviction is handled implicitly by the physical slot being overwritten
                            }
                        }
                    }

                    // 4. Handle Brush/Drawing logic
                    if is_drawing {
                        handle_drawing(
                            &mut world,
                            mouse_pos,
                            top_left_cam,
                            place_material,
                        );
                    }

                    // 5. Render to Screen
                    let instances = world.chunks.get_instances();
                    gpu_manager.upload_instances(&instances);

                    let diff_count = world.queued_pixels.len();

                    if diff_count >= 65536 {
                        log::warn!(
                            "Too many diffs ({}), truncating to 65535 to prevent panic",
                            diff_count
                        );
                    }

                    let render_result = gpu_manager.render(
                        instances.len() as u32,
                        &world.queued_pixels[..diff_count.min(65535)],
                    );

                    if render_result.is_ok() {
                        world.last_render_tick = world.ticks;
                        world.queued_pixels.clear();
                    } else {
                        world.last_render_tick = world.ticks;
                        world.queued_pixels.clear();
                        match render_result {
                            Err(wgpu::SurfaceError::Lost) => {
                                gpu_manager.resize(gpu_manager.size);
                            }
                            Err(wgpu::SurfaceError::OutOfMemory) => target.exit(),
                            Err(e) => {
                                eprintln!("{:?}", e);
                            }
                            _ => {}
                        }
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
                let now = std::time::Instant::now();
                while now.duration_since(last_tick_time) >= tick_rate {
                    last_tick_time += tick_rate;
                    world.simulate_step();
                }
                window.request_redraw();
            }
            _ => {}
        }
    });
}

fn generate_procedural_chunk(coord: ChunkCoord, pixels: &mut [u8], elements: &mut [Element]) {
    for y in 0..TILE_SIZE as i32 {
        let world_y = (coord.y * TILE_SIZE as i32) + y;
        let v2 = (world_y as f32 * 0.02).cos();

        for x in 0..TILE_SIZE as i32 {
            let world_x = (coord.x * TILE_SIZE as i32) + x;

            let v1 = (world_x as f32 * 0.02).sin();
            let v3 = ((world_x as f32 * 0.01) + (world_y as f32 * 0.01)).sin();

            let val = (v1 + v2 + v3) / 3.0;

            let mut el = Element::empty();

            if val > 0.85 {
                el.material = CellType::Sand;
                el.rgb = [200, 200, 50];
            } else if val > 0.75 {
                el.material = CellType::Brick;
                el.rgb = [150, 50, 50];
            }

            let idx = (y * TILE_SIZE as i32 + x) as usize;
            elements[idx] = el;

            let p_idx = idx * 4;
            pixels[p_idx] = el.rgb[0];
            pixels[p_idx + 1] = el.rgb[1];
            pixels[p_idx + 2] = el.rgb[2];
            pixels[p_idx + 3] = if matches!(el.material, CellType::Air) {
                0
            } else {
                255
            };
        }
    }
}

fn handle_drawing(
    world: &mut world::World,
    mouse_pos: PhysicalPosition<f64>,
    top_left_camera: Vec2,
    place_material: CellType,
) {
    let world_x = top_left_camera.x + (mouse_pos.x as f32 / world.zoom);
    let world_y = top_left_camera.y + (mouse_pos.y as f32 / world.zoom);

    let base_x = world_x as i32;
    let base_y = world_y as i32;

    let brush_size = 5;
    for dy in -brush_size..=brush_size {
        for dx in -brush_size..=brush_size {
            if dx * dx + dy * dy > brush_size * brush_size {
                continue;
            }

            let mut el = Element::empty();
            el.material = place_material;
            if matches!(place_material, CellType::Sand) {
                el.rgb = [200, 200, 50];
            } else if matches!(place_material, CellType::Air) {
                el.rgb = [0, 0, 0];
            }

            world.chunks.set_element_with_diff(
                base_x + dx,
                base_y + dy,
                el,
                &mut world.queued_pixels,
                world.ticks,
                world.last_render_tick,
            );
        }
    }
}
// displaced search ts up on the db
