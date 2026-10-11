use crate::base::{MemoryUsage, RMQ, TwoArgMin};

pub struct RMQTree<'a, T> {
    n: usize,
    data: &'a [T],
    tree: Vec<usize>,
}

impl<'a, T> MemoryUsage for RMQTree<'a, T> {
    fn memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.tree.len() * std::mem::size_of::<usize>()
    }
}

impl<'a, T: Ord> RMQ<'a, T> for RMQTree<'a, T> {
    fn new(data: &'a [T]) -> Self {
        let n = data.len();
        let mut tree = Self {
            n,
            data,
            tree: vec![0; n],
        };
        for i in (1..n).rev() {
            let left_index = if 2 * i < n {
                tree.tree[2 * i]
            } else {
                2 * i - n
            };
            let right_index = if 2 * i + 1 < n {
                tree.tree[2 * i + 1]
            } else {
                2 * i + 1 - n
            };
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
                right_min = self.data.argmin(self.tree[r - 1], right_min);
                r -= 1;
            }
            l /= 2;
            r /= 2;
        }
        self.data.argmin(left_min, right_min)
    }
}

crate::rmq_family! {
    pub RMQTreeFamily => RMQTree
}
