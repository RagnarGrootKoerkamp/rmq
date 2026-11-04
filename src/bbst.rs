//! Bindings to the C++ Block-based Sparse Table (BbST) of Kowalski and Grabowski,
//! vendored in `BbST/` and compiled by `build.rs` through `ffi/bbst_shim.cpp`.
//!
//! The C++ structure borrows the input array, like the native structures here.
//! It indexes with 32-bit integers, so inputs must have fewer than `u32::MAX` elements.

use std::{
    ffi::{c_int, c_void},
    marker::PhantomData,
    ptr::NonNull,
};

use mem_dbg::{FlatType, HashMap, MemDbgImpl, MemSize, SizeFlags};

use crate::base::RMQ;

type NewFn<T> = unsafe extern "C" fn(*const T, u32, c_int) -> *mut c_void;
type NewMiniFn<T> = unsafe extern "C" fn(*const T, u32, c_int, c_int) -> *mut c_void;

#[doc(hidden)]
pub struct Ops<New> {
    new: New,
    rmq: unsafe extern "C" fn(*mut c_void, u32, u32) -> u32,
    mem: unsafe extern "C" fn(*mut c_void) -> usize,
    free: unsafe extern "C" fn(*mut c_void),
}

/// Value types the C++ library is compiled for.
pub trait BbSTValue: Ord + Sized {
    #[doc(hidden)]
    const BBST: Ops<NewFn<Self>>;
    #[doc(hidden)]
    const BBST2: Ops<NewMiniFn<Self>>;
}

macro_rules! bbst_value {
    ($t:ty, $new:ident, $rmq:ident, $mem:ident, $free:ident,
     $new2:ident, $rmq2:ident, $mem2:ident, $free2:ident) => {
        unsafe extern "C" {
            fn $new(data: *const $t, n: u32, k_exp: c_int) -> *mut c_void;
            fn $rmq(p: *mut c_void, l: u32, r: u32) -> u32;
            fn $mem(p: *mut c_void) -> usize;
            fn $free(p: *mut c_void);
            fn $new2(data: *const $t, n: u32, k_exp: c_int, mini_k_exp: c_int) -> *mut c_void;
            fn $rmq2(p: *mut c_void, l: u32, r: u32) -> u32;
            fn $mem2(p: *mut c_void) -> usize;
            fn $free2(p: *mut c_void);
        }

        impl BbSTValue for $t {
            const BBST: Ops<NewFn<Self>> = Ops {
                new: $new,
                rmq: $rmq,
                mem: $mem,
                free: $free,
            };
            const BBST2: Ops<NewMiniFn<Self>> = Ops {
                new: $new2,
                rmq: $rmq2,
                mem: $mem2,
                free: $free2,
            };
        }
    };
}

bbst_value!(
    u8,
    bbst_u8_new,
    bbst_u8_rmq,
    bbst_u8_mem,
    bbst_u8_free,
    bbst2_u8_new,
    bbst2_u8_rmq,
    bbst2_u8_mem,
    bbst2_u8_free
);
bbst_value!(
    u16,
    bbst_u16_new,
    bbst_u16_rmq,
    bbst_u16_mem,
    bbst_u16_free,
    bbst2_u16_new,
    bbst2_u16_rmq,
    bbst2_u16_mem,
    bbst2_u16_free
);
bbst_value!(
    u32,
    bbst_u32_new,
    bbst_u32_rmq,
    bbst_u32_mem,
    bbst_u32_free,
    bbst2_u32_new,
    bbst2_u32_rmq,
    bbst2_u32_mem,
    bbst2_u32_free
);
bbst_value!(
    u64,
    bbst_u64_new,
    bbst_u64_rmq,
    bbst_u64_mem,
    bbst_u64_free,
    bbst2_u64_new,
    bbst2_u64_rmq,
    bbst2_u64_mem,
    bbst2_u64_free
);
bbst_value!(
    i32,
    bbst_i32_new,
    bbst_i32_rmq,
    bbst_i32_mem,
    bbst_i32_free,
    bbst2_i32_new,
    bbst2_i32_rmq,
    bbst2_i32_mem,
    bbst2_i32_free
);

fn check_len(n: usize) -> u32 {
    assert!(n > 0, "BbST requires non-empty input");
    // u32::MAX is the library's "not found" sentinel, so it can't be an index.
    u32::try_from(n)
        .ok()
        .filter(|&n| n < u32::MAX)
        .expect("BbST supports fewer than u32::MAX elements")
}

/// BbST: sparse table over the minima of blocks of `2^K_EXP` elements; the
/// query scans the (at most two) partial boundary blocks.
pub struct BbST<'a, T: BbSTValue, const K_EXP: u32> {
    ptr: NonNull<c_void>,
    _data: PhantomData<&'a [T]>,
}

