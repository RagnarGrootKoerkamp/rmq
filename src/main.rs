use std::collections::BTreeMap;
use std::fs::File;
use std::hint::black_box;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{env, io};

use clap::{Parser, Subcommand};
use mem_dbg::{FlatType, MemDbg, MemSize, SizeFlags};
use needletail::parse_fastx_file;
use rmq::base::{RMQ, RMQFamily};
use rmq::bbst::BbSTValue;
use rmq::sparse_table::OffsetSparseTable;
use rmq::*;

use rand::prelude::*;
use rand::rngs::Xoshiro256PlusPlus;
use rand::seq::SliceRandom;
use serde::Serialize;

fn get_rng() -> Xoshiro256PlusPlus {
    Xoshiro256PlusPlus::seed_from_u64(42)
}

trait RMQValue: Copy + Ord + MemDbg + FlatType {
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

#[derive(Clone, Copy, Debug)]
enum RangeDistribution {
    FullyRandom,
    /// r - l < 128
    Small,
    /// r - l in [2^(B-1), 2^B)
    PowerOfTwo(u32),
    LogUniform,
}

impl RangeDistribution {
    fn name(self) -> String {
        match self {
            Self::FullyRandom => "fully_random".to_owned(),
            Self::Small => "small".to_owned(),
            Self::PowerOfTwo(b) => format!("2^{b}"),
            Self::LogUniform => "log_uniform".to_owned(),
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
            Self::LogUniform => {
                // Length is log-uniform in [1, n]: pick a real exponent, then floor 2^e.
                let max_exp = (n as f64).log2();
                let e = rng.random_range(0.0..=max_exp); // inclusive so n == 1 doesn't panic
                let len = (e.exp2() as usize).clamp(1, n);
                // Uniform start among positions where the interval fits.
                let l = rng.random_range(0..=n - len);
                (l, l + len - 1)
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
fn time_queries<'a, T: 'a, R: RMQ<'a, T>>(
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
fn run_benchmarks<T: RMQValue, F: RMQFamily<T>>(
    dist: RangeDistribution,
    n: usize,
    verify: bool,
    data: Option<&[T]>,
    include_build: bool,
) -> Option<BenchmarkResult> {
    let mut rng = get_rng();
    let v = if data.is_none() {
        get_random_permutation(&mut rng, n)
    } else {
        vec![]
    };
    let data = data.unwrap_or(&v);

    let build_time = if include_build {
        Some(time_function(
            |_| F::Rmq::new(black_box(&data)),
            0,
            1,
            1,
            || {},
        ))
    } else {
        None
    };

    let rmq = F::Rmq::new(&data);
    let memory_bytes = rmq.mem_size(SizeFlags::default());
    if env::var("IGNORE_MEM_THRESHOLD").is_err() && memory_bytes > MEMORY_THRESHOLD_BYTES {
        return None;
    }

    let (query_time, query_latency, batch_time) = time_queries(&rmq, dist, n);

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
    let mut last_min = true;
    // Tenth-of-a-decade steps: 1000, 1259, 1585, ...
    for i in 20.. {
        let n = 10f64.powf(i as f64 / 10.0).round() as usize;
        if n > max_n && !last_min {
            break;
        }
        if n < min_n {
            continue;
        } else {
            last_min = false;
        }
        let Some(res) = run_benchmarks::<T, F>(dist, n, true, None, true) else {
            break;
        };
        serde_json::to_writer(&mut *out, &res)?;
        out.write_all(b"\n")?;
        out.flush()?; // keep finished results if a later run crashes
    }
    Ok(())
}

fn run_family_on_genome<T: RMQValue, F: RMQFamily<T>>(
    out: &mut impl Write,
    dist: RangeDistribution,
    genome_lcp: &[T],
) -> std::io::Result<()> {
    eprintln!(
        "run_family_on_genome::<{}, {}>({:?})",
        std::any::type_name::<T>(),
        std::any::type_name::<F>(),
        dist
    );
    let res =
        run_benchmarks::<T, F>(dist, genome_lcp.len(), false, Some(genome_lcp), false).unwrap();
    serde_json::to_writer(&mut *out, &res)?;
    out.write_all(b"\n")?;
    out.flush()?; // keep finished results if a later run crashes

    Ok(())
}

/// `for_each!(B in [8, 16] { run_family::<u32, Fam<B, B>>(...)?; })`
/// runs the body once per value, with `B` bound as a `const usize`.
macro_rules! for_each {
    ($name:ident in [$($v:expr),+ $(,)?] $body:block) => {
        $( { const $name: usize = $v; $body } )+
    };
}

trait RMQVisitor<T: RMQValue> {
    fn visit<F: RMQFamily<T>>(&mut self) -> io::Result<()>;
}

fn visit_all<T: RMQValue>(v: &mut impl RMQVisitor<T>) -> io::Result<()> {
    v.visit::<IndexSparseTableFamily>()?;
    v.visit::<PackedIndexSparseTableFamily>()?;

    v.visit::<OffsetSparseTableFamily>()?;

    v.visit::<RMQTreeFamily>()?;

    for_each!(B in [8, 16, 32, 64] { v.visit::<StridedIndexSparseTableFamily<B>>()?; });
    for_each!(B in [8, 16, 32, 64] { v.visit::<StridedIndexAffixMasksSparseTableFamily<B>>()?; });
    for_each!(B in [8, 16, 32, 64] { v.visit::<BitBlocksSparseTableFamily<B>>()?; });

    v.visit::<SparseTableCartesianBlocksFamily>()?;
    v.visit::<SparseTable3Family>()?;

    Ok(())
}

fn visit_compact<T: RMQValue>(v: &mut impl RMQVisitor<T>) -> io::Result<()> {
    v.visit::<RMQTreeFamily>()?;

    for_each!(B in [8, 16, 32, 64] { v.visit::<StridedIndexSparseTableFamily<B>>()?; });
    for_each!(B in [8, 16, 32, 64] { v.visit::<StridedIndexAffixMasksSparseTableFamily<B>>()?; });
    for_each!(B in [8, 16, 32, 64] { v.visit::<BitBlocksSparseTableFamily<B>>()?; });

    v.visit::<SparseTableCartesianBlocksFamily>()?;
    v.visit::<SparseTable3Family>()?;

    Ok(())
}

fn visit_fast<T: RMQValue + Into<u32>>(v: &mut impl RMQVisitor<T>) -> io::Result<()> {
    v.visit::<IndexSparseTableFamily>()?;
    v.visit::<PackedIndexSparseTableFamily>()?;
    v.visit::<ValueSparseTableFamily>()?;
    v.visit::<FastValueSparseTableFamily>()?;
    v.visit::<FanoutValueSparseTableFamily>()?;
    v.visit::<OffsetSparseTableFamily>()?;

    Ok(())
}

fn visit_bbst_comparison<T: RMQValue + BbSTValue>(v: &mut impl RMQVisitor<T>) -> io::Result<()> {
    v.visit::<BbSTFamily<9>>()?;
    v.visit::<BbST2Family<9, 6>>()?;
    v.visit::<BbST2Family<12, 8>>()?;

    for_each!(B in [64, 512, 4096] { v.visit::<StridedIndexSparseTableFamily<B>>()?; });
    for_each!(B in [32, 64, 512, 4096] { v.visit::<StridedIndexAffixMasksSparseTableFamily<B>>()?; });
    v.visit::<StridedIndexMinHeapSparseTableFamily<32>>()?;

    Ok(())
}

struct SizeSweep<'a, W> {
    out: &'a mut W,
    dist: RangeDistribution,
    min_n: usize,
    max_n: usize,
}

impl<W: Write> RMQVisitor<u32> for SizeSweep<'_, W> {
    fn visit<F: RMQFamily<u32>>(&mut self) -> std::io::Result<()> {
        eprintln!(
            "SizeSweep::<{}, {}>({:?}, n=[{}, {}])",
            std::any::type_name::<u32>(),
            std::any::type_name::<F>(),
            self.dist,
            self.min_n,
            self.max_n
        );
        run_family::<u32, F>(self.out, self.dist, self.min_n, self.max_n)
    }
}

struct FixedRun<'a, W, T> {
    out: &'a mut W,
    dist: RangeDistribution,
    data: &'a [T],
}

impl<W: Write, T: RMQValue> RMQVisitor<T> for FixedRun<'_, W, T> {
    fn visit<F: RMQFamily<T>>(&mut self) -> std::io::Result<()> {
        eprintln!(
            "FixedRun::<{}, {}>({:?}, n={})",
            std::any::type_name::<T>(),
            std::any::type_name::<F>(),
            self.dist,
            self.data.len()
        );
        run_family_on_genome::<T, F>(self.out, self.dist, self.data)
    }
}

struct RangesRun<'a, W> {
    out: &'a mut W,
    n: usize,
}

