pub trait RMQ<'a, T: Sized> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized;

    // TODO: change to argmin?
    fn rmq(&self, l: usize, r: usize) -> usize;
    // TODO: Add fn len(&self) -> usize;
}

pub trait RMQFamily<T> {
    type Rmq<'a>: RMQ<'a, T> + MemoryUsage
    where
        T: 'a;
}

/// Declares a family type for an RMQ structure of the form `Name<'a, T>`.
/// `$rmq` must be an identifier in scope at the call site. Family types are
/// never instantiated; they exist only so generic code can name the structure
/// without committing to an input lifetime. Bounds on `T` default to `Ord`;
/// pass `where T: ...` to override them.
#[macro_export]
macro_rules! rmq_family {
    ($(#[$attr:meta])* $vis:vis $family:ident => $rmq:ident where T: $($bound:tt)+) => {
        $(#[$attr])*
        $vis struct $family;

        impl<T: $($bound)+> $crate::base::RMQFamily<T> for $family {
            type Rmq<'a>
                = $rmq<'a, T>
            where
                T: 'a;
        }
    };
    ($(#[$attr:meta])* $vis:vis $family:ident => $rmq:ident) => {
        $crate::rmq_family! { $(#[$attr])* $vis $family => $rmq where T: Ord }
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

pub trait MemoryUsage {
    /// Bytes used by this structure, excluding the borrowed input data.
    fn memory_bytes(&self) -> usize;
}

impl MemoryUsage for sux::bits::BitFieldVec {
    /// Bytes used by the packed backing words; ignores struct and capacity overhead.
    fn memory_bytes(&self) -> usize {
        (self.len() * self.bit_width()).div_ceil(usize::BITS as usize) * size_of::<usize>()
    }
}
