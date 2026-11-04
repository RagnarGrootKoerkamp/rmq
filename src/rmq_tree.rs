use mem_dbg::{MemDbg, MemSize};

use crate::base::{RMQ, TwoArgMin};

use sux::{
    bits::BitFieldVec,
    traits::{SliceByValue, SliceByValueMut},
};

#[derive(MemSize, MemDbg)]
pub struct RMQTree<'a, T> {
    n: usize,
    data: &'a [T],
    tree: Vec<usize>,
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

#[derive(MemSize, MemDbg)]
pub struct PackedRMQTree<'a, T> {
    n: usize,
    data: &'a [T],
    tree: BitFieldVec,
}

impl<'a, T: Ord> RMQ<'a, T> for PackedRMQTree<'a, T> {
    fn new(data: &'a [T]) -> Self {
        let n = data.len();
        // Every stored index is < n, so n - 1 bounds the width.
        let width = (usize::BITS - n.saturating_sub(1).leading_zeros()).max(1) as usize;
        let mut tree = Self {
            n,
            data,
            tree: BitFieldVec::new(width, n),
        };
        for i in (1..n).rev() {
            let left_index = if 2 * i < n {
                unsafe { tree.tree.get_value_unchecked(2 * i) }
            } else {
                2 * i - n
            };
            let right_index = if 2 * i + 1 < n {
                unsafe { tree.tree.get_value_unchecked(2 * i + 1) }
            } else {
                2 * i + 1 - n
            };
            tree.tree
                .set_value(i, tree.data.argmin(left_index, right_index));
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
                left_min = self
                    .data
                    .argmin(left_min, unsafe { self.tree.get_value_unchecked(l) });
                l += 1;
            }
            if r % 2 == 1 {
                right_min = self
                    .data
                    .argmin(unsafe { self.tree.get_value_unchecked(r - 1) }, right_min);
                r -= 1;
            }
            l /= 2;
            r /= 2;
        }
        self.data.argmin(left_min, right_min)
    }
}

crate::rmq_family! {
    pub PackedRMQTreeFamily => PackedRMQTree
}

/// Bottom-up segment tree with fanout `F`, which must be a power of two `>= 2`.
///
/// Level 0 is the input itself and is not stored. Level `k + 1` holds, for each
/// *complete* block of `F` consecutive entries of level `k`, the index of the
/// block's leftmost minimum. Queries never ascend through a trailing partial
/// block, so those are dropped: level `k` has `n / F^k` entries and the whole
/// tree about `n / (F - 1)`.
#[derive(MemSize, MemDbg)]
pub struct FanoutRMQTree<'a, T, const F: usize> {
    data: &'a [T],
    /// Levels `1, 2, ...` concatenated.
    tree: Vec<usize>,
    /// Level `k` occupies `tree[offsets[k - 1]..offsets[k]]`.
    offsets: Vec<usize>,
}

impl<'a, T, const F: usize> FanoutRMQTree<'a, T, F> {
    const VALID_FANOUT: () = assert!(
        F >= 2 && F.is_power_of_two(),
        "fanout must be a power of two >= 2"
    );

    fn levels(&self) -> impl Iterator<Item = &[usize]> {
        self.offsets.windows(2).map(|w| &self.tree[w[0]..w[1]])
    }
}

impl<'a, T: Ord, const F: usize> RMQ<'a, T> for FanoutRMQTree<'a, T, F> {
    fn new(data: &'a [T]) -> Self {
        #[allow(clippy::let_unit_value)]
        let () = Self::VALID_FANOUT;
        let n = data.len();
        let mut tree = Vec::with_capacity(n / (F - 1));
        let mut offsets = vec![0];

        let mut len = n / F;
        for b in 0..len {
            let start = b * F;
            let mut min = start;
            for i in start + 1..start + F {
                min = data.argmin(min, i);
            }
            tree.push(min);
        }
        offsets.push(tree.len());

        let mut prev = 0;
        while len >= F {
            let next = len / F;
            for b in 0..next {
                let start = prev + b * F;
                let block = &tree[start..start + F];
                let min = block[1..]
                    .iter()
                    .fold(block[0], |min, &m| data.argmin(min, m));
                tree.push(min);
            }
            prev += len;
            len = next;
            offsets.push(tree.len());
        }

        Self {
            data,
            tree,
            offsets,
        }
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        let data = self.data;

        // Complete blocks of the current level are lb..rb; the partial edges
        // l..lb*F and rb*F..=r are scanned here and the rest moves up a level.
        // left_min only absorbs ranges to its right and right_min ranges to
        // its left, so ties resolve to the leftmost position.
        let lb = l.div_ceil(F);
        let rb = (r + 1) / F;
        let mut left_min = l;
        if lb >= rb {
            for i in l + 1..=r {
                left_min = data.argmin(left_min, i);
            }
            return left_min;
        }
        for i in l + 1..lb * F {
            left_min = data.argmin(left_min, i);
        }
        let mut right_min = r;
        for i in (rb * F..r).rev() {
            right_min = data.argmin(i, right_min);
        }
        let (mut l, mut r) = (lb, rb - 1);

        // Ascending from level k means rb <= len_k / F = len_{k+1}, so the next
        // level exists and contains r. The top level has fewer than F entries,
        // hence no complete block, so the loop always returns.
        for level in self.levels() {
            let lb = l.div_ceil(F);
            let rb = (r + 1) / F;
            if lb >= rb {
                for &m in &level[l..=r] {
                    left_min = data.argmin(left_min, m);
                }
                return data.argmin(left_min, right_min);
            }
            for &m in &level[l..lb * F] {
                left_min = data.argmin(left_min, m);
            }
            for &m in level[rb * F..=r].iter().rev() {
                right_min = data.argmin(m, right_min);
            }
            (l, r) = (lb, rb - 1);
        }
        unreachable!("the top level has no complete block")
    }
}

crate::rmq_family! {
    pub FanoutRMQTreeFamily<const F: usize> => FanoutRMQTree
}
