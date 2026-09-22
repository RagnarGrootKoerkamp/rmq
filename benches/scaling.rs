use criterion::{
    AxisScale, BatchSize, BenchmarkId, Criterion, PlotConfiguration, criterion_group,
    criterion_main,
};

fn scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("sort");
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    // Half-decade steps: 10, 31, 100, 316, ..., 1_000_000
    let sizes = (2..=12).map(|i| 10f64.powf(i as f64 / 2.0) as usize);

    for n in sizes {
        if n >= 100_000 {
            group.sample_size(10); // minimum allowed; keeps big sizes from taking forever
        }
        let data: Vec<u64> = (0..n as u64).rev().collect();

        group.bench_with_input(BenchmarkId::from_parameter(n), &data, |b, data| {
            b.iter_batched(
                || data.clone(),  // setup, not timed
                |mut v| v.sort(), // timed
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

criterion_group!(benches, scaling);
criterion_main!(benches);
