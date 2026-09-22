use crate::base::{RMQ, RMQFamily};

pub struct OffsetSparseTable<'a> {
    n: usize,
    k_max: usize,
    data: &'a [u64],
    // Offsets can be packed more tightly
    offsets: Vec<usize>,
}

impl<'a> RMQ<'a, u64> for OffsetSparseTable<'a> {
    fn new(data: &'a [u64]) -> Self {
        let n = data.len();
        let k_max = n.ilog2() as usize;
        let mut table = Self {
            n,
            k_max,
            data,
            offsets: vec![0; (((k_max) * n))],
        };
        for i in 0..table.n - 1 {
            table.offsets[i] = if table.data[i] <= table.data[i + 1] {
                0
            } else {
                1
            }
        }
        for k in 2..=table.k_max {
            for i in 0..table.n + 1 - (1 << k) {
                let loffset = table.offsets[(k - 2) * table.n + i];
                let roffset = table.offsets[(k - 2) * table.n + i + (1 << (k - 1))] + (1 << (k - 1));
                table.offsets[(k-1) * table.n + i] =
                    if table.data[i + loffset] <= table.data[i + roffset] {
                        loffset
                    } else {
                        roffset
                    };
            }
        }
        table
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        if l == r {
            return l;
        }

        let k = (r - l + 1).ilog2() as usize;
        let left_index = self.offsets[(k - 1) * self.n + l] + l;
        let right_index = self.offsets[(k - 1) * self.n + r + 1 - (1 << k)] + r + 1 - (1 << k);

        let left = self.data[left_index];
        let right = self.data[right_index];
        if left <= right {
            left_index
        } else {
            right_index
        }
    }
}

/// Family type for [`OffsetSparseTable`]. Never instantiated; it exists only so
/// generic code can name the structure without committing to an input lifetime.
pub struct SparseTableFamily;

impl RMQFamily<u64> for SparseTableFamily {
    type Rmq<'a> = OffsetSparseTable<'a>;
}
