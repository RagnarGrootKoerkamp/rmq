use std::{marker::PhantomData, mem::MaybeUninit};

use crate::base::{MemoryUsage, RMQ, TwoArgMin};

use sux::{
    bits::BitFieldVec,
    traits::{SliceByValue, SliceByValueMut},
};

pub struct OffsetSparseTable<'a, T> {
    n: usize,
    k_max: usize,
    data: &'a [T],
    offsets: Vec<BitFieldVec>,
}

impl<'a, T> MemoryUsage for OffsetSparseTable<'a, T> {
    fn memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.offsets.len() * std::mem::size_of::<BitFieldVec>()
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
            offsets: (1..=k_max)
                .map(|k| BitFieldVec::new(k as usize, n))
                .collect(),
        };
        for i in 0..table.n - 1 {
            table.offsets[0].set_value(i, (table.data.argmin(i, i + 1) - i));
        }
        for k in 2..=table.k_max {
            for i in 0..table.n + 1 - (1 << k) {
                let (loffset, roffset) = unsafe {
                    (
                        table.offsets[k - 2].get_value_unchecked(i),
                        table.offsets[k - 2].get_value_unchecked(i + (1 << (k - 1)))
                            + (1 << (k - 1)),
                    )
                };
                table.offsets[k - 1].set_value(
                    i,
                    (table
                        .data
                        .argmin(i + loffset as usize, i + roffset as usize)
                        - i),
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
        let (left_index, right_index) = unsafe {
            (
                self.offsets[k - 1].get_value_unchecked(l) + l,
                self.offsets[k - 1].get_value_unchecked(r + 1 - (1 << k)) + r + 1 - (1 << k),
            )
        };

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
    indices: BitFieldVec,
}

impl<'a, T: Ord> PackedIndexSparseTable<'a, T> {
    pub fn new(data: &'a [T], indices: &[usize]) -> Self {
        let n = indices.len();
        let k_max = n.ilog2() as usize;
        // Every stored index is one of `indices`, so the largest one bounds the width.
        let max_index = indices.iter().copied().max().unwrap_or(0);
        let width = (usize::BITS - max_index.leading_zeros()).max(1) as usize;
        let mut table = Self {
            n,
            k_max,
            data,
            indices: BitFieldVec::new(width, (k_max + 1) * n),
        };

        for i in 0..n {
            table.indices.set_value(i, indices[i]);
        }
        for k in 1..=table.k_max {
            for i in 0..table.n + 1 - (1 << k) {
                let (lindex, rindex) = unsafe {
                    (
                        table.indices.get_value_unchecked((k - 1) * table.n + i),
                        table
                            .indices
                            .get_value_unchecked((k - 1) * table.n + i + (1 << (k - 1))),
                    )
                };
                table
                    .indices
                    .set_value(k * n + i, data.argmin(lindex, rindex));
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
        let (left_index, right_index) = unsafe {
            (
                self.indices.get_value_unchecked(k * self.n + l),
                self.indices
                    .get_value_unchecked(k * self.n + r + 1 - (1 << k)),
            )
        };

        self.data.argmin(left_index, right_index)
    }
}

crate::rmq_family! {
    pub PackedIndexSparseTableFamily => PackedIndexSparseTable
}

pub struct PositionOffsetSparseTable<'a, T> {
    n: usize,
    k_max: usize,
    data: &'a [T],
    positions: BitFieldVec,
    offsets: Vec<BitFieldVec>,
}

impl<'a, T: Ord> PositionOffsetSparseTable<'a, T> {
    pub fn new(data: &'a [T], positions: &[usize]) -> Self {
        let n = positions.len();
        let k_max = if n == 0 { 0 } else { n.ilog2() as usize };

        let max_position = positions.iter().copied().max().unwrap_or(0);
        let width = (usize::BITS - max_position.leading_zeros()).max(1) as usize;
        let mut packed_positions = BitFieldVec::new(width, n);
        for i in 0..n {
            packed_positions.set_value(i, positions[i]);
        }
        let mut offsets: Vec<BitFieldVec> = (1..=k_max).map(|k| BitFieldVec::new(k, n)).collect();

        for k in 1..=k_max {
            let half = 1 << (k - 1);
            for i in 0..n + 1 - (1 << k) {
                let loffset = if k == 1 {
                    0
                } else {
                    unsafe { offsets[k - 2].get_value_unchecked(i) }
                };
                let roffset = half
                    + if k == 1 {
                        0
                    } else {
                        unsafe { offsets[k - 2].get_value_unchecked(i + half) }
                    };
                let lpos = positions[i + loffset];
                let rpos = positions[i + roffset];
                offsets[k - 1].set_value(
                    i,
                    if data[lpos] <= data[rpos] {
                        loffset
                    } else {
                        roffset
                    },
                );
            }
        }
        Self {
            n,
            k_max,
            data,
            positions: packed_positions,
            offsets,
        }
    }

    #[inline]
    fn position(&self, slot: usize) -> usize {
        unsafe { self.positions.get_value_unchecked(slot) }
    }
}

impl<'a, T> MemoryUsage for PositionOffsetSparseTable<'a, T> {
    fn memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.positions.memory_bytes()
            + self.offsets.len() * std::mem::size_of::<BitFieldVec>()
            + self.offsets.iter().map(|o| o.memory_bytes()).sum::<usize>()
    }
}

impl<'a, T: Ord> RMQ<'a, T> for PositionOffsetSparseTable<'a, T> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        let positions: Vec<usize> = (0..data.len()).collect();
        PositionOffsetSparseTable::new(data, &positions)
    }

    // Queries slots [l, r], returning a data position.
    fn rmq(&self, l: usize, r: usize) -> usize {
        if l == r {
            return self.position(l);
        }
        let k = (r - l + 1).ilog2() as usize;
        let s = r + 1 - (1 << k);
        let (left_offset, right_offset) = unsafe {
            (
                self.offsets[k - 1].get_value_unchecked(l),
                self.offsets[k - 1].get_value_unchecked(s),
            )
        };
        let left_index = self.position(l + left_offset);
        let right_index = self.position(s + right_offset);

        self.data.argmin(left_index, right_index)
    }
}