impl<'a, T: BbSTValue, const K_EXP: u32> RMQ<'a, T> for BbST<'a, T, K_EXP> {
    fn new(data: &'a [T]) -> Self {
        const { assert!(K_EXP <= 30, "K_EXP must be at most 30") };
        let n = check_len(data.len());
        let ptr = unsafe { (T::BBST.new)(data.as_ptr(), n, K_EXP as c_int) };
        BbST {
            ptr: NonNull::new(ptr).unwrap(),
            _data: PhantomData,
        }
    }

    // Queries [l, r]
    #[inline]
    fn rmq(&self, l: usize, r: usize) -> usize {
        unsafe { (T::BBST.rmq)(self.ptr.as_ptr(), l as u32, r as u32) as usize }
    }
}

// The C++ structure is opaque, so these are written by hand: the struct itself
// plus what the library reports. `mem_dbg` prints the total without a breakdown.
impl<'a, T: BbSTValue, const K_EXP: u32> FlatType for BbST<'a, T, K_EXP> {
    type Flat = mem_dbg::False;
}

impl<'a, T: BbSTValue, const K_EXP: u32> MemSize for BbST<'a, T, K_EXP> {
    fn mem_size_rec(&self, _flags: SizeFlags, _refs: &mut HashMap<usize, usize>) -> usize {
        size_of::<Self>() + unsafe { (T::BBST.mem)(self.ptr.as_ptr()) }
    }
}

impl<'a, T: BbSTValue, const K_EXP: u32> MemDbgImpl for BbST<'a, T, K_EXP> {}

impl<'a, T: BbSTValue, const K_EXP: u32> Drop for BbST<'a, T, K_EXP> {
    fn drop(&mut self) {
        unsafe { (T::BBST.free)(self.ptr.as_ptr()) }
    }
}

crate::rmq_family! {
    pub BbSTFamily<const K_EXP: u32> => BbST where T: BbSTValue
}

/// BbST2: BbST that also stores the in-block offset of the minimum of every
/// mini-block of `2^MINI_K_EXP` elements, so boundary blocks are scanned by
/// mini-block minima instead of element by element.
pub struct BbST2<'a, T: BbSTValue, const K_EXP: u32, const MINI_K_EXP: u32> {
    ptr: NonNull<c_void>,
    _data: PhantomData<&'a [T]>,
}

impl<'a, T: BbSTValue, const K_EXP: u32, const MINI_K_EXP: u32> RMQ<'a, T>
    for BbST2<'a, T, K_EXP, MINI_K_EXP>
{
    fn new(data: &'a [T]) -> Self {
        const {
            assert!(K_EXP <= 30, "K_EXP must be at most 30");
            // Mini-block offsets are stored as u8.
            assert!(MINI_K_EXP <= 8, "MINI_K_EXP must be at most 8");
            assert!(MINI_K_EXP <= K_EXP, "MINI_K_EXP must not exceed K_EXP");
        };
        let n = check_len(data.len());
        let ptr = unsafe { (T::BBST2.new)(data.as_ptr(), n, K_EXP as c_int, MINI_K_EXP as c_int) };
        BbST2 {
            ptr: NonNull::new(ptr).unwrap(),
            _data: PhantomData,
        }
    }

    // Queries [l, r]
    #[inline]
    fn rmq(&self, l: usize, r: usize) -> usize {
        unsafe { (T::BBST2.rmq)(self.ptr.as_ptr(), l as u32, r as u32) as usize }
    }
}

impl<'a, T: BbSTValue, const K_EXP: u32, const MINI_K_EXP: u32> FlatType
    for BbST2<'a, T, K_EXP, MINI_K_EXP>
{
    type Flat = mem_dbg::False;
}

impl<'a, T: BbSTValue, const K_EXP: u32, const MINI_K_EXP: u32> MemSize
    for BbST2<'a, T, K_EXP, MINI_K_EXP>
{
    fn mem_size_rec(&self, _flags: SizeFlags, _refs: &mut HashMap<usize, usize>) -> usize {
        size_of::<Self>() + unsafe { (T::BBST2.mem)(self.ptr.as_ptr()) }
    }
}

impl<'a, T: BbSTValue, const K_EXP: u32, const MINI_K_EXP: u32> MemDbgImpl
    for BbST2<'a, T, K_EXP, MINI_K_EXP>
{
}

impl<'a, T: BbSTValue, const K_EXP: u32, const MINI_K_EXP: u32> Drop
    for BbST2<'a, T, K_EXP, MINI_K_EXP>
{
    fn drop(&mut self) {
        unsafe { (T::BBST2.free)(self.ptr.as_ptr()) }
    }
}

crate::rmq_family! {
    pub BbST2Family<const K_EXP: u32, const MINI_K_EXP: u32> => BbST2 where T: BbSTValue
}