impl<W: Write> RMQVisitor<u32> for RangesRun<'_, W> {
    fn visit<F: RMQFamily<u32>>(&mut self) -> std::io::Result<()> {
        eprintln!(
            "RangesRun::<{}, {}>(n={})",
            std::any::type_name::<u32>(),
            std::any::type_name::<F>(),
            self.n
        );
        run_ranges_once::<F>(self.out, self.n)
    }
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

/// Creates `<runs dir>/<name>.jsonl`. The runs dir is $BENCH_RUNS if set, otherwise the
/// data repo checked out at `runs/` in this crate.
fn create_run_file(name: &str) -> std::io::Result<BufWriter<File>> {
    let dir = match std::env::var_os("BENCH_RUNS") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("runs"),
    };
    if !dir.is_dir() {
        eprintln!(
            "runs directory {} does not exist. Clone the data repo there with\n    \
             git clone --filter=blob:none https://github.com/lcjconte/rmq-data.git {}\n\
             or set BENCH_RUNS to an existing clone.",
            dir.display(),
            dir.display(),
        );
        std::process::exit(1);
    }
    Ok(BufWriter::new(File::create(
        dir.join(format!("{name}.jsonl")),
    )?))
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
    /// Benchmark every family at n = 1e7 over range sizes 2^1 up to 2^24
    Ranges {
        run: String,
        #[arg(long, default_value = "1e7", value_parser = parse_size)]
        n: usize,
    },
    Genome {
        run: String,
    },
    Rank {
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
    #[cfg(target_os = "macos")]
    prefer_performance_cores();

    match Cli::parse().command {
        Command::Main(args) => run_main(args),
        Command::Ranges { run, n } => run_ranges(&run, n),
        Command::Genome { run } => run_genome(&run),
        Command::Rank { run } => run_rank(&run),
    }
}

