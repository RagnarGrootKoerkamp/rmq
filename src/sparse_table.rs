use std::{marker::PhantomData, ptr::NonNull};

use mem_dbg::{FlatType, HashMap, MemDbg, MemDbgImpl, MemSize, SizeFlags};

use crate::base::{BuildableSlice, RMQ, TwoArgMin};

use sux::{
    bits::{BitFieldVec, BitFieldVecU},
    traits::{SliceByValue, SliceByValueMut, TryIntoUnaligned},
};

#[derive(MemSize, MemDbg)]
pub struct OffsetSparseTable<'a, T> {
    data: &'a [T],
    offsets: Vec<BitFieldVecU<Box<[usize]>>>,
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

#[derive(MemSize, MemDbg)]
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

#[derive(MemSize, MemDbg)]
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

#[derive(MemSize, MemDbg)]
pub struct SliceGetter<'a, T>(&'a [T]);
impl<'a, T: Copy> Getter<T> for SliceGetter<'a, T> {
    #[inline]
    fn get(&self, i: usize) -> T {
        self.0[i]
    }
}

#[derive(MemSize, MemDbg)]
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

#[derive(MemSize, MemDbg)]
pub struct ValueSparseTable<'a, T> {
    n: usize,
    k_max: usize,
    entries: Vec<u64>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T: Copy + Into<u32>> ValueSparseTable<'a, T> {
    pub fn new(data: &'a [T], indices: &[usize]) -> Self {
        let n = indices.len();
        let k_max = n.ilog2() as usize;
        let mut table = Self {
            n,
            k_max,
            entries: vec![0; (k_max + 1) * n],
            _marker: PhantomData,
        };

        for i in 0..n {
            table.entries[i] = (data[indices[i]].into() as u64) << 32 | (i as u64);
        }
        for k in 1..=table.k_max {
            for i in 0..table.n + 1 - (1 << k) {
                let lentry = table.entries[(k - 1) * table.n + i];
                let rentry = table.entries[(k - 1) * table.n + i + (1 << (k - 1))];
                table.entries[k * n + i] = lentry.min(rentry);
            }
        }
        table
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
        // SAFETY: for 0 <= l <= r < n, both slots start windows of size 2^k inside level k.
        let (left_entry, right_entry) = unsafe {
            (
                *self.entries.get_unchecked(k * self.n + l),
                *self.entries.get_unchecked(k * self.n + r + 1 - (1 << k)),
            )
        };

        ((left_entry.min(right_entry)) & ((1u64 << 32) - 1)) as usize
    }
}

crate::rmq_family! {
    pub ValueSparseTableFamily => ValueSparseTable where T: Copy + Into<u32>
}

/// Same layout as `ValueSparseTable`, with a batched query path that hides
/// memory latency: the batch is processed in chunks, and while chunk `c` is
/// answered the slots of chunk `c + 1` have already been computed and prefetched.
#[derive(MemSize, MemDbg)]
pub struct FastValueSparseTable<'a, T> {
    n: usize,
    k_max: usize,
    entries: Vec<u64>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T: Copy + Into<u32>> FastValueSparseTable<'a, T> {
    pub fn new(data: &'a [T], indices: &[usize]) -> Self {
        let n = indices.len();
        let k_max = n.ilog2() as usize;
        let mut table = Self {
            n,
            k_max,
            entries: vec![0; (k_max + 1) * n],
            _marker: PhantomData,
        };

        for i in 0..n {
            table.entries[i] = (data[indices[i]].into() as u64) << 32 | (i as u64);
        }
        for k in 1..=table.k_max {
            for i in 0..table.n + 1 - (1 << k) {
                let lentry = table.entries[(k - 1) * table.n + i];
                let rentry = table.entries[(k - 1) * table.n + i + (1 << (k - 1))];
                table.entries[k * n + i] = lentry.min(rentry);
            }
        }
        table
    }

    /// Pass 1 for one chunk: computes both table slots of every query (a loop
    /// without loads, so it vectorizes), then prefetches them. Returns the
    /// number of queries in the chunk.
    #[inline(always)]
    fn prepare_chunk(
        &self,
        queries: &[(usize, usize)],
        left: &mut [usize; FAST_VST_CHUNK],
        right: &mut [usize; FAST_VST_CHUNK],
    ) -> usize {
        let n = self.n;
        for ((&(l, r), a), b) in queries.iter().zip(left.iter_mut()).zip(right.iter_mut()) {
            let k = (r - l + 1).ilog2() as usize;
            *a = k * n + l;
            *b = k * n + r + 1 - (1 << k);
        }
        let len = queries.len();
        let base = self.entries.as_ptr();
        for i in 0..len {
            prefetch(base.wrapping_add(left[i]));
            prefetch(base.wrapping_add(right[i]));
        }
        len
    }
}

