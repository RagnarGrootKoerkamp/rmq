use crate::base::{RMQ, RMQFamily, TwoArgMin};

pub struct RMQTree<'a> {
    n: usize,
    data: &'a[u64],
    tree: Vec<usize>,
}

impl<'a> RMQ<'a, u64> for RMQTree<'a> {
    fn new(data: &'a [u64]) -> Self {
        let n = data.len();
        let mut tree = Self {
            n,
            data,
            tree: vec![0; n],
        };
        for i in (1..n).rev() {
            let left_index = if 2*i < n {tree.tree[2*i]} else {2*i-n};
            let right_index = if 2*i+1 < n {tree.tree[2*i+1]} else {2*i+1-n};
            tree.tree[i] = tree.data.argmin(left_index, right_index);
        }
        tree
    }

    // Queries [l, r]
    fn rmq(&self, mut l: usize, mut r: usize) -> usize {
        let mut left_min = l;
        let mut right_min = r;
        l += self.n;
        r += self.n + 1;
        if l % 2 == 1 {
            l += 1;
        }
        if r % 2 == 1 {
            r -= 1;
        }
        l /= 2;
        r /= 2;
        while l < r {
            if l % 2 == 1 {
                left_min = self.data.argmin(left_min, self.tree[l]);
                l += 1;
            }
            if r % 2 == 1 {
                right_min = self.data.argmin(self.tree[((r - 1))], right_min);
                r -= 1;
            }
            l /= 2;
            r /= 2;
        }
        self.data.argmin(left_min, right_min)
    }
}

/// Family type for [`RMQTree`]. Never instantiated; it exists only so generic
/// code can name the structure without committing to an input lifetime.
pub struct RMQTreeFamily;

impl RMQFamily<u64> for RMQTreeFamily {
    type Rmq<'a> = RMQTree<'a>;
}
