use std::collections::HashMap;

use crate::{
    base::{MemoryUsage, RMQ, TwoArgMin},
    cartesian_tree::convert_to_minheap,
    sparse_table::{IndexSparseTable, PositionOffsetSparseTable},
};

use sux::{
    bits::BitFieldVec,
    traits::{SliceByValue, SliceByValueMut},
};

pub struct SparseTableCartesianBlocks<'a, T> {
    b: usize,
    data: &'a [T],
    block_id: Vec<usize>,
    block_responses: Vec<usize>,
    block_sparse_table: IndexSparseTable<'a, T>,
}

#[inline]
fn _range_index(l: usize, r: usize) -> usize {
    debug_assert!(l <= r);
    r * (r + 1) / 2 + l
}

impl<'a, T: Ord> SparseTableCartesianBlocks<'a, T> {
    #[inline]
    fn in_block(&self, block: usize, l: usize, r: usize) -> usize {
        block * self.b + self.block_responses[self.block_id[block] + _range_index(l, r)]
    }
}

impl<'a, T> MemoryUsage for SparseTableCartesianBlocks<'a, T> {
    fn memory_allocated(&self) -> usize {
        (self.block_id.len() + self.block_responses.len()) * std::mem::size_of::<usize>()
            + self.block_sparse_table.memory_allocated()
    }
}

impl<'a, T: Ord> RMQ<'a, T> for SparseTableCartesianBlocks<'a, T> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        let n = data.len();
        let b = 1usize.max(((n as f64).log2() / 4f64).floor() as usize);
        let lr_pairs = b * (b + 1) / 2;

        let mut id_map = vec![usize::MAX; 1 << (2 * b)];
        let mut block_responses: Vec<usize> = vec![];
        let mut block_id = vec![];
        let mut indices = vec![];

        for i in (0..n).step_by(b) {
            let block = &data[i..n.min(i + b)];
            let id = convert_to_minheap(block, b) as usize;
            if id_map[id] == usize::MAX {
                let offset = block_responses.len();
                id_map[id] = offset;
                block_responses.resize(offset + lr_pairs, 0);
                // Only the last block can be short, and nothing queries past its end,
                // so ranges beyond it can stay unfilled.
                for l in 0..block.len() {
                    let mut min_idx = l;
                    for r in l..block.len() {
                        min_idx = block.argmin(min_idx, r);
                        block_responses[offset + _range_index(l, r)] = min_idx;
                    }
                }
            }

            block_id.push(id_map[id]);
            indices.push(i + block_responses[id_map[id] + _range_index(0, block.len() - 1)]);
        }
        SparseTableCartesianBlocks {
            b,
            data,
            block_id,
            block_responses,
            block_sparse_table: IndexSparseTable::new(data, &indices),
        }
    }

    fn rmq(&self, l: usize, r: usize) -> usize {
        let l_block = l / self.b;
        let r_block = r / self.b;

        if l_block == r_block {
            self.in_block(l_block, l % self.b, r % self.b)
        } else {
            let mut ans = self.in_block(l_block, l % self.b, self.b - 1);
            if r_block - l_block > 1 {
                ans = self
                    .data
                    .argmin(ans, self.block_sparse_table.rmq(l_block + 1, r_block - 1));
            }
            self.data.argmin(ans, self.in_block(r_block, 0, r % self.b))
        }
    }
}

crate::rmq_family! {
    pub SparseTableCartesianBlocksFamily => SparseTableCartesianBlocks
}

/// Fills `out` with in-block answers for every range of a size-`s` block.
/// A short block is treated as padded with +inf, matching the padding in
/// `cartesian_tree_identifier`, so the answers are valid for any block sharing its id.
fn fill_block_answers<T: Ord>(block: &[T], s: usize, out: &mut [u16]) {
    for l in 0..s {
        let mut min_idx = l;
        for r in l..s {
            if r < block.len() {
                min_idx = block.argmin(min_idx, r);
            }
            out[_range_index(l, r)] = min_idx as u16;
        }
    }
}

pub struct SparseTable3<'a, T> {
    n: usize,
    s: usize,
    data: &'a [T],
    p: BitFieldVec,
    t: BitFieldVec,
    t_: BitFieldVec,
    oblock_table: PositionOffsetSparseTable<'a, T>,
}

impl<'a, T> MemoryUsage for SparseTable3<'a, T> {
    fn memory_allocated(&self) -> usize {
        self.p.memory_allocated()
            + self.t.memory_allocated()
            + self.t_.memory_allocated()
            + self.oblock_table.memory_allocated()
    }
}

impl<'a, T: Ord> SparseTable3<'a, T> {
    #[inline]
    fn in_block(&self, block: usize, l: usize, r: usize) -> usize {
        unsafe {
            let offset = self.t.get_value_unchecked(block);
            block * self.s + self.p.get_value_unchecked(offset + _range_index(l, r))
        }
    }