fn run_rank(run: &str) -> io::Result<()> {
    let mut out = create_run_file(&format!("{}_random", run))?;

    let n = 10_000_000;
    let mut rng = get_rng();
    let data: Vec<u32> = get_random_permutation(&mut rng, n);

    visit_fast(&mut FixedRun {
        out: &mut out,
        dist: RangeDistribution::FullyRandom,
        data: &data,
    })?;

    Ok(())
}

fn run_main(args: MainArgs) -> std::io::Result<()> {
    // One file per distribution, since the plotting scripts don't distinguish them
    let mut small_out = create_run_file(&format!("{}_small", args.run))?;
    let mut random_out = create_run_file(&format!("{}_random", args.run))?;
    let mut log_uniform_out = create_run_file(&format!("{}_loguniform", args.run))?;

    visit_all(&mut SizeSweep {
        out: &mut small_out,
        dist: RangeDistribution::Small,
        min_n: args.min_n,
        max_n: args.max_n,
    })?;
    visit_all(&mut SizeSweep {
        out: &mut random_out,
        dist: RangeDistribution::FullyRandom,
        min_n: args.min_n,
        max_n: args.max_n,
    })?;
    visit_all(&mut SizeSweep {
        out: &mut log_uniform_out,
        dist: RangeDistribution::LogUniform,
        min_n: args.min_n,
        max_n: args.max_n,
    })?;

    Ok(())
}

