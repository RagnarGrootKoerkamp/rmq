use crate::{
    base::{MemoryUsage, RMQ, TwoArgMin},
    cartesian_tree::cartesian_tree_identifier,
    sparse_table::IndexSparseTable,
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
    fn memory_bytes(&self) -> usize {
        // The nested table's own struct size is already part of size_of::<Self>().
        std::mem::size_of::<Self>() - std::mem::size_of::<IndexSparseTable<T>>()
            + (self.block_id.len() + self.block_responses.len()) * std::mem::size_of::<usize>()
            + self.block_sparse_table.memory_bytes()
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
            let id = cartesian_tree_identifier(block, b) as usize;
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
