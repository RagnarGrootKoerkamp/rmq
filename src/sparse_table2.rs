use crate::{base::RMQ, cartesian_tree::{cartesian_tree_identifier}, sparse_table::OffsetSparseTable};



pub struct SparseTableOnBlocks<'a> {
    n: usize,
    b: usize,
    data: &'a [u64],
    block_sparse_table: OffsetSparseTable<'a>,
    in_block_responses: Vec<usize>,
}

impl<'a> RMQ<'a, u64> for SparseTableOnBlocks<'a> {
    fn new(data: &'a [u64]) -> Self
    where
        Self: Sized {
        let n = data.len();
        let b = 1usize.max(((n as f64).log2() / 4f64).floor() as usize);
        let cmin = u64::MAX;
        for i in (0..n).step_by(b) {
            let id = cartesian_tree_identifier(&data[i..n.min(i+b)], b);
            let min_idx = i;
            for j in i+1..i+b {
                if data[j] < data[min_idx] {
                    min_idx = j;
                }
            }
        }
        let obj = SparseTableOnBlocks {
            n: data.len(),
            data,

        }
    }

    fn rmq(&self, l: usize, r: usize) -> usize {
        todo!()
    }
}