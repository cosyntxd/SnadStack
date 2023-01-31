use snad_stack::{
    world::World,
    cells::CellType
};
use criterion::{
    criterion_group,
    criterion_main,
    Criterion,
    Throughput,
    BatchSize,
    black_box,
};
use fastrand::Rng;

fn general_bench(c: &mut Criterion) {
    const WIDTH: usize = 1600;
    const HEIGHT: usize = 900;

    let random = Rng::new();
    let choices = [CellType::Water, CellType::Sand, CellType::Stone];
    let rand_range = move |size: usize| {
        random.usize(0..size)
    };

    let mut group = c.benchmark_group("General Benchmarks");
    group.throughput(Throughput::Elements((WIDTH*HEIGHT) as u64));

    group.bench_function("Simulation", |b| b.iter_batched_ref(
        || {
            let mut w = World::new(WIDTH as i32, HEIGHT as i32, 1);
            for _ in 0..10 {
                w.place_circle(
                    rand_range(WIDTH),
                    rand_range(HEIGHT),
                    64,
                    choices[rand_range(choices.len()) as usize],
                    true
                );
            }
            black_box(w)
        },
        |world| {
            world.simulate(1);
        },
        BatchSize::SmallInput,
    ));
    let mut pixels = [0u8; 4*WIDTH*HEIGHT];
    group.bench_function("Rendering", |b| b.iter_batched_ref(
        || {
            black_box(World::new(WIDTH as i32, HEIGHT as i32, 1))
        },
        |world| {
            world.render(&mut pixels)
        },
        BatchSize::SmallInput,
    ));
}

criterion_group!(benches, general_bench);

criterion_main!(benches);