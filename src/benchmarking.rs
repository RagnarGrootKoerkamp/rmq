use std::collections::BTreeMap;
use std::env;
use std::hint::black_box;
use std::time::Instant;

use crate::base::{RMQ, RMQFamily};
use crate::data::{RMQValue, RangeDistribution, get_rng};
use crate::sparse_table::OffsetSparseTable;
use mem_dbg::{MemSize, SizeFlags};

use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct DistStatistics {
    mean: f64,
    std_dev: f64,
    median: f64,
}

#[derive(Debug, Serialize)]
pub struct BenchmarkResult {
    name: &'static str,
    params: BTreeMap<&'static str, usize>,
    distribution: String,
    n: usize,
    build_time: Option<DistStatistics>,
    query_time: DistStatistics,
    query_latency: DistStatistics,
    memory_bytes: usize,
    query_batch_time: Option<DistStatistics>,
}

fn verify_function<T: Copy, R, F, G, V>(
    mut f: F,
    mut verifier: V,
    total_repetitions: usize,
    mut input_generator: G,
) -> bool
where
    R: Eq,
    F: FnMut(T) -> R,
    V: FnMut(T) -> R,
    G: FnMut() -> T,
{
    for _ in 0..total_repetitions {
        let input = input_generator();
        let expected = verifier(input);
        let actual = f(input);
        if expected != actual {
            return false;
        }
    }
    true
}

fn time_function<T: Copy, R, F, G>(
    mut f: F,
    warmup_repetitions: usize,
    repetitions: usize,
    batch_size: usize,
    mut input_generator: G,
) -> DistStatistics
where
    F: FnMut(T) -> R,
    G: FnMut() -> T,
{
    for _ in 0..warmup_repetitions {
        black_box(f(input_generator()));
    }

    let mut remaining = repetitions + batch_size;

    let mut durations: Vec<f64> = Vec::with_capacity(remaining);
    let mut outputs: Vec<R> = Vec::with_capacity(batch_size);
    let mut inputs: Vec<T> = vec![];
    while remaining > 0 {
        let cbatch = batch_size.min(remaining);
        remaining -= cbatch;

        inputs.clear();
        inputs.extend((0..cbatch).map(|_| input_generator()));

        let t = Instant::now();
        for i in &inputs {
            outputs.push(f(*i));
        }
        durations.push(t.elapsed().as_secs_f64() / cbatch as f64);
        black_box(&outputs);
        outputs.clear();
    }
    // Remove warmup batch
    durations.remove(0);

    let mean = durations.iter().sum::<f64>() / durations.len() as f64;
    let std_dev = if durations.len() > 1 {
        (durations.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
            / (durations.len() as f64 - 1f64))
            .sqrt()
    } else {
        0f64
    };
    durations.sort_by(f64::total_cmp);
    let median = durations[durations.len() / 2];
    DistStatistics {
        mean,
        std_dev,
        median,
    }
}

fn time_function_batches<T: Copy, F, G>(
    mut f: F,
    warmup_repetitions: usize,
    repetitions: usize,
    batch_size: usize,
    mut input_generator: G,
) -> DistStatistics
where
    F: FnMut(&[T], &mut [usize]),
    G: FnMut() -> T,
{
    assert_eq!(repetitions % batch_size, 0);
    let mut warmup_outputs: Vec<usize> = vec![0; warmup_repetitions];
    let warmup_inputs: Vec<T> = (0..warmup_repetitions).map(|_| input_generator()).collect();
    for (data, output) in warmup_inputs
        .chunks_exact(batch_size)
        .zip(warmup_outputs.chunks_exact_mut(batch_size))
    {
        f(data, output);
    }
    black_box(&warmup_outputs);

    let mut outputs: Vec<usize> = vec![0; repetitions];
    let inputs: Vec<T> = (0..repetitions).map(|_| input_generator()).collect();
    let t = Instant::now();
    for (data, output) in inputs
        .chunks_exact(batch_size)
        .zip(outputs.chunks_exact_mut(batch_size))
    {
        f(data, output);
    }
    black_box(&outputs);
    let duration = t.elapsed().as_secs_f64() / repetitions as f64;

    DistStatistics {
        mean: duration,
        std_dev: 0f64,
        median: duration,
    }
}

const QUERY_WARMUP_REPETITIONS: usize = 100_000;
const QUERY_REPETITIONS: usize = 1_000_000;

