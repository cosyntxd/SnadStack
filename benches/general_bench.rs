use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use fastrand::Rng;
use snad_stack::{cells::CellType, world::World};

fn general_bench(c: &mut Criterion) {
    const WIDTH: u32 = 1600;
    const HEIGHT: u32 = 900;

    let mut pixels = black_box(vec![0u8; (WIDTH * HEIGHT * 4) as usize]);
    let random = Rng::new();
    let choices = [CellType::Water, CellType::Sand, CellType::Stone];
    let rand_range = move |size: u32| random.u32(0..size);

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
                        rand_range(WIDTH),
                        rand_range(HEIGHT),
                        8,
                        choices[rand_range(choices.len() as u32) as usize],
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

    group.bench_function("Placing", |b| {
        b.iter_batched_ref(
            || {
                let mut w = World::new(WIDTH as i32, HEIGHT as i32, 1);
                let p = &mut pixels.clone();
                let x = rand_range(WIDTH);
                let y = rand_range(HEIGHT);

                black_box((w, pixels.clone(), x, y))
            },
            |(world, pixels, x, y)| {
                world.place_circle(
                    *x,
                    *y,
                    (*x as i32 + fastrand::i32(-64..64).max(0)) as u32,
                    (*y as i32 + fastrand::i32(-64..64).max(0)) as u32,
                    64,
                    choices[rand_range(choices.len() as u32) as usize],
                    true,
                    pixels,
                );
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, general_bench);

criterion_main!(benches);
