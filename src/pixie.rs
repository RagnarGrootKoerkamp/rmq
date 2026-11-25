//! Bindings to the C++ `HybridBTree` of Malkovsky's pixie library, vendored in
//! `pixie/` and compiled by `build.rs` through `ffi/pixie_shim.cpp`.
//!
//! The C++ structure borrows the input array, like the native structures here.
//!
//! Only the leaf sizes upstream supports are compiled: 248 and 496 (prefix/suffix
//! mask leaves, 496 being upstream's default) and 252 (balanced-parentheses
//! leaves). Any other value is a compile-time error.

use std::{ffi::c_void, marker::PhantomData, ptr::NonNull};

use mem_dbg::{FlatType, HashMap, MemDbgImpl, MemSize, SizeFlags};

use crate::base::RMQ;

#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct Ops<T> {
    new: unsafe extern "C" fn(*const T, usize) -> *mut c_void,
    rmq: unsafe extern "C" fn(*mut c_void, usize, usize) -> usize,
    mem: unsafe extern "C" fn(*mut c_void) -> usize,
    free: unsafe extern "C" fn(*mut c_void),
}

/// Value types the C++ library is compiled for.
pub trait PixieValue: Ord + Copy + 'static {
    /// `(leaf size, functions)` for every compiled leaf size.
    #[doc(hidden)]
    const OPS: &'static [(usize, Ops<Self>)];
}

macro_rules! pixie_value {
    ($t:ident, [$($leaf:literal),*]) => {
        impl PixieValue for $t {
            const OPS: &'static [(usize, Ops<Self>)] = &[$(($leaf, {
                unsafe extern "C" {
                    #[link_name = concat!("pixie_hybrid_", stringify!($t), "_", $leaf, "_new")]
                    fn new(data: *const $t, n: usize) -> *mut c_void;
                    #[link_name = concat!("pixie_hybrid_", stringify!($t), "_", $leaf, "_rmq")]
                    fn rmq(p: *mut c_void, l: usize, r: usize) -> usize;
                    #[link_name = concat!("pixie_hybrid_", stringify!($t), "_", $leaf, "_mem")]
                    fn mem(p: *mut c_void) -> usize;
                    #[link_name = concat!("pixie_hybrid_", stringify!($t), "_", $leaf, "_free")]
                    fn free(p: *mut c_void);
                }
                Ops { new, rmq, mem, free }
            })),*];
        }
    };
}

// Must match the instances in ffi/pixie_shim.cpp and TYPES in build.rs.
pixie_value!(u8, [248, 252, 496]);
pixie_value!(u16, [248, 252, 496]);
pixie_value!(u32, [248, 252, 496]);
pixie_value!(u64, [248, 252, 496]);
pixie_value!(i32, [248, 252, 496]);

const fn ops<T: PixieValue>(leaf_size: usize) -> Ops<T> {
    let mut i = 0;
    while i < T::OPS.len() {
        if T::OPS[i].0 == leaf_size {
            return T::OPS[i].1;
        }
        i += 1;
    }
    panic!("HybridBTree supports LEAF_SIZE 248, 252 (BP leaves) or 496");
}

/// pixie's HybridBTree: leaves of `LEAF_SIZE` values with a compact selector
/// each, grouped into a 192-way B-tree whose nodes store a 512-bit Cartesian
/// tree (balanced parentheses) over their children's minima, plus a coarse
/// sparse table over blocks of at least 4096 values for wide queries.
pub struct HybridBTree<'a, T: PixieValue, const LEAF_SIZE: usize> {
    ptr: NonNull<c_void>,
    _data: PhantomData<&'a [T]>,
}

impl<'a, T: PixieValue, const LEAF_SIZE: usize> HybridBTree<'a, T, LEAF_SIZE> {
    const OPS: Ops<T> = ops::<T>(LEAF_SIZE);
}

impl<'a, T: PixieValue, const LEAF_SIZE: usize> RMQ<'a, T> for HybridBTree<'a, T, LEAF_SIZE> {
    fn new(data: &'a [T]) -> Self {
        let ptr = unsafe { (Self::OPS.new)(data.as_ptr(), data.len()) };
        HybridBTree {
            ptr: NonNull::new(ptr).unwrap(),
            _data: PhantomData,
        }
    }

    // Queries [l, r]
    #[inline]
    fn rmq(&self, l: usize, r: usize) -> usize {
        unsafe { (Self::OPS.rmq)(self.ptr.as_ptr(), l, r + 1) }
    }
}

// The C++ structure is opaque, so these are written by hand: the struct itself
// plus what the library reports. `mem_dbg` prints the total without a breakdown.
impl<'a, T: PixieValue, const LEAF_SIZE: usize> FlatType for HybridBTree<'a, T, LEAF_SIZE> {
    type Flat = mem_dbg::False;
}

impl<'a, T: PixieValue, const LEAF_SIZE: usize> MemSize for HybridBTree<'a, T, LEAF_SIZE> {
    fn mem_size_rec(&self, _flags: SizeFlags, _refs: &mut HashMap<usize, usize>) -> usize {
        size_of::<Self>() + unsafe { (Self::OPS.mem)(self.ptr.as_ptr()) }
    }
}

impl<'a, T: PixieValue, const LEAF_SIZE: usize> MemDbgImpl for HybridBTree<'a, T, LEAF_SIZE> {}

impl<'a, T: PixieValue, const LEAF_SIZE: usize> Drop for HybridBTree<'a, T, LEAF_SIZE> {
    fn drop(&mut self) {
        unsafe { (Self::OPS.free)(self.ptr.as_ptr()) }
    }
}

crate::rmq_family! {
    pub HybridBTreeFamily<const LEAF_SIZE: usize> => HybridBTree where T: PixieValue
}
