use criterion::{criterion_group, criterion_main, Criterion};

fn empty_bench(c: &mut Criterion) {
    c.bench_function("empty", |b| b.iter(|| {}));
}

criterion_group!(benches, empty_bench);
criterion_main!(benches);
