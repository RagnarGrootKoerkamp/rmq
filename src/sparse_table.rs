use std::{
    marker::PhantomData,
    mem::{ManuallyDrop, MaybeUninit},
};

use crate::base::{BuildableSlice, MemoryUsage, RMQ, TwoArgMin};

use sux::{
    bits::{BitFieldVec, BitFieldVecU},
    traits::{SliceByValue, SliceByValueMut, TryIntoUnaligned},
};

pub struct OffsetSparseTable<'a, T> {
    data: &'a [T],
    offsets: Vec<BitFieldVecU<Box<[usize]>>>,
}

impl<'a, T> MemoryUsage for OffsetSparseTable<'a, T> {
    fn memory_allocated(&self) -> usize {
        self.offsets.len() * std::mem::size_of::<BitFieldVecU<Box<[usize]>>>()
            + self
                .offsets
                .iter()
                .map(|o| o.memory_allocated())
                .sum::<usize>()
    }
}

impl<'a, T: Ord> RMQ<'a, T> for OffsetSparseTable<'a, T> {
    fn new(data: &'a [T]) -> Self {
        let n = data.len();
        let k_max = n.ilog2() as usize;
        let mut offsets: Vec<BitFieldVec<Box<[usize]>>> =
            (1..=k_max).map(|k| BitFieldVec::new_padded(k, n)).collect();
        for i in 0..n - 1 {
            offsets[0].set_value(i, data.argmin(i, i + 1) - i);
        }
        for k in 2..=k_max {
            for i in 0..n + 1 - (1 << k) {
                let (loffset, roffset) = unsafe {
                    (
                        offsets[k - 2].get_value_unchecked(i),
                        offsets[k - 2].get_value_unchecked(i + (1 << (k - 1))) + (1 << (k - 1)),
                    )
                };
                offsets[k - 1].set_value(i, data.argmin(i + loffset, i + roffset) - i);
            }
        }
        OffsetSparseTable {
            data,
            offsets: offsets
                .into_iter()
                .map(|x| x.try_into_unaligned().unwrap())
                .collect(),
        }
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

pub struct IndexST<'a, T, C = Vec<usize>> {
    n: usize,
    k_max: usize,
    data: &'a [T],
    indices: C,
}

impl<'a, T: Ord, C: BuildableSlice<Value = usize>> IndexST<'a, T, C> {
    pub fn new(data: &'a [T], indices: &[usize]) -> Self {
        let n = indices.len();
        let k_max = n.ilog2() as usize;
        let bit_width = (usize::BITS - (data.len() - 1).leading_zeros()).max(1) as usize;
        let mut table = C::new_builder((k_max + 1) * n, bit_width);

        for i in 0..n {
            table.set_value(i, indices[i]);
        }
        for k in 1..=k_max {
            for i in 0..n + 1 - (1 << k) {
                let (lindex, rindex) = unsafe {
                    (
                        table.get_value_unchecked((k - 1) * n + i),
                        table.get_value_unchecked((k - 1) * n + i + (1 << (k - 1))),
                    )
                };
                unsafe {
                    table.set_value_unchecked(k * n + i, data.argmin(lindex, rindex));
                }
            }
        }
        Self {
            n,
            k_max,
            data,
            indices: C::build(table),
        }
    }
}

impl<'a, T, C: MemoryUsage> MemoryUsage for IndexST<'a, T, C> {
    fn memory_allocated(&self) -> usize {
        self.indices.memory_allocated()
    }
}

impl<'a, T: Ord, C: BuildableSlice<Value = usize>> RMQ<'a, T> for IndexST<'a, T, C> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        let indices: Vec<usize> = (0..data.len()).collect();
        IndexST::new(data, &indices)
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        let k = (r - l + 1).ilog2() as usize;
        let (lindex, rindex) = unsafe {
            (
                self.indices.get_value_unchecked(k * self.n + l),
                self.indices
                    .get_value_unchecked(k * self.n + r + 1 - (1 << k)),
            )
        };

        self.data.argmin(lindex, rindex)
    }
}

crate::rmq_family! {
    pub IndexSparseTableFamily => IndexST
}

type PackedIndexST<'a, T> = IndexST<'a, T, BitFieldVecU<Box<[usize]>>>;

