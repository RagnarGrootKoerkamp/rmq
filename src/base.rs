pub trait RMQ<'a, T: Sized> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized;

    fn rmq(&self, l: usize, r: usize) -> usize;
}

pub trait RMQFamily<T> {
    type Rmq<'a>: RMQ<'a, T> + MemoryUsage
    where
        T: 'a;

    const NAME: &'static str;
    const PARAMS: &'static [(&'static str, usize)] = &[];
}

/// Declares a family type for an RMQ structure of the form `Name<'a, T>`.
/// `$rmq` must be an identifier in scope at the call site. Family types are
/// never instantiated; they exist only so generic code can name the structure
/// without committing to an input lifetime. Bounds on `T` default to `Ord`;
/// pass `where T: ...` to override them.
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
            T: $($bound)+
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

pub trait MemoryUsage: Sized {
    /// Heap bytes allocated by this structure, excluding the struct itself
    /// and the borrowed input data.
    fn memory_allocated(&self) -> usize;

    fn memory_total(&self) -> usize {
        std::mem::size_of::<Self>() + self.memory_allocated()
    }
}

impl<W: sux::traits::Word> MemoryUsage for sux::bits::BitFieldVec<Vec<W>> {
    /// Bytes used by the packed backing words; ignores struct and capacity overhead.
    fn memory_allocated(&self) -> usize {
        let word_bits = 8 * size_of::<W>();
        (self.len() * self.bit_width()).div_ceil(word_bits) * size_of::<W>()
    }
}
