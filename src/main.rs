use std::fs::File;
use std::hint::black_box;
use std::io::{BufWriter, Write};
use std::time::Instant;

use rmq::base::{RMQFamily, RMQ};
use rmq::sparse_table::{IndexSparseTableFamily, OffsetSparseTableFamily};

use rand::prelude::*;
use rand::rngs::Xoshiro256PlusPlus;
use rand::seq::SliceRandom;
use rmq::sparse_table2::{SparseTableOnBlocksFamily};
use serde::Serialize;

fn get_rng() -> Xoshiro256PlusPlus {
    Xoshiro256PlusPlus::seed_from_u64(42)
}

fn get_random_permutation(rng: &mut dyn Rng, n: usize) -> Vec<u64> {
    let mut v: Vec<u64> = (0..n as u64).collect();
    v.shuffle(rng);
    v
}

fn sample_range_biased(rng: &mut dyn Rng, n: usize) -> (usize, usize) {
    let l = rng.random_range(0..n);
    let r = rng.random_range(l..n);
    (l, r)
}

fn sample_range(rng: &mut dyn Rng, n: usize) -> (usize, usize) {
    let mut l = rng.random_range(0..n);
    let mut r = rng.random_range(0..n);
    while l > r {
        l = rng.random_range(0..n);
        r = rng.random_range(0..n);
    }
    (l, r)
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
    query_time: DistStatistics
}


fn time_function<T: Copy, R, F, G>(mut f: F, repetitions: usize, batch_size: usize, mut input_generator: G) -> DistStatistics 
where
    F: FnMut(T) -> R,
    G: FnMut() -> T
{
    let start = Instant::now();
    // BUG: F is called non-determinstic times
    while start.elapsed().as_millis() < 100 {
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
    
    let mean = durations.iter().sum::<f64>()/ durations.len() as f64;
    let std_dev = if durations.len() > 1 { (durations.iter().map(|x| (x-mean).powi(2)).sum::<f64>() / (durations.len() as f64 -1f64)).sqrt() } else {0f64};
    durations.sort_by(f64::total_cmp);
    let median = durations[durations.len() / 2];
    DistStatistics { mean, std_dev, median }
}

fn run_benchmarks<F: RMQFamily<u64>>(n: usize) -> BenchmarkResult {
    let mut rng = get_rng();
    let data = get_random_permutation(&mut rng, n);

    let build_time = time_function(|_| {
        F::Rmq::new(black_box(&data))
    }, 5, 1, || {});

    let rmq = F::Rmq::new(&data);

    let query_time = time_function(|(l, r)| {
        black_box(rmq.rmq(black_box(l), black_box(r)));
    }, 100_000, 1000, || {sample_range(&mut rng, n)});

    BenchmarkResult { name: std::any::type_name::<F>().to_string(), n, build_time, query_time }
}


#[cfg(target_os = "macos")]
fn prefer_performance_cores() {
    unsafe extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
    }
    const QOS_CLASS_USER_INTERACTIVE: u32 = 0x21;
    unsafe { pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0) };
}

fn main() -> std::io::Result<()> {
    prefer_performance_cores();

    let mut out = BufWriter::new(File::create("results.jsonl")?);

    // Tenth-of-a-decade steps: 1000, 1259, 1585, ..., 10_000_000
    for i in 30..=70 {
        let n = 10f64.powf(i as f64 / 10.0).round() as usize;
        for res in [
            run_benchmarks::<OffsetSparseTableFamily>(n),
            run_benchmarks::<IndexSparseTableFamily>(n)
        ] {
            serde_json::to_writer(&mut out, &res)?;
            out.write_all(b"\n")?;
        }        
        out.flush()?; // keep finished results if a later run crashes
    }
    Ok(())
}
