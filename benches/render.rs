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
            .with_visible(false) // internet says must be true but idk
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

    let mut event_loop = EventLoop::new().unwrap();
    let (mut gpu_manager, _window) = pollster::block_on(setup_gpu(&event_loop));

    let dummy_data = [0u8; CHUNK_BYTE_SIZE];

    let mut upload_submissions = std::collections::VecDeque::new();
    c.bench_function("gpu_texture_upload_256x256", |b| {
        b.iter(|| {
            gpu_manager.update_tile(black_box(0), black_box(&dummy_data));
            let idx = gpu_manager.queue.submit([]);
            upload_submissions.push_back(idx);
            if upload_submissions.len() > 3 {
                let old_idx = upload_submissions.pop_front().unwrap();
                let _ = gpu_manager.device.poll(wgpu::PollType::Wait { submission_index: Some(old_idx), timeout: None });
            } else {
                let _ = gpu_manager.device.poll(wgpu::PollType::Poll);
            }
        })
    });

    let mut instances = Vec::new();
    for i in 0..100 {
        let mut random_data = [0u8; CHUNK_BYTE_SIZE];
        for p in (0..random_data.len()).step_by(4) {
            random_data[p] = (p.wrapping_add(i * 13) % 256) as u8;       // R
            random_data[p + 1] = (p.wrapping_add(i * 17) % 256) as u8;   // G
            random_data[p + 2] = (p.wrapping_add(i * 19) % 256) as u8;   // B
            random_data[p + 3] = 255;                                    // A
        }
        gpu_manager.update_tile(i as u8, &random_data);

        instances.push(TileInstance {
            position: [(i % 10) as f32 * 256.0, (i / 10) as f32 * 256.0],
            _padding: [0; 3],
            texture_id: i as u8,
        });
    }

    let mut group = c.benchmark_group("rendering");
    // group.sample_size(10); // Rendering is heavy, fewer samples is fine

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    use winit::platform::pump_events::EventLoopExtPumpEvents;

    let mut render_submissions = std::collections::VecDeque::new();
    group.bench_function("render_100_random_tiles_in_view", |b| {
        b.iter(|| {
            #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
            let _ = event_loop.pump_events(Some(std::time::Duration::ZERO), |_, _| {});

            gpu_manager.update_camera(Vec2::ZERO, 1.0);
            gpu_manager.upload_instances(&instances);
            let idx = match gpu_manager.render(instances.len() as u32, &[]) {
                Ok(idx) => Some(idx),
                Err(wgpu::SurfaceError::Lost) => {
                    gpu_manager.resize(gpu_manager.size);
                    None
                }
                Err(e) => {
                    log::error!("Render error: {:?}", e);
                    None
                }
            };
            if let Some(idx) = idx {
                render_submissions.push_back(idx);
                if render_submissions.len() > 3 {
                    let old_idx = render_submissions.pop_front().unwrap();
                    let _ = gpu_manager.device.poll(wgpu::PollType::Wait { submission_index: Some(old_idx), timeout: None });
                } else {
                    let _ = gpu_manager.device.poll(wgpu::PollType::Poll);
                }
            }
        })
    });
    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default().significance_level(0.1).sample_size(150);
    targets = bench_wgpu_operations, bench_chunk_logic
);
criterion_main!(benches);
