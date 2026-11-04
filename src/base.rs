use mem_dbg::{MemDbg, MemSize};
use sux::{
    bits::{BitFieldVec, BitFieldVecU},
    traits::{SliceByValue, SliceByValueMut, TryIntoUnaligned, Word},
};

pub trait RMQ<'a, T: Sized> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized;

    fn rmq(&self, l: usize, r: usize) -> usize;
    fn rmq_batch(&self, queries: &[(usize, usize)], answers: &mut [usize]) {
        answers
            .iter_mut()
            .zip(queries)
            .for_each(|(ans, &(l, r))| *ans = self.rmq(l, r));
    }
}

pub trait RMQFamily<T> {
    /// `MemSize::mem_size` gives the total size (struct plus heap). Borrowed
    /// input data is not counted unless `SizeFlags::FOLLOW_REFS` is passed.
    type Rmq<'a>: RMQ<'a, T> + MemSize + MemDbg
    where
        T: 'a;

    const NAME: &'static str;
    const PARAMS: &'static [(&'static str, usize)] = &[];
}

/// Declares a family type for an RMQ structure of the form `Name<'a, T>`.
/// `$rmq` must be an identifier in scope at the call site. Family types are
/// never instantiated; they exist only so generic code can name the structure
/// without committing to an input lifetime. Bounds on `T` default to `Ord`;
/// pass `where T: ...` to override them. `T: MemSize + MemDbg + FlatType` is
/// always added, since the derived impls need `&'a [T]: MemSize + MemDbg`.
///
/// Const parameters can be declared on the family (`Family<const B: usize>`);
/// they are forwarded in order after `T`, so the structure must have the form
/// `Name<'a, T, B, ...>`.
#[macro_export]
macro_rules! rmq_family {
    ($(#[$attr:meta])* $vis:vis $family:ident $(<$(const $p:ident : $pty:ty),+ $(,)?>)?
        => $rmq:ident where T: $($bound:tt)+) => {
        $(#[$attr])*
        $vis struct $family $(<$(const $p: $pty),+>)?;

        impl<T $($(, const $p: $pty)+)?> $crate::base::RMQFamily<T>
            for $family $(<$($p),+>)?
        where
            T: $($bound)+ + ::mem_dbg::MemSize + ::mem_dbg::MemDbg + ::mem_dbg::FlatType
        {
            type Rmq<'a>
                = $rmq<'a, T $($(, $p)+)?>
            where
                T: 'a;

            const NAME: &'static str = stringify!($family);
            $(const PARAMS: &'static [(&'static str, usize)] =
                &[$((stringify!($p), $p as usize)),+];)?
        }
    };
    ($(#[$attr:meta])* $vis:vis $family:ident $(<$(const $p:ident : $pty:ty),+ $(,)?>)?
        => $rmq:ident) => {
        $crate::rmq_family! {
            $(#[$attr])* $vis $family $(<$(const $p: $pty),+>)? => $rmq where T: Ord
        }
    };
}

pub trait OwningRMQ<T>: for<'a> RMQ<'a, T> {}

pub trait TwoArgMin {
    fn argmin(&self, a: usize, b: usize) -> usize;
}

impl<T: Ord> TwoArgMin for &[T] {
    fn argmin(&self, a: usize, b: usize) -> usize {
        if self[a] <= self[b] { a } else { b }
    }
}

/// A by-value slice that is filled through a mutable `Builder` and then frozen.
pub trait BuildableSlice: SliceByValue + Sized {
    type Builder: SliceByValueMut<Value = Self::Value>;
    fn new_builder(n: usize, bit_width: usize) -> Self::Builder;
    fn build(builder: Self::Builder) -> Self;
}

impl<W: Word> BuildableSlice for Vec<W> {
    type Builder = Self;
    fn new_builder(n: usize, _bit_width: usize) -> Self {
        vec![W::ZERO; n]
    }
    fn build(builder: Self) -> Self {
        builder
    }
}

impl<W: Word> BuildableSlice for BitFieldVec<Box<[W]>> {
    type Builder = Self;
    fn new_builder(n: usize, bit_width: usize) -> Self {
        BitFieldVec::new_padded(bit_width, n)
    }
    fn build(builder: Self) -> Self {
        builder
    }
}

impl<W: Word> BuildableSlice for BitFieldVecU<Box<[W]>> {
    type Builder = BitFieldVec<Box<[W]>>;
    fn new_builder(n: usize, bit_width: usize) -> Self::Builder {
        BitFieldVec::new_padded(bit_width, n)
    }
    fn build(builder: Self::Builder) -> Self {
        // new_padded already has the padding word, so no reallocation here
        builder
            .try_into_unaligned()
            .expect("bit width unsupported for unaligned reads")
    }
}
