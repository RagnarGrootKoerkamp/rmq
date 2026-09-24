use crate::base::{RMQ, RMQFamily, TwoArgMin};

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
            offsets: vec![0; (k_max) * n],
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
                let roffset =
                    table.offsets[(k - 2) * table.n + i + (1 << (k - 1))] + (1 << (k - 1));
                table.offsets[(k - 1) * table.n + i] =
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

        self.data.argmin(left_index, right_index)
    }
}

/// Family type for [`OffsetSparseTable`]. Never instantiated; it exists only so
/// generic code can name the structure without committing to an input lifetime.
pub struct SparseTableFamily;

impl RMQFamily<u64> for SparseTableFamily {
    type Rmq<'a> = OffsetSparseTable<'a>;
}

pub struct IndexSparseTable<'a> {
    n: usize,
    k_max: usize,
    data: &'a [u64],
    indices: Vec<usize>,
}

impl<'a> IndexSparseTable<'a> {
    pub fn new(data: &'a [u64], indices: &[usize]) -> Self {
        let n = indices.len();
        let k_max = n.ilog2() as usize;
        let mut table = Self {
            n,
            k_max,
            data,
            indices: vec![0; (k_max + 1) * n],
        };

        for i in 0..n {
            table.indices[i] = indices[i];
        }
        for k in 1..=table.k_max {
            for i in 0..table.n + 1 - (1 << k) {
                let lindex = table.indices[(k - 1) * table.n + i];
                let rindex = table.indices[(k - 1) * table.n + i + (1 << (k - 1))];
                table.indices[k * n + i] = data.argmin(lindex, rindex);
            }
        }
        table
    }

    // Queries [l, r]
    pub fn rmq(&self, l: usize, r: usize) -> usize {
        let k = (r - l + 1).ilog2() as usize;
        let left_index = self.indices[k * self.n + l];
        let right_index = self.indices[k * self.n + r + 1 - (1 << k)];

        self.data.argmin(left_index, right_index)
    }
}