const FAST_VST_CHUNK: usize = 32;

#[inline(always)]
fn prefetch<T>(p: *const T) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::x86_64::_mm_prefetch(p as *const i8, core::arch::x86_64::_MM_HINT_T0)
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!(
            "prfm pldl1keep, [{0}]",
            in(reg) p,
            options(nostack, readonly, preserves_flags)
        )
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let _ = p;
}

impl<'a, T: Copy + Into<u32>> RMQ<'a, T> for FastValueSparseTable<'a, T> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        let indices: Vec<usize> = (0..data.len()).collect();
        FastValueSparseTable::new(data, &indices)
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        let k = (r - l + 1).ilog2() as usize;
        // SAFETY: for 0 <= l <= r < n, both slots start windows of size 2^k inside level k.
        let (left_entry, right_entry) = unsafe {
            (
                *self.entries.get_unchecked(k * self.n + l),
                *self.entries.get_unchecked(k * self.n + r + 1 - (1 << k)),
            )
        };

        (left_entry.min(right_entry) & INDEX_MASK) as usize
    }

    fn rmq_batch(&self, queries: &[(usize, usize)], answers: &mut [usize]) {
        let mut chunks = queries.chunks(FAST_VST_CHUNK);
        let (mut left, mut right) = ([0; FAST_VST_CHUNK], [0; FAST_VST_CHUNK]);
        let (mut next_left, mut next_right) = ([0; FAST_VST_CHUNK], [0; FAST_VST_CHUNK]);
        let mut len = match chunks.next() {
            Some(chunk) => self.prepare_chunk(chunk, &mut left, &mut right),
            None => return,
        };
        let base = self.entries.as_ptr();
        for out in answers.chunks_mut(FAST_VST_CHUNK) {
            // Pass 1 for the next chunk: its prefetches overlap this chunk's loads.
            let next_len = chunks.next().map_or(0, |chunk| {
                self.prepare_chunk(chunk, &mut next_left, &mut next_right)
            });
            // Pass 2 for this chunk: loads, prefetched one chunk ago.
            for ((ans, &a), &b) in out.iter_mut().zip(&left[..len]).zip(&right[..len]) {
                // SAFETY: valid queries give slots inside the table, as in `rmq`.
                let entry = unsafe { (*base.add(a)).min(*base.add(b)) };
                *ans = (entry & INDEX_MASK) as usize;
            }
            (left, next_left) = (next_left, left);
            (right, next_right) = (next_right, right);
            len = next_len;
        }
    }
}

crate::rmq_family! {
    pub FastValueSparseTableFamily => FastValueSparseTable where T: Copy + Into<u32>
}

#[derive(MemSize, MemDbg)]
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

/// An owned heap slice held through a raw pointer rather than a `Box`, so the
/// borrow `OwningRMQWrapper::inner` takes of it stays valid when the wrapper
/// moves (a `Box` would assert unique access on every move).
pub struct RawBoxedSlice<V>(NonNull<[V]>);

impl<V> RawBoxedSlice<V> {
    fn new(values: Box<[V]>) -> Self {
        Self(NonNull::from(Box::leak(values)))
    }

    /// # Safety
    /// The returned slice must not outlive `self`.
    unsafe fn as_slice<'b>(&self) -> &'b [V] {
        unsafe { self.0.as_ref() }
    }
}

impl<V> Drop for RawBoxedSlice<V> {
    fn drop(&mut self) {
        drop(unsafe { Box::from_raw(self.0.as_ptr()) });
    }
}

impl<V> FlatType for RawBoxedSlice<V> {
    type Flat = mem_dbg::False;
}

impl<V: MemSize + FlatType> MemSize for RawBoxedSlice<V> {
    fn mem_size_rec(&self, flags: SizeFlags, refs: &mut HashMap<usize, usize>) -> usize {
        // The pointer itself plus the owned heap block, like `Box<[V]>`.
        size_of::<Self>() + unsafe { self.as_slice() }.mem_size_rec(flags, refs)
    }
}

impl<V: MemSize + FlatType> MemDbgImpl for RawBoxedSlice<V> {}