/// Returns (query time, query latency) over ranges drawn from `dist`.
pub fn time_queries<'a, T: 'a, R: RMQ<'a, T>>(
    rmq: &R,
    dist: RangeDistribution,
    n: usize,
) -> (DistStatistics, DistStatistics, Option<DistStatistics>) {
    // Independent queries: the CPU overlaps their cache misses, so this measures throughput.
    let mut query_rng = get_rng();
    let query_time = time_function(
        |(l, r)| {
            black_box(rmq.rmq(black_box(l), black_box(r)));
        },
        QUERY_WARMUP_REPETITIONS,
        QUERY_REPETITIONS,
        1000,
        || dist.sample_range(&mut query_rng, n),
    );

    // Each query's bounds depend on the previous result (`prev & zero` is always 0, but neither
    // the compiler nor the CPU knows that), so queries run one at a time: this measures latency.
    let zero = black_box(0usize);
    let mut prev = 0usize;
    let mut query_rng = get_rng();
    let query_latency = time_function(
        |(l, r)| {
            let d = prev & zero;
            prev = rmq.rmq(l + d, r + d);
            // A predicted branch inside rmq yields `prev` before its loads resolve, which would
            // let the next query start early. lfence waits until this query has fully completed.
            #[cfg(target_arch = "x86_64")]
            unsafe {
                core::arch::x86_64::_mm_lfence()
            };
            prev
        },
        QUERY_WARMUP_REPETITIONS,
        QUERY_REPETITIONS,
        1000,
        || dist.sample_range(&mut query_rng, n),
    );

    let mut batch_rng = get_rng();
    let batch_time = Some(time_function_batches(
        |queries, output| {
            rmq.rmq_batch(black_box(queries), black_box(output));
        },
        QUERY_WARMUP_REPETITIONS,
        QUERY_REPETITIONS,
        1000,
        || dist.sample_range(&mut batch_rng, n),
    ));

    (query_time, query_latency, batch_time)
}

/// Each family is benchmarked until its structure would exceed this many bytes.
const MEMORY_THRESHOLD_BYTES: usize = 1 << 32;

/// Returns `None` if the structure built over `n` elements exceeds the memory threshold.
pub fn run_benchmarks<T: RMQValue, F: RMQFamily<T>>(
    dist: RangeDistribution,
    n: usize,
    verify: bool,
    data: &[T],
    include_build: bool,
) -> Option<BenchmarkResult> {
    let build_time = if include_build {
        Some(time_function(
            |_| F::Rmq::new(black_box(data)),
            0,
            1,
            1,
            || {},
        ))
    } else {
        None
    };

    let rmq = F::Rmq::new(data);
    let memory_bytes = rmq.mem_size(SizeFlags::default());
    if env::var("IGNORE_MEM_THRESHOLD").is_err() && memory_bytes > MEMORY_THRESHOLD_BYTES {
        return None;
    }

    let (query_time, query_latency, batch_time) = time_queries(&rmq, dist, n);

    if verify {
        // Verify the benchmark outputs by rerunning them and comparing against OffsetSparsetable
        let offset_table = OffsetSparseTable::new(data);
        let mut query_rng = get_rng();
        if !verify_function(
            |(l, r)| rmq.rmq(l, r),
            |(l, r)| offset_table.rmq(l, r),
            QUERY_WARMUP_REPETITIONS + QUERY_REPETITIONS,
            || dist.sample_range(&mut query_rng, n),
        ) {
            panic!(
                "Verify failed: {} {:?} n={n} ({})",
                F::NAME,
                F::PARAMS,
                dist.name()
            )
        }
    }

    Some(BenchmarkResult {
        name: F::NAME,
        params: F::PARAMS.iter().copied().collect(),
        distribution: dist.name(),
        n,
        build_time,
        query_time,
        query_latency,
        memory_bytes,
        query_batch_time: batch_time,
    })
}

pub fn run_dfs_benchmark<T: RMQValue, F: RMQFamily<T>>(
    n: usize,
    _verify: bool,
    data: &[T],
    _include_build: bool,
) -> Option<BenchmarkResult> {
    let rmq = F::Rmq::new(data);
    let memory_bytes = rmq.mem_size(SizeFlags::default());
    if env::var("IGNORE_MEM_THRESHOLD").is_err() && memory_bytes > MEMORY_THRESHOLD_BYTES {
        return None;
    }

    let mut query_count = 0usize;
    let mut query_time = time_function(
        |()| {
            let (_query_count, cs) = suffix_tree_dfs(&rmq, 0, n - 1);
            std::hint::black_box(cs);
            query_count = _query_count;
        },
        1,
        1,
        1,
        || (),
    );
    assert_eq!(query_count, n - 1);
    query_time.mean /= query_count as f64;
    query_time.median /= query_count as f64;
    query_time.std_dev /= query_count as f64;

    Some(BenchmarkResult {
        name: F::NAME,
        params: F::PARAMS.iter().copied().collect(),
        distribution: "dfs".to_owned(),
        n,
        build_time: None,
        query_time: query_time.clone(),
        query_latency: query_time,
        memory_bytes,
        query_batch_time: None,
    })
}

fn suffix_tree_dfs<'a, T>(lcp: &impl RMQ<'a, T>, l: usize, r: usize) -> (usize, usize) {
    let mut stack: Vec<(usize, usize)> = vec![(l, r)];
    let mut queries = 0usize;
    let mut checksum = 0usize;

    while let Some((l, r)) = stack.pop() {
        if l >= r {
            continue; // Leaf
        }
        let m = lcp.rmq(l + 1, r);
        queries += 1;
        checksum = checksum.wrapping_add(m);

        stack.push((m, r));
        stack.push((l, m - 1));
    }

    (queries, checksum)
}
