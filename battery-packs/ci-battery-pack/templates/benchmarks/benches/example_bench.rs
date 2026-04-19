use {{ crate_name }}::add;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn bench_add(c: &mut Criterion) {
    let mut group = c.benchmark_group("add");
    for (left, right) in [(1, 2), (100, 200), (u64::MAX / 2, u64::MAX / 2)] {
        group.bench_with_input(
            BenchmarkId::new("add", format!("{left}+{right}")),
            &(left, right),
            |b, &(l, r)| b.iter(|| add(black_box(l), black_box(r))),
        );
    }
    group.finish();
}

criterion_group!(benches, bench_add);
criterion_main!(benches);
