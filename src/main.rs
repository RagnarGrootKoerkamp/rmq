use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::{env, io};

use clap::{Parser, Subcommand};
use mem_dbg::{MemSize, SizeFlags};
use rmq::base::{RMQ, RMQFamily};
use rmq::benchmarking::{DistStatistics, run_benchmarks, run_dfs_benchmark, time_queries};
use rmq::data::{Input, RMQValue, RangeDistribution, get_random_permutation, get_rng};
use rmq::*;

use serde::Serialize;

#[cfg(target_os = "macos")]
fn prefer_performance_cores() {
    unsafe extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
    }
    const QOS_CLASS_USER_INTERACTIVE: u32 = 0x21;
    unsafe { pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0) };
}

/// Only sizes in [min_n, max_n] are benchmarked
fn run_family_sweepn<T: RMQValue, F: RMQFamily<T>>(
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
        let Some(res) = run_benchmarks::<T, F>(
            dist,
            n,
            true,
            &get_random_permutation(&mut get_rng(), n),
            true,
        ) else {
            break;
        };
        serde_json::to_writer(&mut *out, &res)?;
        out.write_all(b"\n")?;
        out.flush()?; // keep finished results if a later run crashes
    }
    Ok(())
}

fn run_family_fixed<T: RMQValue, F: RMQFamily<T>>(
    out: &mut impl Write,
    dist: RangeDistribution,
    genome_lcp: &[T],
) -> std::io::Result<()> {
    eprintln!(
        "run_family_fixed::<{}, {}>({:?})",
        std::any::type_name::<T>(),
        std::any::type_name::<F>(),
        dist
    );
    let res = run_benchmarks::<T, F>(dist, genome_lcp.len(), false, genome_lcp, false).unwrap();
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

#[derive(Clone, Copy, clap::ValueEnum)]
enum VisitKey {
    All,
    Compact,
    Fast,
    External
}

fn visit_by_key<T: RMQValue>(v: &mut impl RMQVisitor<T>, key: VisitKey) -> io::Result<()> {
    match key {
        VisitKey::All => {
            v.visit::<IndexSparseTableFamily>()?;
            v.visit::<PackedIndexSparseTableFamily>()?;

            v.visit::<OffsetSparseTableFamily>()?;

            v.visit::<RMQTreeFamily>()?;

            for_each!(B in [8, 16, 32, 64] { v.visit::<StridedIndexSparseTableFamily<B>>()?; });
            for_each!(B in [8, 16, 32, 64] { v.visit::<StridedIndexAffixMasksSparseTableFamily<B>>()?; });
            for_each!(B in [8, 16, 32, 64] { v.visit::<BitBlocksSparseTableFamily<B>>()?; });

            v.visit::<SparseTableCartesianBlocksFamily>()?;
            v.visit::<SparseTable3Family>()?;
        }
        VisitKey::Compact => {
            v.visit::<RMQTreeFamily>()?;

            for_each!(B in [8, 16, 32, 64] { v.visit::<StridedIndexSparseTableFamily<B>>()?; });
            for_each!(B in [8, 16, 32, 64] { v.visit::<StridedIndexAffixMasksSparseTableFamily<B>>()?; });
            for_each!(B in [8, 16, 32, 64] { v.visit::<BitBlocksSparseTableFamily<B>>()?; });

            v.visit::<SparseTableCartesianBlocksFamily>()?;
            v.visit::<SparseTable3Family>()?;
        }
        VisitKey::Fast => {
            v.visit::<IndexSparseTableFamily>()?;
            v.visit::<PackedIndexSparseTableFamily>()?;
            v.visit::<ValueSparseTableFamily>()?;
            v.visit::<FastValueSparseTableFamily>()?;
            v.visit::<FanoutValueSparseTableFamily>()?;
            v.visit::<OffsetSparseTableFamily>()?;
        }
        VisitKey::External => {
            v.visit::<BbSTFamily<6>>()?;
            v.visit::<BbSTFamily<9>>()?;
            v.visit::<BbST2Family<9, 6>>()?;
            v.visit::<BbST2Family<12, 8>>()?;

            for_each!(B in [64, 512, 4096] { v.visit::<StridedIndexSparseTableFamily<B>>()?; });
            for_each!(B in [32, 64, 512, 4096] { v.visit::<StridedIndexAffixMasksSparseTableFamily<B>>()?; });
            v.visit::<StridedIndexMinHeapSparseTableFamily<32>>()?;

            for_each!(E in [64, 128, 256, 512, 1024, 2048] { v.visit::<FLRMQFamily<E>>()?; });

            for_each!(L in [248, 252, 496] { v.visit::<HybridBTreeFamily<L>>()?; });
        }
    }

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
        run_family_sweepn::<u32, F>(self.out, self.dist, self.min_n, self.max_n)
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
        run_family_fixed::<T, F>(self.out, self.dist, self.data)
    }
}