    #[inline]
    fn in_oblock(&self, oblock: usize, l: usize, r: usize) -> usize {
        let j = unsafe {
            let offset = self.t_.get_value_unchecked(oblock);
            self.p.get_value_unchecked(offset + _range_index(l, r))
        };
        self.a_p_index(oblock * self.s + j)
    }

    #[inline]
    /// Data position of A'[i], the minimum of block i.
    fn a_p_index(&self, i: usize) -> usize {
        self.in_block(i, 0, self.s - 1)
    }
}

impl<'a, T: Ord + Copy> RMQ<'a, T> for SparseTable3<'a, T> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        let n = data.len();
        let s: usize = ((n as f64).log2() / 4f64).ceil().max(1f64) as usize;
        let lr_pairs = s * (s + 1) / 2;

        let mut p_vec: Vec<u16> = Vec::new();
        let mut id_map: HashMap<u64, usize> = HashMap::new();
        let mut t_vec: Vec<usize> = Vec::new();

        let mut a_p_indices: Vec<usize> = Vec::new();

        for i in (0..n).step_by(s) {
            let block = &data[i..n.min(i + s)];
            let id = convert_to_minheap(block, s);
            if let std::collections::hash_map::Entry::Vacant(e) = id_map.entry(id) {
                let offset = p_vec.len();
                e.insert(offset);
                p_vec.resize(offset + lr_pairs, 0);
                fill_block_answers(block, s, &mut p_vec[offset..]);
            }

            t_vec.push(id_map[&id]);
            a_p_indices.push(i + p_vec[id_map[&id] + _range_index(0, s - 1)] as usize);
        }

        let mut t = BitFieldVec::new(2 * s, t_vec.len());
        for i in 0..t_vec.len() {
            t.set_value(i, t_vec[i]);
        }

        let n_p = n.div_ceil(s);
        let mut t_p_vec: Vec<usize> = Vec::new();
        let mut a_pp_indices: Vec<usize> = Vec::new();

        for i in (0..n_p).step_by(s) {
            let block: Vec<T> = a_p_indices[i..n_p.min(i + s)]
                .iter()
                .map(|idx| data[*idx])
                .collect();
            let id = convert_to_minheap(&block, s);

            if let std::collections::hash_map::Entry::Vacant(e) = id_map.entry(id) {
                let offset = p_vec.len();
                e.insert(offset);
                p_vec.resize(offset + lr_pairs, 0);
                fill_block_answers(&block, s, &mut p_vec[offset..]);
            }

            t_p_vec.push(id_map[&id]);
            // Data position of the out-block minimum, so the table compares real values.
            a_pp_indices
                .push(a_p_indices[i + p_vec[id_map[&id] + _range_index(0, s - 1)] as usize]);
        }

        let mut t_ = BitFieldVec::new(2 * s, t_p_vec.len());
        for i in 0..t_p_vec.len() {
            t_.set_value(i, t_p_vec[i]);
        }

        let mut p = BitFieldVec::new(s.ilog2() as usize + 1, p_vec.len());
        for i in 0..p_vec.len() {
            p.set_value(i, p_vec[i] as usize);
        }

        SparseTable3 {
            n,
            s,
            data,
            p,
            t,
            t_,
            oblock_table: PositionOffsetSparseTable::new(data, &a_pp_indices),
        }
    }

    fn rmq(&self, l: usize, r: usize) -> usize {
        let left_iblock = l / self.s;
        let right_iblock = r / self.s;

        if left_iblock == right_iblock {
            return self.in_block(left_iblock, l % self.s, r % self.s);
        }
        let left_iblock_result = self.in_block(left_iblock, l % self.s, self.s - 1);
        let right_iblock_result = self.in_block(right_iblock, 0, r % self.s);

        // Whole blocks strictly between the two partial ones, as indices into A'.
        // Results are combined left to right so ties resolve to the leftmost minimum.
        if right_iblock - left_iblock == 1 {
            return self.data.argmin(left_iblock_result, right_iblock_result);
        }
        let left_mid = left_iblock + 1;
        let right_mid = right_iblock - 1;

        let left_oblock = left_mid / self.s;
        let right_oblock = right_mid / self.s;

        let mid_result = if left_oblock == right_oblock {
            self.in_oblock(left_oblock, left_mid % self.s, right_mid % self.s)
        } else {
            let left_oblock_result = self.in_oblock(left_oblock, left_mid % self.s, self.s - 1);
            let right_oblock_result = self.in_oblock(right_oblock, 0, right_mid % self.s);
            if right_oblock - left_oblock > 1 {
                let table_result = self.oblock_table.rmq(left_oblock + 1, right_oblock - 1);
                self.data.argmin(
                    self.data.argmin(left_oblock_result, table_result),
                    right_oblock_result,
                )
            } else {
                self.data.argmin(left_oblock_result, right_oblock_result)
            }
        };

        self.data.argmin(
            self.data.argmin(left_iblock_result, mid_result),
            right_iblock_result,
        )
    }
}

crate::rmq_family! {
    pub SparseTable3Family => SparseTable3 where T: Ord + Copy
}
