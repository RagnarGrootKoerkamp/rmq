use mem_dbg::{MemDbg, MemSize};
use sux::{
    bits::{BitFieldVec, BitFieldVecU},
    traits::{SliceByValue, SliceByValueMut, TryIntoUnaligned},
};

use crate::{
    base::{RMQ, TwoArgMin}, sparse_table::{IndexST, OwningOffsetSparseTable, ValueSparseTable},
};

#[derive(MemSize, MemDbg)]
pub struct StridedIndexSparseTable<'a, T, const B: usize> {
    data: &'a [T],
    sparse_table: ValueSparseTable<'a, T>,
}

impl<'a, T: Ord + Copy + Into<u32>, const B: usize> RMQ<'a, T> for StridedIndexSparseTable<'a, T, B> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        let n = data.len();
        let mut indices: Vec<usize> = vec![];

        for i in (0..n).step_by(B) {
            let mut idx = i;
            for j in i + 1..(i + B).min(n) {
                idx = data.argmin(idx, j);
            }
            indices.push(idx);
        }

        Self {
            data,
            sparse_table: ValueSparseTable::new(data, &indices),
        }
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        let lblock = l / B;
        let rblock = r / B;

        if lblock == rblock {
            return self.scan(l, r);
        }

        let over_sample_result = self.sparse_table.rmq(lblock, rblock);

        if over_sample_result < l || over_sample_result > r {
            let left_in_block = self.scan(l, (lblock + 1) * B - 1);
            let right_in_block = self.scan(rblock * B, r);

            if lblock + 1 == rblock {
                self.data.argmin(left_in_block, right_in_block)
            } else {
                let middle = self.sparse_table.rmq(lblock + 1, rblock - 1);
                self.data
                    .argmin(left_in_block, self.data.argmin(middle, right_in_block))
            }
        } else {
            over_sample_result
        }
    }
}

impl<'a, T: Ord, const B: usize> StridedIndexSparseTable<'a, T, B> {
    fn scan(&self, l: usize, r: usize) -> usize {
        let mut min_index = l;
        for i in l + 1..r + 1 {
            min_index = self.data.argmin(min_index, i);
        }
        min_index
    }
}

crate::rmq_family! {
    pub StridedIndexSparseTableFamily<const B: usize> => StridedIndexSparseTable where T: Ord + Copy + Into<u32>
}

#[derive(MemSize, MemDbg)]
pub struct StridedIndexAffixMasksSparseTable<'a, T, const B: usize> {
    data: &'a [T],
    sparse_table: OwningOffsetSparseTable<'a, T>,
    // Bit i set iff data[i] is a prefix or suffix minimum of its block
    affix_bits: Box<[u64]>,
}

impl<'a, T: Ord + Copy, const B: usize> RMQ<'a, T> for StridedIndexAffixMasksSparseTable<'a, T, B> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        let n = data.len();
        let mut indices: Vec<usize> = vec![];
        let mut affix_bits = vec![0u64; n.div_ceil(64)].into_boxed_slice();
        let mut set = |i: usize| affix_bits[i / 64] |= 1 << (i % 64);

        for i in (0..n).step_by(B) {
            let end = (i + B).min(n);

            let mut idx = i;
            for j in i + 1..end {
                idx = data.argmin(idx, j);
            }
            indices.push(idx);

            // Decreasing sequence from start to smallest
            let mut last = i;
            set(i);
            for j in i + 1..=idx {
                if data[j] < data[last] {
                    last = j;
                    set(j);
                }
            }

            // Decreasing sequence from end to smallest
            let mut last = end - 1;
            set(end - 1);
            for j in (idx..end - 1).rev() {
                if data[j] <= data[last] {
                    last = j;
                    set(j);
                }
            }
        }

        // The block min (idx) separates the two sequences at query time
        Self {
            data,
            sparse_table: OwningOffsetSparseTable::new(data, &indices),
            affix_bits,
        }
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        let lblock = l / B;
        let rblock = r / B;

        if lblock == rblock {
            return self.scan(l, r);
        }

        let over_sample_result = self.sparse_table.rmq(lblock, rblock);

        if over_sample_result < l || over_sample_result > r {
            let left_in_block = self.suffix_min(l);
            let right_in_block = self.prefix_min(r);

            if lblock + 1 == rblock {
                self.data.argmin(left_in_block, right_in_block)
            } else {
                let middle = self.sparse_table.rmq(lblock + 1, rblock - 1);
                self.data
                    .argmin(left_in_block, self.data.argmin(middle, right_in_block))
            }
        } else {
            over_sample_result
        }
    }
}