/// `BenchmarkResult` without build time, which doesn't depend on the range size.
#[derive(Debug, Serialize)]
struct RangesResult {
    name: &'static str,
    params: BTreeMap<&'static str, usize>,
    distribution: String,
    n: usize,
    memory_bytes: usize,
    query_time: DistStatistics,
    query_latency: DistStatistics,
}

fn run_ranges(run: &str, n: usize) -> std::io::Result<()> {
    let mut out = create_run_file(&format!("{run}_ranges"))?;

    visit_bbst_comparison(&mut RangesRun { out: &mut out, n })?;

    Ok(())
}

fn run_ranges_once<Family: RMQFamily<u32>>(out: &mut impl Write, n: usize) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    prefer_performance_cores();

    // Built once and shared by every range size, since building doesn't depend on the ranges
    let data: Vec<u32> = get_random_permutation(&mut get_rng(), n);
    let rmq = <Family as RMQFamily<u32>>::Rmq::new(&data);
    let memory_bytes = rmq.mem_size(SizeFlags::default());

    let b_max = (n - 1).ilog2() + 1;
    for b in 1..=b_max {
        let dist = RangeDistribution::PowerOfTwo(b);
        let (query_time, query_latency, _) = time_queries(&rmq, dist, n);
        let res = RangesResult {
            name: <Family as RMQFamily<u32>>::NAME,
            params: <Family as RMQFamily<u32>>::PARAMS.iter().copied().collect(),
            distribution: dist.name(),
            n,
            memory_bytes,
            query_time,
            query_latency,
        };
        serde_json::to_writer(&mut *out, &res)?;
        out.write_all(b"\n")?;
        out.flush()?;
    }

    Ok(())
}

fn run_genome(run: &str) -> std::io::Result<()> {
    let filename = "ncbi_dataset/genome.fna";
    let mut reader = parse_fastx_file(&filename).unwrap();

    let mut v: Vec<u8> = Vec::new();
    while let Some(record) = reader.next() {
        let mut seq = record.unwrap().seq().to_vec();
        for b in seq.iter_mut() {
            v.push(b'A' + ((*b >> 1) & 3));
        }
    }

    let lcp = sa_and_lcp(&v);

    // One file per distribution, since the plotting scripts don't distinguish them
    let mut small_out = create_run_file(&format!("{}_small", run))?;
    let mut random_out = create_run_file(&format!("{}_random", run))?;
    let mut log_uniform_out = create_run_file(&format!("{}_loguniform", run))?;

    visit_compact(&mut FixedRun {
        out: &mut small_out,
        dist: RangeDistribution::Small,
        data: &lcp,
    })?;
    visit_compact(&mut FixedRun {
        out: &mut random_out,
        dist: RangeDistribution::FullyRandom,
        data: &lcp,
    })?;
    visit_compact(&mut FixedRun {
        out: &mut log_uniform_out,
        dist: RangeDistribution::LogUniform,
        data: &lcp,
    })?;

    Ok(())
}

pub fn sa_and_lcp(t: &[u8]) -> Vec<u32> {
    eprintln!("building sa..");
    let sa_builder = libsais::SuffixArrayConstruction::for_text(t)
        .in_owned_buffer64()
        .multi_threaded(libsais::ThreadCount::openmp_default())
        .run()
        .unwrap();
    eprintln!(
        "SA:   {:.3} GB",
        std::mem::size_of_val(sa_builder.suffix_array()) as f32 / 1e9
    );

    eprintln!("building plcp..");
    let plcp_builder = sa_builder
        .plcp_construction()
        .multi_threaded(libsais::ThreadCount::openmp_default())
        .run()
        .unwrap();
    eprintln!(
        "PLCP: {:.3} GB",
        std::mem::size_of_val(plcp_builder.plcp()) as f32 / 1e9
    );

    let res_with_plcp_and_lcp = plcp_builder
        .lcp_construction()
        .multi_threaded(libsais::ThreadCount::openmp_default())
        .run()
        .unwrap();

    let (_, lcp, _, r) = res_with_plcp_and_lcp.into_parts();

    lcp.into_iter().map(|x| x.try_into().unwrap()).collect()
}