crate::rmq_family! {
    pub PositionOffsetSparseTableFamily => PositionOffsetSparseTable
}

pub struct ValueSparseTable<'a, T> {
    n: usize,
    k_max: usize,
    entries: Vec<MaybeUninit<(usize, T)>>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T: Ord + Copy> ValueSparseTable<'a, T> {
    pub fn new(data: &'a [T], indices: &[usize]) -> Self {
        let n = indices.len();
        let k_max = n.ilog2() as usize;
        let mut table = Self {
            n,
            k_max,
            entries: vec![MaybeUninit::uninit(); (k_max + 1) * n],
            _marker: PhantomData,
        };

        for i in 0..n {
            table.entries[i].write((indices[i], data[indices[i]]));
        }
        for k in 1..=table.k_max {
            for i in 0..table.n + 1 - (1 << k) {
                let lentry = unsafe { table.entries[(k - 1) * table.n + i].assume_init() };
                let rentry =
                    unsafe { table.entries[(k - 1) * table.n + i + (1 << (k - 1))].assume_init() };
                table.entries[k * n + i].write(if rentry.1 < lentry.1 { rentry } else { lentry });
            }
        }
        table
    }
}

impl<'a, T> MemoryUsage for ValueSparseTable<'a, T> {
    fn memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.entries.len() * std::mem::size_of::<MaybeUninit<(usize, T)>>()
    }
}

impl<'a, T: Ord + Copy> RMQ<'a, T> for ValueSparseTable<'a, T> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        let indices: Vec<usize> = (0..data.len()).collect();
        ValueSparseTable::new(data, &indices)
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        let k = (r - l + 1).ilog2() as usize;
        let left_entry = unsafe { self.entries[k * self.n + l].assume_init() };
        let right_entry = unsafe { self.entries[k * self.n + r + 1 - (1 << k)].assume_init() };

        if right_entry.1 < left_entry.1 {
            right_entry.0
        } else {
            left_entry.0
        }
    }
}

crate::rmq_family! {
    pub ValueSparseTableFamily => ValueSparseTable where T: Ord + Copy
}

/// Packs each entry as `(value << index_bits) | index` into a `u128`, so the minimum entry is
/// the integer minimum and ties resolve to the leftmost index. Field widths are taken from the
/// largest value and index present, and entries are stored at exactly that width in `u128` words.
pub struct PackedValueSparseTable<'a, T> {
    n: usize,
    k_max: usize,
    index_bits: u32,
    entries: BitFieldVec<Vec<u128>>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T: Copy + Into<u64>> PackedValueSparseTable<'a, T> {
    pub fn new(data: &'a [T], indices: &[usize]) -> Self {
        let n = indices.len();
        let k_max = n.ilog2() as usize;
        let bits = |x: u64| u64::BITS - x.leading_zeros();
        let index_bits = bits(indices.iter().copied().max().unwrap_or(0) as u64);
        let value_bits = bits(indices.iter().map(|&i| data[i].into()).max().unwrap_or(0));
        let mut table = Self {
            n,
            k_max,
            index_bits,
            entries: BitFieldVec::new((index_bits + value_bits).max(1) as usize, (k_max + 1) * n),
            _marker: PhantomData,
        };

        for i in 0..n {
            let value: u64 = data[indices[i]].into();
            table
                .entries
                .set_value(i, (value as u128) << index_bits | indices[i] as u128);
        }
        for k in 1..=table.k_max {
            for i in 0..table.n + 1 - (1 << k) {
                let (lentry, rentry) = unsafe {
                    (
                        table.entries.get_value_unchecked((k - 1) * table.n + i),
                        table
                            .entries
                            .get_value_unchecked((k - 1) * table.n + i + (1 << (k - 1))),
                    )
                };
                table.entries.set_value(k * n + i, lentry.min(rentry));
            }
        }
        table
    }
}

impl<'a, T> MemoryUsage for PackedValueSparseTable<'a, T> {
    fn memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.entries.memory_bytes()
    }
}

impl<'a, T: Copy + Into<u64>> RMQ<'a, T> for PackedValueSparseTable<'a, T> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        let indices: Vec<usize> = (0..data.len()).collect();
        PackedValueSparseTable::new(data, &indices)
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        let k = (r - l + 1).ilog2() as usize;
        let (left_entry, right_entry) = unsafe {
            (
                self.entries.get_value_unchecked(k * self.n + l),
                self.entries
                    .get_value_unchecked(k * self.n + r + 1 - (1 << k)),
            )
        };

        (left_entry.min(right_entry) & ((1 << self.index_bits) - 1)) as usize
    }
}

crate::rmq_family! {
    pub PackedValueSparseTableFamily => PackedValueSparseTable where T: Copy + Into<u64>
}
