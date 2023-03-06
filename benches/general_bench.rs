use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use fastrand::Rng;
use snad_stack::{cells::CellType, world::World};

fn general_bench(c: &mut Criterion) {
    const WIDTH: usize = 1600;
    const HEIGHT: usize = 900;

    let mut pixels = black_box(vec![0u8; WIDTH * HEIGHT * 4]);
    let random = Rng::new();
    let choices = [CellType::Water, CellType::Sand, CellType::Stone];
    let rand_range = move |size: usize| random.usize(0..size);

    let mut group = c.benchmark_group("General Benchmarks");
    group.throughput(Throughput::Elements((WIDTH * HEIGHT) as u64));

    group.bench_function("Simulation", |b| {
        b.iter_batched_ref(
            || {
                let mut w = World::new(WIDTH as i32, HEIGHT as i32, 1);
                let p = &mut pixels.clone();
                for _ in 0..10 {
                    w.place_circle(
                        rand_range(WIDTH),
                        rand_range(HEIGHT),
                        64,
                        choices[rand_range(choices.len()) as usize],
                        true,
                        p,
                    );
                }
                black_box((w, pixels.clone()))
            },
            |(world, pixels)| {
                world.simulate(1, pixels);
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("Rendering", |b| {
        b.iter_batched_ref(
            || black_box(World::new(WIDTH as i32, HEIGHT as i32, 1)),
            |world| world.render(&mut pixels),
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, general_bench);

criterion_main!(benches);
