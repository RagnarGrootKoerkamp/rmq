use std::fs::File;
use std::hint::black_box;
use std::io::{BufWriter, Write};
use std::time::Instant;

use clap::Parser;
use rmq::base::{MemoryUsage, RMQ, RMQFamily};
use rmq::sparse_table::OffsetSparseTable;
use rmq::*;

use rand::prelude::*;
use rand::rngs::Xoshiro256PlusPlus;
use rand::seq::SliceRandom;
use serde::Serialize;

fn get_rng() -> Xoshiro256PlusPlus {
    Xoshiro256PlusPlus::seed_from_u64(42)
}

trait RMQValue: Copy + Ord {
    /// Truncating cast, like `x as Self`.
    fn from_u64(x: u64) -> Self;
}
impl RMQValue for u8 {
    fn from_u64(x: u64) -> Self {
        x as u8
    }
}
impl RMQValue for u16 {
    fn from_u64(x: u64) -> Self {
        x as u16
    }
}
impl RMQValue for u32 {
    fn from_u64(x: u64) -> Self {
        x as u32
    }
}
impl RMQValue for u64 {
    fn from_u64(x: u64) -> Self {
        x
    }
}

/// Values wrap around if `n` exceeds `T`'s range, so the result may contain duplicates.
fn get_random_permutation<T: RMQValue>(rng: &mut dyn Rng, n: usize) -> Vec<T> {
    let mut v: Vec<T> = (0..n as u64).map(T::from_u64).collect();
    v.shuffle(rng);
    v
}

trait RangeDistribution {
    fn sample_range(rng: &mut dyn Rng, n: usize) -> (usize, usize);
}

struct FullyRandomRangeDistribution;
impl RangeDistribution for FullyRandomRangeDistribution {
    fn sample_range(rng: &mut dyn Rng, n: usize) -> (usize, usize) {
        let mut l = rng.random_range(0..n);
        let mut r = rng.random_range(0..n);
        while l > r {
            l = rng.random_range(0..n);
            r = rng.random_range(0..n);
        }
        (l, r)
    }
}

// Small as in r-l < 128
struct SmallRangesDistribution;
impl RangeDistribution for SmallRangesDistribution {
    fn sample_range(rng: &mut dyn Rng, n: usize) -> (usize, usize) {
        let mut l = rng.random_range(0..n);
        let mut r = rng.random_range(l..l + 128);
        while r >= n {
            l = rng.random_range(0..n);
            r = rng.random_range(l..l + 128);
        }
        (l, r)
    }
}

#[derive(Debug, Serialize)]
struct DistStatistics {
    mean: f64,
    std_dev: f64,
    median: f64,
}

#[derive(Debug, Serialize)]
struct BenchmarkResult {
    name: String,
    n: usize,
    build_time: DistStatistics,
    query_time: DistStatistics,
    query_latency: DistStatistics,
    memory_bytes: usize,
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
    return true;
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

/// Each family is benchmarked until its structure would exceed this many bytes.
const MEMORY_THRESHOLD_BYTES: usize = 1 << 32;

/// Returns `None` if the structure built over `n` elements exceeds the memory threshold.
fn run_benchmarks<T: RMQValue, F: RMQFamily<T>, R: RangeDistribution>(
    n: usize, verify: bool
) -> Option<BenchmarkResult> {
    let mut rng = get_rng();
    let data = get_random_permutation(&mut rng, n);

    let rmq = F::Rmq::new(&data);
    let memory_bytes = rmq.memory_total();
    if memory_bytes > MEMORY_THRESHOLD_BYTES {
        return None;
    }
    drop(rmq);

    let build_time = time_function(|_| F::Rmq::new(black_box(&data)), 0, 1, 1, || {});

    let rmq = F::Rmq::new(&data);

    let warmup_repetitions = 100_000;
    let repetitions = 1_000_000;

    // Independent queries: the CPU overlaps their cache misses, so this measures throughput.
    let mut query_rng = get_rng();
    let query_time = time_function(
        |(l, r)| {
            black_box(rmq.rmq(black_box(l), black_box(r)));
        },
        warmup_repetitions,
        repetitions,
        1000,
        || R::sample_range(&mut query_rng, n),
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
            prev
        },
        warmup_repetitions,
        repetitions,
        1000,
        || R::sample_range(&mut query_rng, n),
    );

    if verify {
        // Verify the benchmark outputs by rerunning them and comparing against OffsetSparsetable
        let offset_table = OffsetSparseTable::new(&data);
        verify_function(
            |(l, r)| rmq.rmq(l, r),
            |(l, r)| offset_table.rmq(l, r),
            warmup_repetitions + repetitions,
            || R::sample_range(&mut query_rng, n),
        );
    }

    Some(BenchmarkResult {
        name: std::any::type_name::<F>().to_string(),
        n,
        build_time,
        query_time,
        query_latency,
        memory_bytes,
    })
}

#[cfg(target_os = "macos")]
fn prefer_performance_cores() {
    unsafe extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
    }
    const QOS_CLASS_USER_INTERACTIVE: u32 = 0x21;
    unsafe { pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0) };
}

const MAX_N: usize = 1e7 as usize;

fn run_family<T: RMQValue, F: RMQFamily<T>>(out: &mut impl Write) -> std::io::Result<()> {
    // Tenth-of-a-decade steps: 1000, 1259, 1585, ...
    for i in 20.. {
        let n = 10f64.powf(i as f64 / 10.0).round() as usize;
        if n > MAX_N {
            break;
        }
        let Some(res) = run_benchmarks::<T, F, FullyRandomRangeDistribution>(n, true) else {
            break;
        };
        serde_json::to_writer(&mut *out, &res)?;
        out.write_all(b"\n")?;
        out.flush()?; // keep finished results if a later run crashes
    }
    Ok(())
}

#[derive(Parser)]
struct Args {
    run: String,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    prefer_performance_cores();

    let mut out = BufWriter::new(File::create(format!("runs/{}.jsonl", args.run))?);

    run_family::<u32, OffsetSparseTableFamily>(&mut out)?;
    run_family::<u32, StridedIndexSparseTableFamily<256, 256>>(&mut out)?;

    Ok(())
}
