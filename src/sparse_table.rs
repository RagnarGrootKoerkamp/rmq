use crate::base::RMQ;

pub struct SparseTable<'a> {
    n: usize,
    k_max: usize,
    data: &'a [u64],
    // Offsets can be packed more tightly
    offsets: Vec<usize>,
}

impl<'a> RMQ<'a, u64> for SparseTable<'a> {
    fn new(data: &'a [u64]) -> Self {
        let n = data.len() as usize;
        let k_max = n.ilog2() as usize;
        let mut table = Self {
            n,
            k_max,
            data,
            offsets: vec![0; ((k_max) * n) as usize],
        };
        for i in 0..table.n - 1 {
            table.offsets[i] = if table.data[i] <= table.data[i + 1] {
                0
            } else {
                1
            }
        }
        for k in 2..=table.k_max {
            for i in 0..table.n + 1 - (1 << k) {
                let loffset = table.offsets[(k - 2) * table.n + i];
                let roffset = table.offsets[(k - 2) * table.n + i + (1 << k - 1)] + (1 << k - 1);
                table.offsets[((k-1) * table.n + i)] =
                    if (table.data[i + loffset] <= table.data[i + roffset]) {
                        loffset
                    } else {
                        roffset
                    };
            }
        }
        table
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        if l == r {
            return l;
        }

        let k = (r - l + 1).ilog2() as usize;
        let left_index = self.offsets[(k - 1) * self.n + l] + l;
        let right_index = self.offsets[(k - 1) * self.n + r + 1 - (1 << k)] + r + 1 - (1 << k);

        let left = self.data[left_index];
        let right = self.data[right_index];
        if left <= right {
            left_index
        } else {
            right_index
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rand::prelude::*;
    use rand::rngs::Xoshiro256PlusPlus;
    use rand::seq::SliceRandom;

    fn calculate_random_permutation(rng: &mut dyn Rng, n: u64) -> Vec<u64> {
        let mut v: Vec<u64> = (0..n).collect();
        v.shuffle(rng);
        v
    }

    fn get_rng() -> Xoshiro256PlusPlus {
        Xoshiro256PlusPlus::seed_from_u64(42)
    }

    #[test]
    fn test_init() {
        let data = calculate_random_permutation(&mut get_rng(), 100);
        let _table = SparseTable::new(&data);
    }

    fn eval_range<'a>(data: &'a [u64], rmq: &'a dyn RMQ<'a, u64>, l: usize, r: usize) {
        let expected = l + data[l..=r]
            .iter()
            .enumerate()
            .min_by_key(|x| x.1)
            .unwrap()
            .0;
        let actual = rmq.rmq(l, r);
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_query_random() {
        let mut rng = get_rng();
        let data = calculate_random_permutation(&mut rng, 100);
        let _table = SparseTable::new(&data);
        for _ in 0..100 {
            // BUG: Ranges not uniformly random!
            let l = rng.random_range(0..data.len());
            let r = rng.random_range(l..data.len());

            eval_range(&data, &_table, l, r);
        }
    }

    #[test]
    fn test_query_edges() {
        let mut rng = get_rng();
        let data = calculate_random_permutation(&mut rng, 100);
        let _table = SparseTable::new(&data);

        eval_range(&data, &_table, 0, 99);
        eval_range(&data, &_table, 0, 0);
        eval_range(&data, &_table, 99, 99);

        eval_range(&data, &_table, 50, 99);
    }
}
