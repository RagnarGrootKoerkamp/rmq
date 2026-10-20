use sux::{
    bits::{BitFieldVec, BitFieldVecU},
    traits::{SliceByValue, SliceByValueMut, TryIntoUnaligned},
};

use crate::{
    base::{MemoryUsage, RMQ, TwoArgMin},
    sparse_table::{IndexSparseTable, OwningOffsetSparseTable},
};

pub struct StridedIndexSparseTable<'a, T, const B: usize, const S: usize> {
    data: &'a [T],
    sparse_table: OwningOffsetSparseTable<'a, T>,
}

impl<'a, T, const B: usize, const S: usize> MemoryUsage for StridedIndexSparseTable<'a, T, B, S> {
    fn memory_allocated(&self) -> usize {
        self.sparse_table.memory_allocated()
    }
}

impl<'a, T: Ord + Copy, const B: usize, const S: usize> RMQ<'a, T>
    for StridedIndexSparseTable<'a, T, B, S>
{
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        let n = data.len();
        let mut indices: Vec<usize> = vec![];

        for i in (0..n).step_by(B) {
            let mut idx = i;
            for j in (i + 1..(i + B).min(n)) {
                idx = data.argmin(idx, j);
            }
            indices.push(idx);
        }

        let table = Self {
            data,
            sparse_table: OwningOffsetSparseTable::new(data, &indices),
        };

        table
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        let lblock = l / B;
        let rblock = r / B;

        if lblock == rblock {
            if B > S && r - l > S {
                let over_sample_result = self.sparse_table.rmq(lblock, lblock);
                if (over_sample_result >= l && over_sample_result <= r) {
                    return over_sample_result;
                }
            }
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

impl<'a, T: Ord, const B: usize, const S: usize> StridedIndexSparseTable<'a, T, B, S> {
    fn scan(&self, l: usize, r: usize) -> usize {
        let mut min_index = l;
        for i in l + 1..r + 1 {
            min_index = self.data.argmin(min_index, i);
        }
        min_index
    }
}

crate::rmq_family! {
    pub StridedIndexSparseTableFamily<const B: usize, const S: usize> => StridedIndexSparseTable where T: Ord + Copy
}

pub struct StridedIndexAffixMasksSparseTable<'a, T, const B: usize, const S: usize> {
    data: &'a [T],
    sparse_table: OwningOffsetSparseTable<'a, T>,
    // One B-bit mask per block: prefix minima | suffix minima
    block_affix_masks: BitFieldVec,
}

impl<'a, T, const B: usize, const S: usize> MemoryUsage
    for StridedIndexAffixMasksSparseTable<'a, T, B, S>
{
    fn memory_allocated(&self) -> usize {
        self.sparse_table.memory_allocated() + self.block_affix_masks.memory_allocated()
    }
}

impl<'a, T: Ord + Copy, const B: usize, const S: usize> RMQ<'a, T>
    for StridedIndexAffixMasksSparseTable<'a, T, B, S>
{
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        const { assert!(B <= 64, "affix masks are u64, so B must be <= 64") };

        let n = data.len();
        let k_max = n.ilog2() as usize;
        let mut indices: Vec<usize> = vec![];
        let mut block_affix_masks = BitFieldVec::new(B, n.div_ceil(B));

        for i in (0..n).step_by(B) {
            let end = (i + B).min(n);

            let mut idx = i;
            for j in (i + 1..end) {
                idx = data.argmin(idx, j);
            }
            indices.push(idx);

            // Decreasing sequence from start to smallest
            let mut prefix_seq: u64 = 1;
            for j in (i + 1..end) {
                let last = prefix_seq.ilog2();
                if data[j] < data[i + last as usize] {
                    prefix_seq |= 1 << j - i;
                }
            }

            // Decreasing sequence from end to smallest
            let mut suffix_seq: u64 = 1 << (end - 1 - i);
            for j in (i..end - 1).rev() {
                let last = suffix_seq.trailing_zeros();
                if data[j] <= data[i + last as usize] {
                    suffix_seq |= 1 << j - i;
                }
            }

            // min_idx can be used to separate them
            let affix_seq = prefix_seq | suffix_seq;
            block_affix_masks.set_value(i / B, affix_seq as usize);
        }

        let table = Self {
            data,
            sparse_table: OwningOffsetSparseTable::new(data, &indices),
            block_affix_masks,
        };

        table
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        let lblock = l / B;
        let rblock = r / B;

        if lblock == rblock {
            if B > S && r - l > S {
                let over_sample_result = self.sparse_table.rmq(lblock, lblock);
                if (over_sample_result >= l && over_sample_result <= r) {
                    return over_sample_result;
                }
            }
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

impl<'a, T: Ord + Copy, const B: usize, const S: usize>
    StridedIndexAffixMasksSparseTable<'a, T, B, S>
{
    #[inline]
    fn block_affix_mask(&self, block: usize) -> usize {
        unsafe { self.block_affix_masks.get_value_unchecked(block) }
    }

    // Leftmost minimum of [start of r's block, r]
    #[inline]
    fn prefix_min(&self, r: usize) -> usize {
        let block = r / B;
        let start = block * B;
        let min_offset = self.sparse_table.rmq(block, block) - start;
        // Bits up to min_offset are prefix minima; past it the answer is the block min
        let k = (r - start).min(min_offset);
        let mask = self.block_affix_mask(block) & (usize::MAX >> (usize::BITS as usize - 1 - k));
        start + mask.ilog2() as usize
    }

    // Leftmost minimum of [l, end of l's block]
    #[inline]
    fn suffix_min(&self, l: usize) -> usize {
        let block = l / B;
        let start = block * B;
        let min_offset = self.sparse_table.rmq(block, block) - start;
        // Bits from min_offset on are suffix minima; before it the answer is the block min
        let k = (l - start).max(min_offset);
        let mask = self.block_affix_mask(block) & (usize::MAX << k);
        start + mask.trailing_zeros() as usize
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
    pub StridedIndexAffixMasksSparseTableFamily<const B: usize, const S: usize> => StridedIndexAffixMasksSparseTable where T: Ord + Copy
}

pub struct StridedIndexMinHeapSparseTable<'a, T, const B: usize, const S: usize> {
    data: &'a [T],
    sparse_table: OwningOffsetSparseTable<'a, T>,
    // One 2B-bit cartesian tree identifier per block
    block_ids: BitFieldVec,
}

impl<'a, T, const B: usize, const S: usize> MemoryUsage
    for StridedIndexMinHeapSparseTable<'a, T, B, S>
{
    fn memory_allocated(&self) -> usize {
        self.sparse_table.memory_allocated() + self.block_ids.memory_allocated()
    }
}

impl<'a, T: Ord + Copy, const B: usize, const S: usize> RMQ<'a, T>
    for StridedIndexMinHeapSparseTable<'a, T, B, S>
{
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

        let table = Self {
            data,
            sparse_table: OwningOffsetSparseTable::new(data, &indices),
            block_ids,
        };

        table
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        let lblock = l / B;
        let rblock = r / B;

        if lblock == rblock {
            if B > S && r - l > S {
                let over_sample_result = self.sparse_table.rmq(lblock, lblock);
                if (over_sample_result >= l && over_sample_result <= r) {
                    return over_sample_result;
                }
            }
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

impl<'a, T, const B: usize, const S: usize> StridedIndexMinHeapSparseTable<'a, T, B, S> {
    // Leftmost minimum of block[l..=r], with l and r relative to the block start
    #[inline]
    fn in_block(&self, block: usize, l: usize, r: usize) -> usize {
        let id = unsafe { self.block_ids.get_value_unchecked(block) } as u64;
        block * B + crate::cartesian_tree::rmq_on_minheap(id, B, l, r)
    }
}

crate::rmq_family! {
    pub StridedIndexMinHeapSparseTableFamily<const B: usize, const S: usize> => StridedIndexMinHeapSparseTable where T: Ord + Copy
}

pub struct BitBlocksSparseTable<'a, T, const B: usize> {
    data: &'a [T],
    stack: BitFieldVecU<Box<[u64]>>,
    block_sparse_table: IndexSparseTable<'a, T>,
}

// Leftmost argmin of [l, r], which must satisfy r - l < B.
#[inline]
fn in_window(stack: &BitFieldVecU<Box<[u64]>>, l: usize, r: usize) -> usize {
    let mask = u64::MAX >> (63 - (r - l));
    // SAFETY: callers only pass r < n.
    let word = unsafe { stack.get_value_unchecked(r) };
    r + (word & mask).leading_zeros() as usize - 63
}

impl<'a, T, const B: usize> MemoryUsage for BitBlocksSparseTable<'a, T, B> {
    fn memory_allocated(&self) -> usize {
        self.stack.as_ref().len() * std::mem::size_of::<u64>()
            + self.block_sparse_table.memory_allocated()
    }
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
            block_sparse_table: IndexSparseTable::new(data, &indices),
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