crate::rmq_family! {
    pub PackedIndexSparseTableFamily => PackedIndexST
}

pub struct PositionOffsetSparseTable<'a, T> {
    data: &'a [T],
    positions: BitFieldVecU<Box<[usize]>>,
    offsets: Vec<BitFieldVecU<Box<[usize]>>>,
}

impl<'a, T: Ord> PositionOffsetSparseTable<'a, T> {
    pub fn new(data: &'a [T], positions: &[usize]) -> Self {
        let n = positions.len();
        let k_max = if n == 0 { 0 } else { n.ilog2() as usize };

        let max_position = positions.iter().copied().max().unwrap_or(0);
        let width = (usize::BITS - max_position.leading_zeros()).max(1) as usize;
        let mut packed_positions = BitFieldVec::new_padded(width, n);
        for i in 0..n {
            packed_positions.set_value(i, positions[i]);
        }
        let mut offsets: Vec<BitFieldVec<Box<[usize]>>> =
            (1..=k_max).map(|k| BitFieldVec::new_padded(k, n)).collect();

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
            data,
            positions: packed_positions.try_into_unaligned().unwrap(),
            offsets: offsets
                .into_iter()
                .map(|x| x.try_into_unaligned().unwrap())
                .collect(),
        }
    }

    #[inline]
    fn position(&self, slot: usize) -> usize {
        unsafe { self.positions.get_value_unchecked(slot) }
    }
}

impl<'a, T> MemoryUsage for PositionOffsetSparseTable<'a, T> {
    fn memory_allocated(&self) -> usize {
        self.positions.memory_allocated()
            + self.offsets.len() * std::mem::size_of::<BitFieldVecU<Box<[usize]>>>()
            + self
                .offsets
                .iter()
                .map(|o| o.memory_allocated())
                .sum::<usize>()
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

pub trait Getter<T> {
    fn get(&self, i: usize) -> T;
}
impl<T, F: Fn(usize) -> T> Getter<T> for F {
    #[inline]
    fn get(&self, i: usize) -> T {
        self(i)
    }
}

pub struct SliceGetter<'a, T>(&'a [T]);
impl<'a, T: Copy> Getter<T> for SliceGetter<'a, T> {
    #[inline]
    fn get(&self, i: usize) -> T {
        self.0[i]
    }
}

pub struct CustomOffsetST<'a, T, U> {
    pub getter: U,
    offsets: Vec<BitFieldVec>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T: Ord, U: Getter<T>> CustomOffsetST<'a, T, U> {
    pub fn new(n: usize, getter: U) -> Self {
        let k_max = if n == 0 { 0 } else { n.ilog2() as usize };

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
                let lpos = i + loffset;
                let rpos = i + roffset;
                offsets[k - 1].set_value(
                    i,
                    if getter.get(lpos) <= getter.get(rpos) {
                        loffset
                    } else {
                        roffset
                    },
                );
            }
        }
        Self {
            getter,
            offsets,
            _marker: PhantomData,
        }
    }

    // Queries slots [l, r], returning a data position.
    pub fn rmq(&self, l: usize, r: usize) -> usize {
        if l == r {
            return l;
        }
        let k = (r - l + 1).ilog2() as usize;
        let s = r + 1 - (1 << k);
        let (left_offset, right_offset) = unsafe {
            (
                self.offsets[k - 1].get_value_unchecked(l),
                self.offsets[k - 1].get_value_unchecked(s),
            )
        };
        let left_index = l + left_offset;
        let right_index = s + right_offset;

        if self.getter.get(left_index) <= self.getter.get(right_index) {
            left_index
        } else {
            right_index
        }
    }
}

impl<'a, T, U> MemoryUsage for CustomOffsetST<'a, T, U> {
    /// Offset tables only; whatever the getter owns is counted by the caller.
    fn memory_allocated(&self) -> usize {
        self.offsets.len() * std::mem::size_of::<BitFieldVec>()
            + self
                .offsets
                .iter()
                .map(|o| o.memory_allocated())
                .sum::<usize>()
    }
}

pub type SliceOffsetST<'a, T> = CustomOffsetST<'a, T, SliceGetter<'a, T>>;

