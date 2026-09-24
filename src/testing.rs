use crate::{
    base::{RMQ, RMQFamily},
    naive::NaiveRMQ,
};

use rand::prelude::*;
use rand::rngs::Xoshiro256PlusPlus;
use rand::seq::SliceRandom;

fn get_rng() -> Xoshiro256PlusPlus {
    Xoshiro256PlusPlus::seed_from_u64(42)
}

fn get_random_permutation(rng: &mut dyn Rng, n: u64) -> Vec<u64> {
    let mut v: Vec<u64> = (0..n).collect();
    v.shuffle(rng);
    v
}

// Drawn with replacement from 0..distinct, so a small `distinct` forces ties.
fn get_random_with_replace(rng: &mut dyn Rng, n: u64, distinct: u64) -> Vec<u64> {
    (0..n).map(|_| rng.random_range(0..distinct)).collect()
}

// Compares <= 100 ranges. (If more than 100 possible, these are randomly sampled)
pub fn compare_with_naive<F: RMQFamily<u64>>(data: &[u64]) {
    let naive = NaiveRMQ::new(data);
    let test_object = F::Rmq::new(data);
    let n = data.len();
    let check = |l: usize, r: usize| {
        assert_eq!(
            test_object.rmq(l, r),
            naive.rmq(l, r),
            "n = {n}: rmq({l}, {r})"
        );
    };
    if n * (n + 1) / 2 <= 100 {
        for l in 0..n {
            for r in l..n {
                check(l, r);
            }
        }
    } else {
        let mut rng = get_rng();
        for _ in 0..100 {
            // This is not uniformly random
            let l = rng.random_range(0..n);
            let r = rng.random_range(l..n);
            check(l, r);
        }
    }
}

// Every range of every size up to the exhaustive limit in compare_with_naive.
pub fn test_small_sizes<F: RMQFamily<u64>>() {
    let mut rng = get_rng();
    for n in 1..=13 {
        compare_with_naive::<F>(&get_random_permutation(&mut rng, n));
    }
}

pub fn test_random_permutations<F: RMQFamily<u64>>() {
    let repeats = 10;
    let mut rng = get_rng();
    for _ in 0..repeats {
        let data = get_random_permutation(&mut rng, 100);
        compare_with_naive::<F>(&data);
    }
}

pub fn test_random_with_ties<F: RMQFamily<u64>>() {
    let repeats = 10;
    let mut rng = get_rng();
    for _ in 0..repeats {
        let data = get_random_with_replace(&mut rng, 100, 5);
        compare_with_naive::<F>(&data);
    }
}

pub fn test_edge_ranges<F: RMQFamily<u64>>() {
    let data = get_random_permutation(&mut get_rng(), 100);
    let naive = NaiveRMQ::new(&data);
    let test_object = F::Rmq::new(&data);
    let n = data.len();
    for (l, r) in [
        (0, n - 1),
        (0, 0),
        (n - 1, n - 1),
        (n / 2, n - 1),
        (0, n / 2),
    ] {
        assert_eq!(test_object.rmq(l, r), naive.rmq(l, r), "range [{l}, {r}]");
    }
}

// Declares one `#[test]` per shared check, in a module named `$name`, for the
// implementation named by the family `$family`.
#[macro_export]
macro_rules! rmq_conformance {
    ($name:ident, $family:ty $(,)?) => {
        mod $name {
            #[allow(unused_imports)]
            use super::*;

            #[test]
            fn small_sizes() {
                $crate::testing::test_small_sizes::<$family>();
            }

            #[test]
            fn random_permutations() {
                $crate::testing::test_random_permutations::<$family>();
            }

            #[test]
            fn random_with_ties() {
                $crate::testing::test_random_with_ties::<$family>();
            }

            #[test]
            fn edge_ranges() {
                $crate::testing::test_edge_ranges::<$family>();
            }
        }
    };
}
