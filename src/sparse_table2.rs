use crate::{
    base::{RMQ, RMQFamily, TwoArgMin},
    cartesian_tree::cartesian_tree_identifier,
    sparse_table::IndexSparseTable,
};

pub struct SparseTableOnBlocks<'a> {
    b: usize,
    data: &'a [u64],
    block_id: Vec<usize>,
    block_responses: Vec<usize>,
    block_sparse_table: IndexSparseTable<'a>,
}

#[inline]
fn _range_index(l: usize, r: usize) -> usize {
    debug_assert!(l <= r);
    r * (r + 1) / 2 + l
}

impl<'a> SparseTableOnBlocks<'a> {
    #[inline]
    fn in_block(&self, block: usize, l: usize, r: usize) -> usize {
        block * self.b + self.block_responses[self.block_id[block] + _range_index(l, r)]
    }
}

impl<'a> RMQ<'a, u64> for SparseTableOnBlocks<'a> {
    fn new(data: &'a [u64]) -> Self
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
            let end = n.min(i + b);
            let id = cartesian_tree_identifier(&data[i..end], b) as usize;
            if id_map[id] == usize::MAX {
                let offset = block_responses.len();
                id_map[id] = offset;
                block_responses.resize(offset + lr_pairs, 0);
                let value = |j: usize| if i + j < n { data[i + j] } else { u64::MAX };
                for l in 0..b {
                    let mut min_idx = l;
                    for r in l..b {
                        if value(r) < value(min_idx) {
                            min_idx = r;
                        }
                        block_responses[offset + _range_index(l, r)] = min_idx;
                    }
                }
            }

            block_id.push(id_map[id]);
            indices.push(i + block_responses[id_map[id] + _range_index(0, end - 1 - i)]);
        }
        SparseTableOnBlocks {
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

pub struct SparseTableOnBlocksFamily;

impl RMQFamily<u64> for SparseTableOnBlocksFamily {
    type Rmq<'a> = SparseTableOnBlocks<'a>;
}
