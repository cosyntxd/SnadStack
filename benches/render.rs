use criterion::{black_box, criterion_group, criterion_main, Criterion};
use glam::Vec2;
use std::sync::Arc;
use winit::dpi::PhysicalSize;
use winit::window::WindowBuilder;
use winit::event_loop::EventLoop;

use sand_sim::render::*;
use sand_sim::chunks::*;

async fn setup_gpu() -> (GpuScreenManager, Arc<winit::window::Window>) {
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        WindowBuilder::new()
            .with_visible(false)
            .build(&event_loop)
            .unwrap(),
    );
    let gpu_manager = GpuScreenManager::new(window.clone()).await;
    (gpu_manager, window)
}
fn main() {
    
}