impl<'a, T: Ord + Copy, const B: usize> StridedIndexAffixMasksSparseTable<'a, T, B> {
    #[inline]
    fn affix_word(&self, w: usize) -> u64 {
        unsafe { *self.affix_bits.get_unchecked(w) }
    }

    // Leftmost minimum of [start of r's block, r]
    #[inline]
    fn prefix_min(&self, r: usize) -> usize {
        let block = r / B;
        // Bits up to the block min are prefix minima; past it the answer is the block min
        let p = r.min(self.sparse_table.rmq(block, block));
        // The block start bit is always set, so the scan stops inside the block
        let mut w = p / 64;
        let mut word = self.affix_word(w) & (u64::MAX >> (63 - p % 64));
        while word == 0 {
            w -= 1;
            word = self.affix_word(w);
        }
        w * 64 + word.ilog2() as usize
    }

    // Leftmost minimum of [l, end of l's block]
    #[inline]
    fn suffix_min(&self, l: usize) -> usize {
        let block = l / B;
        // Bits from the block min on are suffix minima; before it the answer is the block min
        let p = l.max(self.sparse_table.rmq(block, block));
        // The block end bit is always set, so the scan stops inside the block
        let mut w = p / 64;
        let mut word = self.affix_word(w) & (u64::MAX << (p % 64));
        while word == 0 {
            w += 1;
            word = self.affix_word(w);
        }
        w * 64 + word.trailing_zeros() as usize
    }

    fn scan(&self, l: usize, r: usize) -> usize {
        let mut min_index = l;
        for i in l + 1..r + 1 {
            min_index = self.data.argmin(min_index, i);
        }
        min_index
    }
}

crate::rmq_family! {
    pub StridedIndexAffixMasksSparseTableFamily<const B: usize> => StridedIndexAffixMasksSparseTable where T: Ord + Copy
}

#[derive(MemSize, MemDbg)]
pub struct StridedIndexMinHeapSparseTable<'a, T, const B: usize> {
    data: &'a [T],
    sparse_table: OwningOffsetSparseTable<'a, T>,
    // One 2B-bit cartesian tree identifier per block
    block_ids: BitFieldVec,
}

impl<'a, T: Ord + Copy, const B: usize> RMQ<'a, T> for StridedIndexMinHeapSparseTable<'a, T, B> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        const { assert!(B <= 32, "identifiers are u64 (2B bits), so B must be <= 32") };

        let n = data.len();
        let mut indices: Vec<usize> = vec![];
        let mut block_ids = BitFieldVec::new(2 * B, n.div_ceil(B));

        for i in (0..n).step_by(B) {
            let end = (i + B).min(n);
            let id = crate::cartesian_tree::convert_to_minheap(&data[i..end], B);
            block_ids.set_value(i / B, id as usize);
            indices.push(i + crate::cartesian_tree::rmq_on_minheap(id, B, 0, end - 1 - i));
        }

        Self {
            data,
            sparse_table: OwningOffsetSparseTable::new(data, &indices),
            block_ids,
        }
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        let lblock = l / B;
        let rblock = r / B;

        if lblock == rblock {
            return self.in_block(lblock, l - lblock * B, r - lblock * B);
        }

        let over_sample_result = self.sparse_table.rmq(lblock, rblock);

        if over_sample_result < l || over_sample_result > r {
            let left_in_block = self.in_block(lblock, l - lblock * B, B - 1);
            let right_in_block = self.in_block(rblock, 0, r - rblock * B);

            if lblock + 1 == rblock {
                self.data.argmin(left_in_block, right_in_block)
            } else {
                let middle = self.sparse_table.rmq(lblock + 1, rblock - 1);
                self.data
                    .argmin(left_in_block, self.data.argmin(middle, right_in_block))
            }
        } else {
            over_sample_result
        }
    }
}

