use criterion::{criterion_group, criterion_main, Criterion};
use glam::Vec2;
use sand_sim::element::{CellType, Element};
use sand_sim::render::TILE_SIZE;
use sand_sim::world::World;
use winit::dpi::PhysicalSize;

fn bench_simulation(c: &mut Criterion) {
    let mut world = World::new();

    world.screen_size = PhysicalSize::new(10 * TILE_SIZE, 10 * TILE_SIZE);
    world.zoom = 1.0;
    world.camera_pos = Vec2::new(5.0 * TILE_SIZE as f32, 5.0 * TILE_SIZE as f32);

    world
        .chunks
        .update(world.camera_pos, world.screen_size, world.zoom);

    for i in 0..100 {
        let cx = (i * 12345 % (10 * TILE_SIZE)) as i32;
        let cy = (i * 54321 % (10 * TILE_SIZE)) as i32;

        let material = if i % 2 == 0 {
            CellType::Sand
        } else {
            CellType::Stone
        };

        let mut el = Element::empty();
        el.material = material;
        if material == CellType::Sand {
            el.rgb = [200, 200, 50];
        } else {
            el.rgb = [100, 100, 100];
        }

        let r = 20;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    world.chunks.set_element_with_diff(
                        cx + dx,
                        cy + dy,
                        el,
                        &mut world.streamer,
                        world.ticks,
                        world.last_render_tick,
                    );
                }
            }
        }
    }

    for i in 0..0 {
        let wx = (100 + i * 200) as f32;
        let wy = 100.0;
        let body = world.physics.spawn_dynamic_rect(wx, wy, 10, 10);
        for el in body.elements.iter_mut() {
            el.material = CellType::Stone;
            el.rgb = [150, 150, 150];
        }
    }

    if let Some(s) = &mut world.streamer { s.current_diffs.clear(); }

    c.bench_function("simulate_10x10_scattered", |b| {
        b.iter(|| {
            world.simulate_step();
        });
    });
}

criterion_group!(benches, bench_simulation);
criterion_main!(benches);
