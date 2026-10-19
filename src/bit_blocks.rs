use crate::{
    base::{MemoryUsage, RMQ, TwoArgMin},
    sparse_table::IndexSparseTable,
};

const B: usize = 64;

pub struct BitBlocksSparseTable<'a, T> {
    data: &'a [T],
    // Bit k of stack[i] is set iff i - k is on the monotonic stack after pushing i.
    stack: Vec<u64>,
    block_sparse_table: IndexSparseTable<'a, T>,
}

// Leftmost argmin of [l, r], which must satisfy r - l < 64.
#[inline]
fn in_window(stack: &[u64], l: usize, r: usize) -> usize {
    // Keeps bits 0..=r-l, i.e. positions l..=r.
    let mask = u64::MAX >> (B - 1 - (r - l));
    r + (stack[r] & mask).leading_zeros() as usize - (B - 1)
}

impl<'a, T> MemoryUsage for BitBlocksSparseTable<'a, T> {
    fn memory_allocated(&self) -> usize {
        self.stack.len() * std::mem::size_of::<u64>()
            + self.block_sparse_table.memory_allocated()
    }
}

impl<'a, T: Ord> RMQ<'a, T> for BitBlocksSparseTable<'a, T> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        let n = data.len();
        let mut stack = vec![0u64; n];
        let mut cur = 0u64;
        for i in 0..n {
            cur <<= 1;
            // Pop strictly larger elements, so the deepest of equal values is the leftmost.
            while cur != 0 {
                let top = cur.trailing_zeros() as usize;
                if data[i - top] <= data[i] {
                    break;
                }
                cur &= !(1u64 << top);
            }
            cur |= 1;
            stack[i] = cur;
        }

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
    pub BitBlocksSparseTableFamily => BitBlocksSparseTable
}