impl<'a, T, const B: usize> StridedIndexMinHeapSparseTable<'a, T, B> {
    // Leftmost minimum of block[l..=r], with l and r relative to the block start
    #[inline]
    fn in_block(&self, block: usize, l: usize, r: usize) -> usize {
        let id = unsafe { self.block_ids.get_value_unchecked(block) } as u64;
        block * B + crate::cartesian_tree::rmq_on_minheap(id, B, l, r)
    }
}

crate::rmq_family! {
    pub StridedIndexMinHeapSparseTableFamily<const B: usize> => StridedIndexMinHeapSparseTable where T: Ord + Copy
}

#[derive(MemSize, MemDbg)]
pub struct BitBlocksSparseTable<'a, T, const B: usize> {
    data: &'a [T],
    stack: BitFieldVecU<Box<[u64]>>,
    block_sparse_table: IndexST<'a, T>,
}

// Leftmost argmin of [l, r], which must satisfy r - l < B.
#[inline]
fn in_window(stack: &BitFieldVecU<Box<[u64]>>, l: usize, r: usize) -> usize {
    let mask = u64::MAX >> (63 - (r - l));
    // SAFETY: callers only pass r < n.
    let word = unsafe { stack.get_value_unchecked(r) };
    r + (word & mask).leading_zeros() as usize - 63
}

impl<'a, T: Ord, const B: usize> RMQ<'a, T> for BitBlocksSparseTable<'a, T, B> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        // Widths allowed by BitFieldVecU's unaligned reads on u64 words.
        const { assert!(B >= 1 && (B <= 58 || B == 60 || B == 64)) };

        let n = data.len();
        let width_mask = u64::MAX >> (64 - B);
        let mut stack = BitFieldVec::<Box<[u64]>>::new_padded(B, n);
        let mut cur = 0u64;
        for i in 0..n {
            cur = (cur << 1) & width_mask;
            // Pop strictly larger elements, so the deepest of equal values is the leftmost.
            while cur != 0 {
                let top = cur.trailing_zeros() as usize;
                if data[i - top] <= data[i] {
                    break;
                }
                cur &= !(1u64 << top);
            }
            cur |= 1;
            stack.set_value(i, cur);
        }
        let stack = stack
            .try_into_unaligned()
            .expect("bit width checked at compile time");

        let indices: Vec<usize> = (0..n)
            .step_by(B)
            .map(|i| in_window(&stack, i, n.min(i + B) - 1))
            .collect();
        BitBlocksSparseTable {
            data,
            stack,
            block_sparse_table: IndexST::new(data, &indices),
        }
    }

    fn rmq(&self, l: usize, r: usize) -> usize {
        if r - l < B {
            return in_window(&self.stack, l, r);
        }

        // Ranges longer than a block always span at least two blocks.
        let l_block = l / B;
        let r_block = r / B;
        let left = in_window(&self.stack, l, l_block * B + B - 1);
        let right = in_window(&self.stack, r_block * B, r);

        // Always query a valid block range, then discard it if the middle is empty.
        let lo = l_block + 1;
        let hi = (r_block - 1).max(lo);
        let mid = self.block_sparse_table.rmq(lo, hi);
        let mid = if r_block > l_block + 1 { mid } else { left };

        // Combine left to right so ties resolve to the leftmost index.
        self.data.argmin(self.data.argmin(left, mid), right)
    }
}

crate::rmq_family! {
    pub BitBlocksSparseTableFamily<const B: usize> => BitBlocksSparseTable
}
