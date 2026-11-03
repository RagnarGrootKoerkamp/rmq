//! Bindings to the C++ indexing FL-RMQ of Ferragina and Lari, vendored in
//! `FL-RMQ/` and compiled by `build.rs` through `ffi/flrmq_shim.cpp`.
//!
//! The C++ structure doesn't store the input: it is passed again on every
//! query, so this borrows it like the native structures here. Construction
//! copies the input into a temporary `std::vector`, which the library requires.
//!
//! Only the epsilons listed in `FLRMQ_EPSILONS` in `build.rs` are compiled;
//! any other value is a compile-time error.

use std::{ffi::c_void, ptr::NonNull};

use crate::base::{MemoryUsage, RMQ};

#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct Ops<T> {
    new: unsafe extern "C" fn(*const T, usize) -> *mut c_void,
    rmq: unsafe extern "C" fn(*mut c_void, *const T, usize, usize) -> usize,
    mem: unsafe extern "C" fn(*mut c_void) -> usize,
    free: unsafe extern "C" fn(*mut c_void),
}

/// Value types the C++ library is compiled for.
pub trait FLRMQValue: Ord + Copy + 'static {
    /// `(epsilon, functions)` for every compiled epsilon.
    #[doc(hidden)]
    const OPS: &'static [(usize, Ops<Self>)];
}

macro_rules! flrmq_value {
    ($t:ident, [$($eps:literal),*]) => {
        impl FLRMQValue for $t {
            const OPS: &'static [(usize, Ops<Self>)] = &[$(($eps, {
                unsafe extern "C" {
                    #[link_name = concat!("flrmq_", stringify!($t), "_", $eps, "_new")]
                    fn new(data: *const $t, n: usize) -> *mut c_void;
                    #[link_name = concat!("flrmq_", stringify!($t), "_", $eps, "_rmq")]
                    fn rmq(p: *mut c_void, data: *const $t, i: usize, j: usize) -> usize;
                    #[link_name = concat!("flrmq_", stringify!($t), "_", $eps, "_mem")]
                    fn mem(p: *mut c_void) -> usize;
                    #[link_name = concat!("flrmq_", stringify!($t), "_", $eps, "_free")]
                    fn free(p: *mut c_void);
                }
                Ops { new, rmq, mem, free }
            })),*];
        }
    };
}

include!(concat!(env!("OUT_DIR"), "/flrmq_ops.rs"));

const fn ops<T: FLRMQValue>(epsilon: usize) -> Ops<T> {
    let mut i = 0;
    while i < T::OPS.len() {
        if T::OPS[i].0 == epsilon {
            return T::OPS[i].1;
        }
        i += 1;
    }
    panic!("FL-RMQ is not compiled for this EPSILON; see FLRMQ_EPSILONS in build.rs");
}

/// FL-RMQ: a piecewise linear approximation, with maximum error `EPSILON`, of
/// the sparse table's argmin positions; a query scans the two predicted windows
/// of `2 * EPSILON + 2` elements. Ranges up to that length are scanned directly.
pub struct FLRMQ<'a, T: FLRMQValue, const EPSILON: usize> {
    ptr: NonNull<c_void>,
    data: &'a [T],
}

impl<'a, T: FLRMQValue, const EPSILON: usize> FLRMQ<'a, T, EPSILON> {
    const OPS: Ops<T> = ops::<T>(EPSILON);
}

impl<'a, T: FLRMQValue, const EPSILON: usize> RMQ<'a, T> for FLRMQ<'a, T, EPSILON> {
    fn new(data: &'a [T]) -> Self {
        // The library shifts an int by the query's log length.
        assert!(
            data.len() <= i32::MAX as usize,
            "FL-RMQ supports at most i32::MAX elements"
        );
        let ptr = unsafe { (Self::OPS.new)(data.as_ptr(), data.len()) };
        FLRMQ {
            ptr: NonNull::new(ptr).unwrap(),
            data,
        }
    }

    // Queries [l, r]
    #[inline]
    fn rmq(&self, l: usize, r: usize) -> usize {
        unsafe { (Self::OPS.rmq)(self.ptr.as_ptr(), self.data.as_ptr(), l, r) }
    }
}

impl<'a, T: FLRMQValue, const EPSILON: usize> MemoryUsage for FLRMQ<'a, T, EPSILON> {
    fn memory_allocated(&self) -> usize {
        unsafe { (Self::OPS.mem)(self.ptr.as_ptr()) }
    }
}

impl<'a, T: FLRMQValue, const EPSILON: usize> Drop for FLRMQ<'a, T, EPSILON> {
    fn drop(&mut self) {
        unsafe { (Self::OPS.free)(self.ptr.as_ptr()) }
    }
}

crate::rmq_family! {
    pub FLRMQFamily<const EPSILON: usize> => FLRMQ where T: FLRMQValue
}
