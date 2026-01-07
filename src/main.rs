// main.rs
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use winit::{
    dpi::PhysicalSize,
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
pub mod chunks;
pub mod render;
use render::{GpuScreenManager, PixelDiff, TileInstance, MAX_PHYSICAL_TEXTURES, TILE_SIZE};

use crate::chunks::ChunkManager;

// --- Chunk Storage (World State) ---

// --- Application Logic ---

struct App {
    gpu: GpuScreenManager,
    chunk_manager: ChunkManager,
    // Camera State
    camera_pos: glam::Vec2,
    zoom: f32,
    // Input State
    mouse_pressed: bool,
    mouse_pos: glam::Vec2,
    last_mouse_pos: glam::Vec2,
    pan_btn: bool,
}

impl App {
    async fn new(window: Arc<winit::window::Window>) -> Self {
        let gpu = GpuScreenManager::new(window).await;
        Self {
            gpu,
            chunk_manager: ChunkManager::new(),
            camera_pos: glam::Vec2::ZERO,
            zoom: 1.0,
            mouse_pressed: false,
            mouse_pos: glam::Vec2::ZERO,
            last_mouse_pos: glam::Vec2::ZERO,
            pan_btn: false,
        }
    }

    fn update_and_render(&mut self) {
        // 1. Game State Updates (Chunk visibility)
        let new_chunks =
            self.chunk_manager
                .update_visible_bounds(self.camera_pos, self.gpu.size, self.zoom);

        // 2. Prepare GPU Commands
        let mut load_diffs = Vec::new();
        let mut chunks_to_clear = Vec::new();

        for (slot, coords) in new_chunks {
            chunks_to_clear.push(slot);
            let restored = self.chunk_manager.load_chunk_pixels(coords, slot as u8);
            load_diffs.extend(restored);
        }

        // 3. User Interaction (Drawing)
        let mut input_diffs = Vec::new();
        if self.mouse_pressed {
            let world_x = self.camera_pos.x + (self.mouse_pos.x / self.zoom);
            let world_y = self.camera_pos.y + (self.mouse_pos.y / self.zoom);

            for dy in 0..5 {
                for dx in 0..5 {
                    let wx = (world_x + dx as f32) as i32;
                    let wy = (world_y + dy as f32) as i32;
                    let cx = (wx as f32 / TILE_SIZE as f32).floor() as i32;
                    let cy = (wy as f32 / TILE_SIZE as f32).floor() as i32;

                    if let Some(&tile_id) = self.chunk_manager.active_chunks.get(&(cx, cy)) {
                        let local_x = (wx.rem_euclid(TILE_SIZE as i32)) as u8;
                        let local_y = (wy.rem_euclid(TILE_SIZE as i32)) as u8;
                        input_diffs.push(PixelDiff {
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

        // 4. Save Input & Merge
        if !input_diffs.is_empty() {
            self.chunk_manager.save_pixels(&input_diffs);
            load_diffs.extend(input_diffs);
        }

        // 5. Submit to Renderer
        self.gpu.update_camera(self.camera_pos, self.zoom);
        self.gpu.clear_chunks(&chunks_to_clear);
        self.gpu.apply_diffs(&load_diffs);
        self.gpu.upload_instances(&self.chunk_manager.instances);
        let _ = self.gpu.render(self.chunk_manager.instances.len() as u32);
    }
}

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
                    app.camera_pos += delta / app.zoom;
                }
                app.mouse_pos = new_pos;
                app.last_mouse_pos = new_pos;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let change = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                app.zoom = (app.zoom + change * 0.1).clamp(0.1, 5.0);
            }
            _ => {}
        },
        Event::AboutToWait => window.request_redraw(),
        _ => {}
    });
}