#[derive(MemSize, MemDbg)]
pub struct OwningRMQWrapper<'a, V, R> {
    positions: BitFieldVec,
    // Fix: Incorrect lifetime, `inner` actually borrows from `values`.
    // Fields drop in declaration order, so `inner` is dropped before `values`.
    inner: R,
    values: RawBoxedSlice<V>,
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
        let values = RawBoxedSlice::new(positions.iter().map(|&i| data[i]).collect());
        // SAFETY: `inner` is stored next to `values` and dropped before it.
        let inner = R::new(unsafe { values.as_slice() });

        Self {
            positions: packed_positions,
            inner,
            values,
            _marker: PhantomData,
        }
    }

    #[inline]
    fn position(&self, slot: usize) -> usize {
        unsafe { self.positions.get_value_unchecked(slot) }
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

/// A sparse table of `value << 32 | index` entries, like `ValueSparseTable`,
/// that only stores every other level: windows of size 4^j instead of 2^k.
/// A query whose length has an odd log uses two windows from its own level, an
/// even log uses four windows from the level below.
#[derive(MemSize, MemDbg)]
pub struct FanoutValueSparseTable<'a, T> {
    n: usize,
    // levels[j] holds windows of size 2^(2j), transposed (see `transposed_slot`)
    levels: Vec<Box<[u64]>>,
    _marker: PhantomData<&'a T>,
}

// Level k (window 2^k) is stored transposed with stride s = 2^k, i.e.
// column-major by i mod s, so that entries i and i + s are adjacent.
#[inline(always)]
fn transposed_slot(i: usize, k: usize, n: usize) -> usize {
    let rows = (n + (1 << k) - 1) >> k;
    (i & ((1 << k) - 1)) * rows + (i >> k)
}

const INDEX_MASK: u64 = (1u64 << 32) - 1;

impl<'a, T: Copy + Into<u32>> FanoutValueSparseTable<'a, T> {
    pub fn new(data: &'a [T], indices: &[usize]) -> Self {
        let n = indices.len();
        let k_max = n.ilog2() as usize;
        let level_len = |k: usize| ((n + (1 << k) - 1) >> k) << k;

        let mut levels: Vec<Box<[u64]>> = Vec::with_capacity(k_max / 2 + 1);
        // Level 0 has stride 1, so its transposed layout is the identity.
        levels.push(
            (0..n)
                .map(|i| (data[indices[i]].into() as u64) << 32 | (i as u64))
                .collect(),
        );
        for k in (2..=k_max).step_by(2) {
            let prev = &levels[(k - 2) / 2];
            let mut level = vec![0u64; level_len(k)].into_boxed_slice();
            for i in 0..n + 1 - (1 << k) {
                // With s = 2^(k-2), i, i + s, i + 2s, i + 3s are adjacent in level k - 2
                let p = transposed_slot(i, k - 2, n);
                let lmin = prev[p].min(prev[p + 1]);
                let rmin = prev[p + 2].min(prev[p + 3]);
                level[transposed_slot(i, k, n)] = lmin.min(rmin);
            }
            levels.push(level);
        }
        Self {
            n,
            levels,
            _marker: PhantomData,
        }
    }
}

impl<'a, T: Copy + Into<u32>> RMQ<'a, T> for FanoutValueSparseTable<'a, T> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized,
    {
        let indices: Vec<usize> = (0..data.len()).collect();
        FanoutValueSparseTable::new(data, &indices)
    }

    // Queries [l, r]
    fn rmq(&self, l: usize, r: usize) -> usize {
        let n = self.n;
        let k = (r - l + 1).ilog2() as usize;
        let entry = if k % 2 == 0 {
            // Two overlapping windows of size 2^k
            let level = &self.levels[k / 2];
            let j = r + 1 - (1 << k);
            // SAFETY: l and j start windows of size 2^k inside [0, n).
            unsafe {
                let left = *level.get_unchecked(transposed_slot(l, k, n));
                let right = *level.get_unchecked(transposed_slot(j, k, n));
                left.min(right)
            }
        } else {
            // Four overlapping windows of size s = 2^(k-1), since 2s <= r - l + 1 < 4s.
            // l, l + s and j, j + s are adjacent pairs in the transposed level k - 1
            let s = 1 << (k - 1);
            let level = &self.levels[(k - 1) / 2];
            let j = r + 1 - 2 * s;
            let p = transposed_slot(l, k - 1, n);
            let q = transposed_slot(j, k - 1, n);
            // SAFETY: l, l + s, j, j + s start windows of size s inside [0, n).
            unsafe {
                let lmin = (*level.get_unchecked(p)).min(*level.get_unchecked(p + 1));
                let rmin = (*level.get_unchecked(q)).min(*level.get_unchecked(q + 1));
                lmin.min(rmin)
            }
        };

        (entry & INDEX_MASK) as usize
    }
}

crate::rmq_family! {
    pub FanoutValueSparseTableFamily => FanoutValueSparseTable where T: Copy + Into<u32>
}
