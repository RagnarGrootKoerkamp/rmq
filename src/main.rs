use std::collections::BTreeMap;
use std::fs::File;
use std::hint::black_box;
use std::io::{BufWriter, Write};
use std::time::Instant;

use clap::{Parser, Subcommand};
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
    const NAME: &'static str;
    fn sample_range(rng: &mut dyn Rng, n: usize) -> (usize, usize);
}

struct FullyRandomRangeDistribution;
impl RangeDistribution for FullyRandomRangeDistribution {
    const NAME: &'static str = "fully_random";
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
    const NAME: &'static str = "small";
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
    name: &'static str,
    params: BTreeMap<&'static str, usize>,
    distribution: &'static str,
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
        if !verify_function(
            |(l, r)| rmq.rmq(l, r),
            |(l, r)| offset_table.rmq(l, r),
            warmup_repetitions + repetitions,
            || R::sample_range(&mut query_rng, n),
        ) {
            panic!("Verify failed: {} {:?} n={n} ({})", F::NAME, F::PARAMS, R::NAME)
        }
    }

    Some(BenchmarkResult {
        name: F::NAME,
        params: F::PARAMS.iter().copied().collect(),
        distribution: R::NAME,
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

/// Only sizes in [min_n, max_n] are benchmarked; set min_n = max_n to run a single size.
fn run_family<T: RMQValue, F: RMQFamily<T>, R: RangeDistribution>(
    out: &mut impl Write,
    min_n: usize,
    max_n: usize,
) -> std::io::Result<()> {
    // Tenth-of-a-decade steps: 1000, 1259, 1585, ...
    for i in 20.. {
        let n = 10f64.powf(i as f64 / 10.0).round() as usize;
        if n > max_n {
            break;
        }
        if n < min_n {
            continue;
        }
        let Some(res) = run_benchmarks::<T, F, R>(n, true) else {
            break;
        };
        serde_json::to_writer(&mut *out, &res)?;
        out.write_all(b"\n")?;
        out.flush()?; // keep finished results if a later run crashes
    }
    Ok(())
}

/// Runs `run_family` once per listed `B`, substituting it for each `B` in the family's parameters:
/// `sweep_family!(&mut out, min_n, max_n, u32, Dist, Family<B, B> for B in [8, 16]);`
macro_rules! sweep_family {
    ($out:expr, $min_n:expr, $max_n:expr, $t:ty, $dist:ty, $family:ident<B, B> for B in [$($b:literal),+ $(,)?] $(,)?) => {
        $( run_family::<$t, $family<$b, $b>, $dist>($out, $min_n, $max_n)?; )+
    };
    ($out:expr, $min_n:expr, $max_n:expr, $t:ty, $dist:ty, $family:ident<B> for B in [$($b:literal),+ $(,)?] $(,)?) => {
        $( run_family::<$t, $family<$b>, $dist>($out, $min_n, $max_n)?; )+
    };
}

fn run_all<R: RangeDistribution>(
    out: &mut impl Write,
    min_n: usize,
    max_n: usize,
) -> std::io::Result<()> {
    // Baselines
    run_family::<u32, IndexSparseTableFamily, R>(out, min_n, max_n)?;
    run_family::<u32, OffsetSparseTableFamily, R>(out, min_n, max_n)?;
    run_family::<u32, RMQTreeFamily, R>(out, min_n, max_n)?;

    // Block structures, compared at equal block sizes
    sweep_family!(out, min_n, max_n, u32, R, StridedIndexSparseTableFamily<B, B> for B in [8, 16, 32, 64]);
    sweep_family!(out, min_n, max_n, u32, R, StridedIndexAffixMasksSparseTableFamily<B, B> for B in [8, 16, 32, 64]);
    sweep_family!(out, min_n, max_n, u32, R, BitBlocksSparseTableFamily<B> for B in [8, 16, 32, 64]);

    // Block size chosen internally from n
    run_family::<u32, SparseTableCartesianBlocksFamily, R>(out, min_n, max_n)?;
    run_family::<u32, SparseTable3Family, R>(out, min_n, max_n)?;

    Ok(())
}

/// Accepts plain integers or scientific notation, e.g. `1e7`.
fn parse_size(s: &str) -> Result<usize, String> {
    if let Ok(n) = s.parse::<usize>() {
        return Ok(n);
    }
    match s.parse::<f64>() {
        Ok(x) if x >= 0.0 && x.fract() == 0.0 => Ok(x as usize),
        _ => Err(format!("invalid size: {s}")),
    }
}

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the benchmarks
    Main(MainArgs),
    /// Print hello world
    Hello,
}

#[derive(clap::Args)]
struct MainArgs {
    run: String,
    /// Smallest n to benchmark
    #[arg(long, default_value = "1000", value_parser = parse_size)]
    min_n: usize,
    /// Largest n to benchmark; set equal to --min-n to run a single size
    #[arg(long, default_value = "1e7", value_parser = parse_size)]
    max_n: usize,
}

fn main() -> std::io::Result<()> {
    match Cli::parse().command {
        Command::Main(args) => run_main(args),
        Command::Hello => {
            println!("Hello, world!");
            Ok(())
        }
    }
}

fn run_main(args: MainArgs) -> std::io::Result<()> {
    prefer_performance_cores();

    // One file per distribution, since the plotting scripts don't distinguish them
    let mut small_out = BufWriter::new(File::create(format!("runs/{}_small.jsonl", args.run))?);
    let mut random_out = BufWriter::new(File::create(format!("runs/{}_random.jsonl", args.run))?);

    run_all::<SmallRangesDistribution>(&mut small_out, args.min_n, args.max_n)?;
    run_all::<FullyRandomRangeDistribution>(&mut random_out, args.min_n, args.max_n)?;

    Ok(())
}
