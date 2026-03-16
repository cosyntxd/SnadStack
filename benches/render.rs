use criterion::{black_box, criterion_group, criterion_main, Criterion};
use glam::Vec2;
use std::sync::Arc;
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

use sand_sim::chunks::{ChunkManager, CHUNK_BYTE_SIZE};
use sand_sim::render::{GpuScreenManager, TileInstance};

async fn setup_gpu(event_loop: &EventLoop<()>) -> (GpuScreenManager, Arc<winit::window::Window>) {
    let window = Arc::new(
        WindowBuilder::new()
            .with_visible(true) // MUST be visible on Windows to avoid present() blocking forever
            .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0))
            .build(event_loop)
            .unwrap(),
    );
    let gpu_manager = GpuScreenManager::new(window.clone()).await;
    (gpu_manager, window)
}

fn bench_chunk_logic(c: &mut Criterion) {
    let mut manager = ChunkManager::new();
    let camera_pos = Vec2::new(100.0, 100.0);
    let screen_size = PhysicalSize::new(1280, 720);
    let zoom = 1.0;

    c.bench_function("chunk_manager_update_logic", |b| {
        b.iter(|| {
            // Benchmark the CPU visibility and command generation logic
            manager.update(
                black_box(camera_pos),
                black_box(screen_size),
                black_box(zoom),
            )
        })
    });
}

fn bench_wgpu_operations(c: &mut Criterion) {
    let _ = env_logger::builder().is_test(true).filter_level(log::LevelFilter::Warn).try_init();
    
    // Use an event loop that we can pump
    let mut event_loop = EventLoop::new().unwrap();
    let (gpu_manager, _window) = pollster::block_on(setup_gpu(&event_loop));

    let dummy_data = [0u8; CHUNK_BYTE_SIZE];

    c.bench_function("gpu_texture_upload_256x256", |b| {
        b.iter(|| {
            gpu_manager.update_tile(black_box(0), black_box(&dummy_data));
        })
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default().significance_level(0.1).sample_size(50);
    targets = bench_wgpu_operations, bench_chunk_logic
);
criterion_main!(benches);
