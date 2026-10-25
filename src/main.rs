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

#[derive(Clone, Copy)]
enum RangeDistribution {
    FullyRandom,
    /// r - l < 128
    Small,
    /// r - l in [2^(B-1), 2^B)
    PowerOfTwo(u32),
}

impl RangeDistribution {
    fn name(self) -> String {
        match self {
            Self::FullyRandom => "fully_random".to_owned(),
            Self::Small => "small".to_owned(),
            Self::PowerOfTwo(b) => format!("2^{b}"),
        }
    }

    fn sample_range(self, rng: &mut dyn Rng, n: usize) -> (usize, usize) {
        match self {
            Self::FullyRandom => {
                let mut l = rng.random_range(0..n);
                let mut r = rng.random_range(0..n);
                while l > r {
                    l = rng.random_range(0..n);
                    r = rng.random_range(0..n);
                }
                (l, r)
            }
            Self::Small => {
                let mut l = rng.random_range(0..n);
                let mut r = rng.random_range(l..l + 128);
                while r >= n {
                    l = rng.random_range(0..n);
                    r = rng.random_range(l..l + 128);
                }
                (l, r)
            }
            Self::PowerOfTwo(b) => {
                assert!(b >= 1 && b < usize::BITS, "range size 2^{b} out of bounds");
                let lo = 1usize << (b - 1);
                let hi = 1usize << b;
                assert!(n > lo, "n={n} too small for range size 2^{b}");
                loop {
                    // l >= n - lo can never give a valid r, so don't sample it
                    let l = rng.random_range(0..n - lo);
                    let r = rng.random_range(l + lo..l + hi);
                    if r < n {
                        return (l, r);
                    }
                }
            }
        }
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
    distribution: String,
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

const QUERY_WARMUP_REPETITIONS: usize = 100_000;
const QUERY_REPETITIONS: usize = 1_000_000;

/// Returns (query time, query latency) over ranges drawn from `dist`.
fn time_queries<'a, T: 'a, R: RMQ<'a, T>>(
    rmq: &R,
    dist: RangeDistribution,
    n: usize,
) -> (DistStatistics, DistStatistics) {
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
            prev
        },
        QUERY_WARMUP_REPETITIONS,
        QUERY_REPETITIONS,
        1000,
        || dist.sample_range(&mut query_rng, n),
    );

    (query_time, query_latency)
}

/// Each family is benchmarked until its structure would exceed this many bytes.
const MEMORY_THRESHOLD_BYTES: usize = 1 << 32;