impl<'a, T: Ord + Copy> RMQ<'a, T> for SliceOffsetST<'a, T> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        CustomOffsetST::new(data.len(), SliceGetter(data))
    }

    fn rmq(&self, l: usize, r: usize) -> usize {
        CustomOffsetST::rmq(self, l, r)
    }
}

crate::rmq_family! {
    pub CustomOffsetSparseTableFamily => SliceOffsetST where T: Ord + Copy
}

pub struct ValueSparseTable<'a, T> {
    n: usize,
    k_max: usize,
    entries: Vec<MaybeUninit<u64>>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T: Copy + Into<u32>> ValueSparseTable<'a, T> {
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
            table.entries[i].write((data[indices[i]].into() as u64) << 32 | (i as u64));
        }
        for k in 1..=table.k_max {
            for i in 0..table.n + 1 - (1 << k) {
                let lentry = unsafe { table.entries[(k - 1) * table.n + i].assume_init() };
                let rentry =
                    unsafe { table.entries[(k - 1) * table.n + i + (1 << (k - 1))].assume_init() };
                table.entries[k * n + i].write(lentry.min(rentry));
            }
        }
        table
    }
}

impl<'a, T> MemoryUsage for ValueSparseTable<'a, T> {
    fn memory_allocated(&self) -> usize {
        self.entries.len() * std::mem::size_of::<MaybeUninit<(usize, T)>>()
    }
}

impl<'a, T: Copy + Into<u32>> RMQ<'a, T> for ValueSparseTable<'a, T> {
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

        ((left_entry.min(right_entry)) & ((1u64 << 32) - 1)) as usize
    }
}

crate::rmq_family! {
    pub ValueSparseTableFamily => ValueSparseTable where T: Copy + Into<u32>
}

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
    fn memory_allocated(&self) -> usize {
        self.entries.memory_allocated()
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

pub struct OwningRMQWrapper<'a, V, R> {
    positions: BitFieldVec,
    // Fix: Incorrect lifetime, `inner` actually borrows from `values`
    inner: ManuallyDrop<R>,
    values: *mut [V],
    _marker: PhantomData<&'a V>,
}

impl<'a, V: Copy, R: RMQ<'a, V>> OwningRMQWrapper<'a, V, R> {
    pub fn new(data: &'a [V], positions: &[usize]) -> Self {
        let n = positions.len();
        let max_position = positions.iter().copied().max().unwrap_or(0);
        let width = (usize::BITS - max_position.leading_zeros()).max(1) as usize;
        let mut packed_positions = BitFieldVec::new(width, n);
        for i in 0..n {
            packed_positions.set_value(i, positions[i]);
        }
        let values: *mut [V] =
            Box::into_raw(positions.iter().map(|&i| data[i]).collect::<Box<[V]>>());
        let inner = R::new(unsafe { &*values });

        Self {
            positions: packed_positions,
            inner: ManuallyDrop::new(inner),
            values,
            _marker: PhantomData,
        }
    }

    #[inline]
    fn position(&self, slot: usize) -> usize {
        unsafe { self.positions.get_value_unchecked(slot) }
    }
}

impl<'a, V, R> Drop for OwningRMQWrapper<'a, V, R> {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.inner); // drop the borrower first
            drop(Box::from_raw(self.values)); // then the owner
        }
    }
}

impl<'a, V, R: MemoryUsage> MemoryUsage for OwningRMQWrapper<'a, V, R> {
    fn memory_allocated(&self) -> usize {
        self.positions.memory_allocated()
            + self.inner.memory_allocated()
            + self.values.len() * std::mem::size_of::<V>()
    }
}

impl<'a, V: Copy, R: RMQ<'a, V>> RMQ<'a, V> for OwningRMQWrapper<'a, V, R> {
    fn new(data: &'a [V]) -> Self
    where
        Self: Sized,
    {
        let positions: Vec<usize> = (0..data.len()).collect();
        Self::new(data, &positions)
    }

    fn rmq(&self, l: usize, r: usize) -> usize {
        self.position(self.inner.rmq(l, r))
    }
}

pub type OwningOffsetSparseTable<'a, T> = OwningRMQWrapper<'a, T, OffsetSparseTable<'a, T>>;

crate::rmq_family! {
    pub OwningOffsetSparseTableFamily => OwningOffsetSparseTable where T: Ord + Copy
}

pub struct OffsetSparseTable4<'a, T> {
    data: &'a [T],
    offsets: Vec<BitFieldVecU<Box<[usize]>>>,
}

