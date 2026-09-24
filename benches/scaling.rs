use criterion::{
    AxisScale, BatchSize, BenchmarkId, Criterion, PlotConfiguration, criterion_group,
    criterion_main,
};
use rand::{RngExt, SeedableRng, rngs::Xoshiro256PlusPlus};
use rmq::{base::RMQ, rmq_tree::RMQTree, sparse_table::OffsetSparseTable};

fn scaling_sparse_table(c: &mut Criterion) {
    let mut group = c.benchmark_group("sparse_table");
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    // Half-decade steps: 10, 31, 100, 316, ..., 1_000_000
    let sizes = (2..=12).map(|i| 10f64.powf(i as f64 / 2.0) as usize);

    for n in sizes {
        if n >= 100_000 {
            group.sample_size(10); // minimum allowed; keeps big sizes from taking forever
        }
        let data: Vec<u64> = (0..n as u64).rev().collect();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(42);

        group.bench_with_input(BenchmarkId::from_parameter(n), &data, |b, data| {
            b.iter_batched(
                || OffsetSparseTable::new(&data), // setup, not timed
                |v| {
                    let l = rng.random_range(0..n);
                    let r = rng.random_range(l..n);
                    v.rmq(l, r);
                }, // timed
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();

    group = c.benchmark_group("rmq_tree");
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    // Half-decade steps: 10, 31, 100, 316, ..., 1_000_000
    let sizes = (2..=12).map(|i| 10f64.powf(i as f64 / 2.0) as usize);

    for n in sizes {
        if n >= 100_000 {
            group.sample_size(10); // minimum allowed; keeps big sizes from taking forever
        }
        let data: Vec<u64> = (0..n as u64).rev().collect();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(42);

        group.bench_with_input(BenchmarkId::from_parameter(n), &data, |b, data| {
            b.iter_batched(
                || RMQTree::new(&data), // setup, not timed
                |v| {
                    let l = rng.random_range(0..n);
                    let r = rng.random_range(l..n);
                    v.rmq(l, r);
                }, // timed
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

criterion_group!(benches, scaling_sparse_table);
criterion_main!(benches);