struct RangesRun<'a, W> {
    out: &'a mut W,
    data: &'a [u32],
}

impl<W: Write> RMQVisitor<u32> for RangesRun<'_, W> {
    fn visit<F: RMQFamily<u32>>(&mut self) -> std::io::Result<()> {
        eprintln!(
            "RangesRun::<{}, {}>(n={})",
            std::any::type_name::<u32>(),
            std::any::type_name::<F>(),
            self.data.len()
        );
        run_ranges_once::<F>(self.out, self.data)
    }
}

struct DFSRun<'a, W, T> {
    out: &'a mut W,
    data: &'a [T],
}

impl<W: Write, T: RMQValue> RMQVisitor<T> for DFSRun<'_, W, T> {
    fn visit<F: RMQFamily<T>>(&mut self) -> std::io::Result<()> {
        eprintln!(
            "DFSRun::<{}, {}>(n={})",
            std::any::type_name::<T>(),
            std::any::type_name::<F>(),
            self.data.len()
        );
        let res = run_dfs_benchmark::<T, F>(self.data.len(), false, self.data, false).unwrap();

        serde_json::to_writer(&mut *self.out, &res)?;
        self.out.write_all(b"\n")?;
        self.out.flush()?; // keep finished results if a later run crashes

        Ok(())
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
    SweepN {
        run: String,
        /// Smallest n to benchmark
        #[arg(long, default_value = "1000", value_parser = parse_size)]
        min_n: usize,
        /// Largest n to benchmark; set equal to --min-n to run a single size
        #[arg(long, default_value = "1e7", value_parser = parse_size)]
        max_n: usize,
        #[arg(long)]
        visit: VisitKey,
    },
    /// Benchmark the selected families over range sizes 2^1 up to 2^ceil(log2 n)
    Ranges {
        run: String,
        /// Size of the random permutation; ignored when an input file is given
        #[arg(long, default_value = "1e7", value_parser = parse_size)]
        n: usize,
        /// Input instead of a random permutation: a .u32 array, or a FASTA file (.fa, .fna, ...,
        /// optionally .gz) whose LCP array is used and cached as a .u32 file next to it
        input_file: Option<String>,
        #[arg(long)]
        visit: VisitKey,
    },
    /// Benchmark the selected families on one input under each range distribution
    FixedInput {
        run: String,
        /// Size of the random permutation; ignored when an input file is given
        #[arg(long, default_value = "1e7", value_parser = parse_size)]
        n: usize,
        /// Input instead of a random permutation: a .u32 array, or a FASTA file (.fa, .fna, ...,
        /// optionally .gz) whose LCP array is used and cached as a .u32 file next to it
        input_file: Option<String>,
        #[arg(long)]
        visit: VisitKey,
    },
    Rank {
        run: String,
    },
    DFS {
        run: String,
        /// Size of the random permutation; ignored when an input file is given
        #[arg(long, default_value = "1e7", value_parser = parse_size)]
        n: usize,
        /// Input instead of a random permutation: a .u32 array, or a FASTA file (.fa, .fna, ...,
        /// optionally .gz) whose LCP array is used and cached as a .u32 file next to it
        input_file: Option<String>,
        #[arg(long)]
        visit: VisitKey,
    },
}