/// Returns `None` if the structure built over `n` elements exceeds the memory threshold.
fn run_benchmarks<T: RMQValue, F: RMQFamily<T>>(
    dist: RangeDistribution,
    n: usize,
    verify: bool,
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

    let (query_time, query_latency) = time_queries(&rmq, dist, n);

    if verify {
        // Verify the benchmark outputs by rerunning them and comparing against OffsetSparsetable
        let offset_table = OffsetSparseTable::new(&data);
        let mut query_rng = get_rng();
        if !verify_function(
            |(l, r)| rmq.rmq(l, r),
            |(l, r)| offset_table.rmq(l, r),
            QUERY_WARMUP_REPETITIONS + QUERY_REPETITIONS,
            || dist.sample_range(&mut query_rng, n),
        ) {
            panic!("Verify failed: {} {:?} n={n} ({})", F::NAME, F::PARAMS, dist.name())
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
fn run_family<T: RMQValue, F: RMQFamily<T>>(
    out: &mut impl Write,
    dist: RangeDistribution,
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
        let Some(res) = run_benchmarks::<T, F>(dist, n, true) else {
            break;
        };
        serde_json::to_writer(&mut *out, &res)?;
        out.write_all(b"\n")?;
        out.flush()?; // keep finished results if a later run crashes
    }
    Ok(())
}

/// Runs `run_family` once per listed `B`, substituting it for each `B` in the family's parameters:
/// `sweep_family!(&mut out, dist, min_n, max_n, u32, Family<B, B> for B in [8, 16]);`
macro_rules! sweep_family {
    ($out:expr, $dist:expr, $min_n:expr, $max_n:expr, $t:ty, $family:ident<B, B> for B in [$($b:literal),+ $(,)?] $(,)?) => {
        $( run_family::<$t, $family<$b, $b>>($out, $dist, $min_n, $max_n)?; )+
    };
    ($out:expr, $dist:expr, $min_n:expr, $max_n:expr, $t:ty, $family:ident<B> for B in [$($b:literal),+ $(,)?] $(,)?) => {
        $( run_family::<$t, $family<$b>>($out, $dist, $min_n, $max_n)?; )+
    };
}

fn run_all(
    out: &mut impl Write,
    dist: RangeDistribution,
    min_n: usize,
    max_n: usize,
) -> std::io::Result<()> {
    // Baselines
    run_family::<u32, IndexSparseTableFamily>(out, dist, min_n, max_n)?;
    run_family::<u32, OffsetSparseTableFamily>(out, dist, min_n, max_n)?;
    run_family::<u32, RMQTreeFamily>(out, dist, min_n, max_n)?;

    // Block structures, compared at equal block sizes
    sweep_family!(out, dist, min_n, max_n, u32, StridedIndexSparseTableFamily<B, B> for B in [8, 16, 32, 64]);
    sweep_family!(out, dist, min_n, max_n, u32, StridedIndexAffixMasksSparseTableFamily<B, B> for B in [8, 16, 32, 64]);
    sweep_family!(out, dist, min_n, max_n, u32, BitBlocksSparseTableFamily<B> for B in [8, 16, 32, 64]);

    // Block size chosen internally from n
    run_family::<u32, SparseTableCartesianBlocksFamily>(out, dist, min_n, max_n)?;
    run_family::<u32, SparseTable3Family>(out, dist, min_n, max_n)?;

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
    /// Benchmark OffsetSparseTable at n = 1e7 over range sizes 2^1 up to 2^24
    Ranges {
        run: String,
    },
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
        Command::Ranges { run } => run_ranges(&run),
    }
}

fn run_main(args: MainArgs) -> std::io::Result<()> {
    prefer_performance_cores();

    // One file per distribution, since the plotting scripts don't distinguish them
    let mut small_out = BufWriter::new(File::create(format!("runs/{}_small.jsonl", args.run))?);
    let mut random_out = BufWriter::new(File::create(format!("runs/{}_random.jsonl", args.run))?);

    run_all(&mut small_out, RangeDistribution::Small, args.min_n, args.max_n)?;
    run_all(&mut random_out, RangeDistribution::FullyRandom, args.min_n, args.max_n)?;

    Ok(())
}

/// `BenchmarkResult` without build time and memory, which don't depend on the range size.
#[derive(Debug, Serialize)]
struct RangesResult {
    name: &'static str,
    params: BTreeMap<&'static str, usize>,
    distribution: String,
    n: usize,
    query_time: DistStatistics,
    query_latency: DistStatistics,
}

fn run_ranges(run: &str) -> std::io::Result<()> { 
    let mut out = BufWriter::new(File::create(format!("runs/{run}_ranges.jsonl"))?);


    run_ranges_once::<OffsetSparseTableFamily>(&mut out)?;
    run_ranges_once::<StridedIndexSparseTableFamily<16, 64>>(&mut out)?;
    run_ranges_once::<StridedIndexSparseTableFamily<32, 64>>(&mut out)?;
    run_ranges_once::<StridedIndexSparseTableFamily<64, 64>>(&mut out)?;
    run_ranges_once::<StridedIndexSparseTableFamily<128, 128>>(&mut out)?;

    Ok(())
}

fn run_ranges_once<Family: RMQFamily<u32>>(out: &mut BufWriter<File>) -> std::io::Result<()> {
    prefer_performance_cores();

    let n = 10_000_000;

    // Built once and shared by every range size, since building doesn't depend on the ranges
    let data: Vec<u32> = get_random_permutation(&mut get_rng(), n);
    let rmq = <Family as RMQFamily<u32>>::Rmq::new(&data);

    // 2^24 is the largest B with 2^(B-1) < n. No verification: the reference is the
    // benchmarked structure itself.
    for b in 1..=24 {
        let dist = RangeDistribution::PowerOfTwo(b);
        let (query_time, query_latency) = time_queries(&rmq, dist, n);
        let res = RangesResult {
            name: <Family as RMQFamily<u32>>::NAME,
            params: <Family as RMQFamily<u32>>::PARAMS.iter().copied().collect(),
            distribution: dist.name(),
            n,
            query_time,
            query_latency,
        };
        serde_json::to_writer(&mut *out, &res)?;
        out.write_all(b"\n")?;
        out.flush()?;
    }

    Ok(())
}
