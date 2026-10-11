use crate::{
    base::{MemoryUsage, RMQ, TwoArgMin},
    packed_vec::PackedVec,
};

pub struct OffsetSparseTable<'a, T> {
    n: usize,
    k_max: usize,
    data: &'a [T],
    // Offsets can be packed more tightly
    offsets: Vec<PackedVec>,
}

impl<'a, T> MemoryUsage for OffsetSparseTable<'a, T> {
    fn memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.offsets.len() * std::mem::size_of::<PackedVec>()
            + self.offsets.iter().map(|o| o.memory_bytes()).sum::<usize>()
    }
}

impl<'a, T: Ord> RMQ<'a, T> for OffsetSparseTable<'a, T> {
    fn new(data: &'a [T]) -> Self {
        let n = data.len();
        let k_max = n.ilog2() as usize;
        let mut table = Self {
            n,
            k_max,
            data,
            offsets: (1..=k_max).map(|k| PackedVec::new(n, k as u32)).collect(),
        };
        for i in 0..table.n - 1 {
            table.offsets[0].set(
                i,
                if table.data[i] <= table.data[i + 1] {
                    0
                } else {
                    1
                },
            );
        }
        for k in 2..=table.k_max {
            for i in 0..table.n + 1 - (1 << k) {
                let loffset = table.offsets[k - 2].get(i);
                let roffset = table.offsets[k - 2].get(i + (1 << (k - 1))) + (1 << (k - 1));
                table.offsets[k - 1].set(
                    i,
                    if table.data[i + loffset as usize] <= table.data[i + roffset as usize] {
                        loffset
                    } else {
                        roffset
                    },
                );
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
        let left_index = self.offsets[k - 1].get(l) as usize + l;
        let right_index = self.offsets[k - 1].get(r + 1 - (1 << k)) as usize + r + 1 - (1 << k);

        self.data.argmin(left_index, right_index)
    }
}

crate::rmq_family! {
    pub OffsetSparseTableFamily => OffsetSparseTable
}

pub struct IndexSparseTable<'a, T> {
    n: usize,
    k_max: usize,
    data: &'a [T],
    indices: Vec<usize>,
}

impl<'a, T: Ord> IndexSparseTable<'a, T> {
    pub fn new(data: &'a [T], indices: &[usize]) -> Self {
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
}

impl<'a, T> MemoryUsage for IndexSparseTable<'a, T> {
    fn memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.indices.len() * std::mem::size_of::<usize>()
    }
}

impl<'a, T: Ord> RMQ<'a, T> for IndexSparseTable<'a, T> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        let indices: Vec<usize> = (0..data.len()).collect();
        IndexSparseTable::new(data, &indices)
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        let k = (r - l + 1).ilog2() as usize;
        let left_index = self.indices[k * self.n + l];
        let right_index = self.indices[k * self.n + r + 1 - (1 << k)];

        self.data.argmin(left_index, right_index)
    }
}

crate::rmq_family! {
    pub IndexSparseTableFamily => IndexSparseTable
}

pub struct PackedIndexSparseTable<'a, T> {
    n: usize,
    k_max: usize,
    data: &'a [T],
    indices: PackedVec,
}

impl<'a, T: Ord> PackedIndexSparseTable<'a, T> {
    pub fn new(data: &'a [T], indices: &[usize]) -> Self {
        let n = indices.len();
        let k_max = n.ilog2() as usize;
        // Every stored index is one of `indices`, so the largest one bounds the width.
        let max_index = indices.iter().copied().max().unwrap_or(0) as u64;
        let width = (u64::BITS - max_index.leading_zeros()).max(1);
        let mut table = Self {
            n,
            k_max,
            data,
            indices: PackedVec::new((k_max + 1) * n, width),
        };

        for i in 0..n {
            table.indices.set(i, indices[i] as u64);
        }
        for k in 1..=table.k_max {
            for i in 0..table.n + 1 - (1 << k) {
                let lindex = table.indices.get((k - 1) * table.n + i) as usize;
                let rindex = table.indices.get((k - 1) * table.n + i + (1 << (k - 1))) as usize;
                table
                    .indices
                    .set(k * n + i, data.argmin(lindex, rindex) as u64);
            }
        }
        table
    }
}

impl<'a, T> MemoryUsage for PackedIndexSparseTable<'a, T> {
    fn memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.indices.memory_bytes()
    }
}

impl<'a, T: Ord> RMQ<'a, T> for PackedIndexSparseTable<'a, T> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        let indices: Vec<usize> = (0..data.len()).collect();
        PackedIndexSparseTable::new(data, &indices)
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        let k = (r - l + 1).ilog2() as usize;
        let left_index = self.indices.get(k * self.n + l) as usize;
        let right_index = self.indices.get(k * self.n + r + 1 - (1 << k)) as usize;

        self.data.argmin(left_index, right_index)
    }
}

crate::rmq_family! {
    pub PackedIndexSparseTableFamily => PackedIndexSparseTable
}
