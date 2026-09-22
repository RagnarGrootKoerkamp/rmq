The cleanest approach in Rust is: define a trait that captures the RMQ contract, write one generic test suite against that trait, then instantiate it per implementation with a `macro_rules!` so each structure gets its own named test cases.

## 1. Pin down the contract in a trait

```rust
// src/lib.rs
use std::ops::Range;

pub trait Rmq<T: Ord> {
    /// Preprocess `data`. Panics are not allowed for any input.
    fn build(data: &[T]) -> Self
    where
        Self: Sized;

    /// Index of the minimum in `range`. On ties, the *leftmost* index.
    /// Returns `None` iff the range is empty.
    fn argmin(&self, range: Range<usize>) -> Option<usize>;

    fn len(&self) -> usize;
}
```

Two decisions worth making deliberately here, because they're what the shared tests will enforce:

- **Return the index, not the value.** It's a strictly stronger spec, it's what downstream users (LCA, suffix arrays, Cartesian trees) actually need, and it makes tie-breaking observable and therefore testable.
- **Write the tie-break into the trait docs.** "Leftmost on ties" — otherwise your sparse table (which combines overlapping blocks) and your segment tree (which combines disjoint ones) will silently disagree, and no test can tell you which is wrong.

## 2. A naive oracle

```rust
pub struct Naive<'a, T>(&'a [T]);  // or an owning version

impl<T: Ord + Clone> Rmq<T> for NaiveOwned<T> {
    fn build(data: &[T]) -> Self { NaiveOwned(data.to_vec()) }
    fn argmin(&self, r: Range<usize>) -> Option<usize> {
        (r.start..r.end.min(self.0.len()))
            .min_by(|&i, &j| self.0[i].cmp(&self.0[j]))  // min_by keeps the first on ties
            .filter(|_| r.start < r.end)
    }
    fn len(&self) -> usize { self.0.len() }
}
```

This is your reference for differential testing, and it's also a legitimate library export for tiny inputs.

## 3. Generic suite + macro instantiation

Put the suite in the library so both integration tests and benches can reach it:

```rust
// src/testing.rs   —  pub mod testing;  gated by #[cfg(any(test, feature = "testing"))]
use crate::Rmq;

pub fn check_against_naive<R: Rmq<u8>>(data: &[u8]) {
    let s = R::build(data);
    let naive = NaiveOwned::build(data);
    for lo in 0..=data.len() {
        for hi in lo..=data.len() {
            assert_eq!(s.argmin(lo..hi), naive.argmin(lo..hi),
                       "data={data:?} range={lo}..{hi}");
        }
    }
}

/// All arrays of length `n` over alphabet `0..k` — small alphabet ⇒ many ties.
pub fn exhaustive<R: Rmq<u8>>(max_n: usize, k: u8) {
    for n in 0..=max_n {
        let mut data = vec![0u8; n];
        loop {
            check_against_naive::<R>(&data);
            // odometer increment
            let mut i = 0;
            while i < n { data[i] += 1; if data[i] < k { break } data[i] = 0; i += 1 }
            if i == n { break }
        }
    }
}

#[macro_export]
macro_rules! rmq_conformance {
    ($name:ident, $ty:ty) => {
        mod $name {
            use $crate::{Rmq, testing::*};
            #[test] fn empty()        { assert_eq!(<$ty>::build(&[]).argmin(0..0), None); }
            #[test] fn singleton()    { check_against_naive::<$ty>(&[7]); }
            #[test] fn all_equal()    { check_against_naive::<$ty>(&[3; 9]); }
            #[test] fn exhaustive_small() { exhaustive::<$ty>(8, 3); }
            #[test] fn random_large() { /* seeded rng, n ~ 10_000, alphabet ~ 5 */ }
        }
    };
}
```

```rust
// tests/conformance.rs
use rmq::{rmq_conformance, SegmentTree, SparseTable, SqrtDecomp};

rmq_conformance!(segment_tree, SegmentTree<u8>);
rmq_conformance!(sparse_table, SparseTable<u8>);
rmq_conformance!(sqrt_decomp,  SqrtDecomp<u8>);
```

`cargo test` now shows `segment_tree::exhaustive_small`, `sparse_table::exhaustive_small`, etc. — a failure tells you both *which structure* and *which property* broke, rather than dumping you into one giant `#[test] fn it_works`.

## Details that bite

**Small alphabet is the whole game.** Random `u64` data almost never produces ties, so a leftmost-index bug sails through millions of random cases. `exhaustive(8, 3)` — all arrays of length ≤ 8 over `{0,1,2}`, all O(n²) ranges — is a few hundred thousand assertions, runs in well under a second, and catches essentially every off-by-one and tie-break error you can write.

**Each file in `tests/` is its own crate.** Shared helpers there need `tests/common/mod.rs` (note: `mod.rs`, not `common.rs`, or Cargo treats it as a test target). Putting the suite in `src/testing.rs` behind a `testing` feature avoids this entirely and lets your benches use it too.

**Structures with input preconditions need a generator hook.** A ±1 RMQ only accepts arrays where `|a[i+1] - a[i]| == 1`, so it can't consume the same corpus as the others. Either give the trait an associated generator (`type Gen: InputGen`) and have the macro pull test data from it, or keep those impls in a separate suite macro. Don't try to force one corpus over both.

**Benchmark through the same trait.** A generic `fn bench_rmq<R: Rmq<u32>>(c: &mut Criterion)` plus one call per impl gives you comparable build-time/query-time numbers for free, which is usually the point of having several RMQ structures in one crate.

Once the exhaustive suite is green, adding `proptest` over larger sizes is cheap insurance, but it's genuinely the exhaustive small-n pass that does the heavy lifting here.