use criterion::{criterion_group, criterion_main, Criterion};

pub fn find_bench(c: &mut Criterion) {
    // begin setup
                                             
    c.bench_function("client_request", |_| {
    });
}

criterion_group!(benches, find_bench);
criterion_main!(benches);