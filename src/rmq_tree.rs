use crate::base::RMQ;

pub struct RMQTree<'a> {
    n: usize,
    data: Vec<&'a u64>,
}

impl<'a> RMQTree<'a> {
    fn _init(&mut self) {
        for i in (1..(self.n as usize) - 1).rev() {
            self.data[i] = self.data[2 * i].min(self.data[2 * i + 1]);
        }
    }
}

impl<'a> RMQ<'a, u64> for RMQTree<'a> {
    fn new(reference: &'a [u64]) -> Self {
        let n = reference.len();
        let mut tree = Self {
            n,
            data: vec![&reference[0]; (2 * n) as usize],
        };
        for i in 0..n {
            tree.data[(n + i) as usize] = &reference[i as usize];
        }
        tree._init();
        tree
    }

    // Queries [l, r]
    fn rmq(&self, mut l: usize, mut r: usize) -> usize {
        l += self.n;
        r += self.n + 1;
        let mut left_min = self.data[l as usize];
        let mut right_min = self.data[(r - 1) as usize];
        while l < r {
            if l % 2 == 1 {
                left_min = left_min.min(self.data[l as usize]);
                l += 1;
            }
            if r % 2 == 1 {
                right_min = self.data[(r - 1) as usize].min(right_min);
                r -= 1;
            }
            l /= 2;
            r /= 2;
        }
        let pointer: *const u64 = (left_min.min(right_min));
        let origin: *const u64 = self.data[0];
        unsafe { (pointer.offset_from(origin)) as usize }
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
        let _table = RMQTree::new(&data);
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
        let _table = RMQTree::new(&data);
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
        let _table = RMQTree::new(&data);

        eval_range(&data, &_table, 0, 99);
        eval_range(&data, &_table, 0, 0);
        eval_range(&data, &_table, 99, 99);

        eval_range(&data, &_table, 50, 99);
    }
}