fn main() -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    prefer_performance_cores();

    match Cli::parse().command {
        Command::SweepN {
            run,
            min_n,
            max_n,
            visit,
        } => run_main(&run, min_n, max_n, visit),
        Command::Ranges {
            run,
            n,
            input_file,
            visit,
        } => run_ranges(
            &run,
            &Input::from_args(input_file.as_deref(), n)?.load()?,
            visit,
        ),
        Command::FixedInput {
            run,
            n,
            input_file,
            visit,
        } => run_fixed_input(
            &run,
            &Input::from_args(input_file.as_deref(), n)?.load()?,
            visit,
        ),
        Command::Rank { run } => run_rank(&run),
        Command::DFS {
            run,
            n,
            input_file,
            visit,
        } => run_dfs(
            &run,
            &Input::from_args(input_file.as_deref(), n)?.load()?,
            visit,
        ),
    }
}

fn run_dfs(run: &str, data: &[u32], visit: VisitKey) -> io::Result<()> {
    let mut out = create_run_file(&format!("{}_dfs", run))?;

    visit_by_key(
        &mut DFSRun {
            out: &mut out,
            data,
        },
        visit,
    )?;

    Ok(())
}

fn run_rank(run: &str) -> io::Result<()> {
    let mut out = create_run_file(&format!("{}_random", run))?;

    let n = 10_000_000;
    let mut rng = get_rng();
    let data: Vec<u32> = get_random_permutation(&mut rng, n);

    visit_by_key(
        &mut FixedRun {
            out: &mut out,
            dist: RangeDistribution::FullyRandom,
            data: &data,
        },
        VisitKey::Fast,
    )?;

    Ok(())
}

fn run_main(run: &str, min_n: usize, max_n: usize, visit: VisitKey) -> std::io::Result<()> {
    for (dist_name, dist) in [
        ("small", RangeDistribution::Small),
        ("random", RangeDistribution::FullyRandom),
        ("loguniform", RangeDistribution::LogUniform),
    ] {
        let mut out = create_run_file(&format!("{}_{}", run, dist_name))?;
        visit_by_key(
            &mut SizeSweep {
                out: &mut out,
                dist,
                min_n,
                max_n,
            },
            visit,
        )?;
    }

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

fn run_ranges(run: &str, data: &[u32], visit: VisitKey) -> std::io::Result<()> {
    let mut out = create_run_file(&format!("{run}_ranges"))?;

    visit_by_key(
        &mut RangesRun {
            out: &mut out,
            data,
        },
        visit,
    )?;

    Ok(())
}

fn run_ranges_once<Family: RMQFamily<u32>>(
    out: &mut impl Write,
    data: &[u32],
) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    prefer_performance_cores();

    let n = data.len();
    // Built once and shared by every range size, since building doesn't depend on the ranges
    let rmq = <Family as RMQFamily<u32>>::Rmq::new(data);
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

fn run_fixed_input(run: &str, data: &[u32], visit: VisitKey) -> std::io::Result<()> {
    // One file per distribution, since the plotting scripts don't distinguish them
    for (dist_name, dist) in [
        ("small", RangeDistribution::Small),
        ("random", RangeDistribution::FullyRandom),
        ("loguniform", RangeDistribution::LogUniform),
    ] {
        let mut out = create_run_file(&format!("{}_{}", run, dist_name))?;
        visit_by_key(
            &mut FixedRun {
                out: &mut out,
                dist,
                data,
            },
            visit,
        )?;
    }

    Ok(())
}
