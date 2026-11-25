use mem_dbg::{FlatType, MemDbg};

use crate::bbst::BbSTValue;
use crate::flrmq::FLRMQValue;
use crate::pixie::PixieValue;
use rand::{Rng, RngExt, SeedableRng, rngs::Xoshiro256PlusPlus, seq::SliceRandom};
use std::fs::File;
use std::io;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use needletail::parse_fastx_file;

/// `BbSTValue` is a supertrait so generic code can always visit the BbST families;
/// a new value type needs C++ BbST bindings in `bbst.rs`.
pub trait RMQValue: Copy + Ord + MemDbg + FlatType + BbSTValue + FLRMQValue + PixieValue {
    /// Truncating cast, like `x as Self`.
    fn from_u64(x: u64) -> Self;
    /// Lossless conversion into u32. Panics if `Self` is wider than u32.
    fn into_u32(self) -> u32;
}
impl RMQValue for u8 {
    fn into_u32(self) -> u32 {
        self.into()
    }
    fn from_u64(x: u64) -> Self {
        x as u8
    }
}
impl RMQValue for u16 {
    fn into_u32(self) -> u32 {
        self.into()
    }
    fn from_u64(x: u64) -> Self {
        x as u16
    }
}
impl RMQValue for u32 {
    fn into_u32(self) -> u32 {
        self
    }
    fn from_u64(x: u64) -> Self {
        x as u32
    }
}
impl RMQValue for u64 {
    fn into_u32(self) -> u32 {
        panic!("u64 does not convert into u32")
    }
    fn from_u64(x: u64) -> Self {
        x
    }
}

/// Where benchmark input comes from.
#[derive(Clone, Debug)]
pub enum Input {
    /// Random permutation of `0..n`.
    Random(usize),
    /// LCP array of a FASTA file's concatenated sequences, cached next to it as a `.u32` file.
    Fasta(PathBuf),
    /// Raw little-endian `u32` array.
    Array(PathBuf),
}

const FASTA_EXTENSIONS: &[&str] = &["fa", "fasta", "fna", "ffn", "frn", "fas"];
const ARRAY_EXTENSION: &str = "u32";

impl Input {
    /// Picks the input kind from the file extension (`.u32`, or a FASTA extension optionally
    /// followed by `.gz`), or random data of size `n` if no file is given.
    pub fn from_args(file: Option<&str>, n: usize) -> io::Result<Self> {
        let Some(file) = file else {
            return Ok(Self::Random(n));
        };
        let path = PathBuf::from(file);
        let ext = |p: &Path| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
        };
        let mut base = path.as_path();
        if ext(base).as_deref() == Some("gz") {
            base = Path::new(base.file_stem().unwrap());
        }
        match ext(base).as_deref() {
            Some(ARRAY_EXTENSION) if base == path => Ok(Self::Array(path)),
            Some(e) if FASTA_EXTENSIONS.contains(&e) => Ok(Self::Fasta(path)),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{file}: unknown input type; expected .{ARRAY_EXTENSION} or one of .{} (optionally .gz)",
                    FASTA_EXTENSIONS.join(", .")
                ),
            )),
        }
    }

    pub fn load(&self) -> io::Result<Vec<u32>> {
        match self {
            Self::Random(n) => Ok(get_random_permutation(&mut get_rng(), *n)),
            Self::Array(path) => read_u32_array(path),
            Self::Fasta(path) => {
                let cache = fasta_cache_path(path);
                if cache.is_file() {
                    eprintln!("using cached LCP array {}", cache.display());
                    return read_u32_array(&cache);
                }
                let lcp = sa_and_lcp(&read_fasta(path)?);
                write_u32_array(&cache, &lcp)?;
                eprintln!("cached LCP array to {}", cache.display());
                Ok(lcp)
            }
        }
    }
}

/// `genome.fna` and `genome.fna.gz` both cache to `genome.u32`.
fn fasta_cache_path(fasta: &Path) -> PathBuf {
    let mut cache = fasta.to_path_buf();
    if cache
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("gz"))
    {
        cache.set_extension("");
    }
    cache.set_extension(ARRAY_EXTENSION);
    cache
}

/// Concatenates all records, mapping each base to one of 4 letters (`(b >> 1) & 3` separates
/// A, C, G, T in both cases).
fn read_fasta(path: &Path) -> io::Result<Vec<u8>> {
    let mut reader = parse_fastx_file(path).map_err(io::Error::other)?;
    let mut v: Vec<u8> = Vec::new();
    while let Some(record) = reader.next() {
        let record = record.map_err(io::Error::other)?;
        v.extend(record.seq().iter().map(|b| b'A' + ((b >> 1) & 3)));
    }
    Ok(v)
}

pub fn read_u32_array(path: &Path) -> io::Result<Vec<u32>> {
    let bytes = std::fs::read(path)?;
    if bytes.len() % 4 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{}: length {} is not a multiple of 4",
                path.display(),
                bytes.len()
            ),
        ));
    }
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect())
}

/// Writes to a temporary file first so an interrupted run never leaves a truncated cache.
pub fn write_u32_array(path: &Path, data: &[u32]) -> io::Result<()> {
    let tmp = path.with_extension(format!("{ARRAY_EXTENSION}.tmp"));
    {
        let mut w = BufWriter::new(File::create(&tmp)?);
        for x in data {
            w.write_all(&x.to_le_bytes())?;
        }
        w.flush()?;
    }
    std::fs::rename(&tmp, path)
}

pub fn get_rng() -> Xoshiro256PlusPlus {
    Xoshiro256PlusPlus::seed_from_u64(42)
}

/// Values wrap around if `n` exceeds `T`'s range, so the result may contain duplicates.
pub fn get_random_permutation<T: RMQValue>(rng: &mut dyn Rng, n: usize) -> Vec<T> {
    let mut v: Vec<T> = (0..n as u64).map(T::from_u64).collect();
    v.shuffle(rng);
    v
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

    let (_, lcp, _, _r) = res_with_plcp_and_lcp.into_parts();

    lcp.into_iter().map(|x| x.try_into().unwrap()).collect()
}

#[derive(Clone, Copy, Debug)]
pub enum RangeDistribution {
    FullyRandom,
    /// r - l < 128
    Small,
    /// r - l in [2^(B-1), 2^B)
    PowerOfTwo(u32),
    LogUniform,
}

impl RangeDistribution {
    pub fn name(self) -> String {
        match self {
            Self::FullyRandom => "fully_random".to_owned(),
            Self::Small => "small".to_owned(),
            Self::PowerOfTwo(b) => format!("2^{b}"),
            Self::LogUniform => "log_uniform".to_owned(),
        }
    }

    pub fn sample_range(self, rng: &mut dyn Rng, n: usize) -> (usize, usize) {
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
                assert!(
                    (1..usize::BITS).contains(&b),
                    "range size 2^{b} out of bounds"
                );
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