// Each level k (window 2^k) is stored transposed with stride s = 2^k, i.e.
// column-major by i mod s, so that entries i and i + s are adjacent.
#[inline(always)]
fn transposed_slot(i: usize, k: usize, n: usize) -> usize {
    let rows = (n + (1 << k) - 1) >> k;
    (i & ((1 << k) - 1)) * rows + (i >> k)
}

impl<'a, T> MemoryUsage for OffsetSparseTable4<'a, T> {
    fn memory_allocated(&self) -> usize {
        self.offsets.len() * std::mem::size_of::<BitFieldVecU<Box<[usize]>>>()
            + self
                .offsets
                .iter()
                .map(|o| o.memory_allocated())
                .sum::<usize>()
    }
}

impl<'a, T: Ord> RMQ<'a, T> for OffsetSparseTable4<'a, T> {
    fn new(data: &'a [T]) -> Self {
        let n = data.len();
        let k_max = n.ilog2() as usize;
        let mut offsets: Vec<BitFieldVec<Box<[usize]>>> = (1..=k_max)
            .step_by(2)
            .map(|k| BitFieldVec::new_padded(k, ((n + (1 << k) - 1) >> k) << k))
            .collect();
        for i in 0..n - 1 {
            offsets[0].set_value(transposed_slot(i, 1, n), data.argmin(i, i + 1) - i);
        }
        for k in (3..=k_max).step_by(2) {
            let s = 1 << (k - 2);
            for i in 0..n + 1 - (1 << k) {
                // i, i + s, i + 2s, i + 3s are adjacent in level k - 2
                let p = transposed_slot(i, k - 2, n);
                let (l1offset, l2offset, l3offset, l4offset) = unsafe {
                    (
                        offsets[(k - 3) / 2].get_value_unchecked(p),
                        offsets[(k - 3) / 2].get_value_unchecked(p + 1) + s,
                        offsets[(k - 3) / 2].get_value_unchecked(p + 2) + 2 * s,
                        offsets[(k - 3) / 2].get_value_unchecked(p + 3) + 3 * s,
                    )
                };
                let lindex = data.argmin(i + l1offset, i + l2offset);
                let rindex = data.argmin(i + l3offset, i + l4offset);
                offsets[(k - 1) / 2]
                    .set_value(transposed_slot(i, k, n), data.argmin(lindex, rindex) - i);
            }
        }
        OffsetSparseTable4 {
            data,
            offsets: offsets
                .into_iter()
                .map(|x| x.try_into_unaligned().unwrap())
                .collect(),
        }
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        if l == r {
            return l;
        }

        let n = self.data.len();
        let k = (r - l + 1).ilog2() as usize;
        if k % 2 == 1 {
            // Two overlapping windows of size 2^k
            let offsets = &self.offsets[(k - 1) / 2];
            let j = r + 1 - (1 << k);
            let (left_index, right_index) = unsafe {
                (
                    offsets.get_value_unchecked(transposed_slot(l, k, n)) + l,
                    offsets.get_value_unchecked(transposed_slot(j, k, n)) + j,
                )
            };
            self.data.argmin(left_index, right_index)
        } else {
            // Four overlapping windows of size s = 2^(k-1), since 2s <= r - l + 1 < 4s.
            // l, l + s and j, j + s are adjacent pairs in the transposed level k - 1
            let s = 1 << (k - 1);
            let offsets = &self.offsets[(k - 2) / 2];
            let j = r + 1 - 2 * s;
            let p = transposed_slot(l, k - 1, n);
            let q = transposed_slot(j, k - 1, n);
            let (i1, i2, i3, i4) = unsafe {
                (
                    offsets.get_value_unchecked(p) + l,
                    offsets.get_value_unchecked(p + 1) + l + s,
                    offsets.get_value_unchecked(q) + j,
                    offsets.get_value_unchecked(q + 1) + j + s,
                )
            };
            let left_index = self.data.argmin(i1, i2);
            let right_index = self.data.argmin(i3, i4);
            self.data.argmin(left_index, right_index)
        }
    }
}

crate::rmq_family! {
    pub OffsetSparseTable4Family => OffsetSparseTable4
}
