pub trait RMQ<'a, T: Sized> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized; // Rename to build?

    // TODO: change to argmin?
    fn rmq(&self, l: usize, r: usize) -> usize;
    // TODO: Add fn len(&self) -> usize;
}

pub trait RMQFamily<T> {
    type Rmq<'a>: RMQ<'a, T>
    where
        T: 'a;
}

pub trait OwningRMQ<T>: for<'a> RMQ<'a, T> {}

pub trait TwoArgMin {
    fn argmin(&self, a: usize, b: usize) -> usize;
}

impl TwoArgMin for &[u64] {
    fn argmin(&self, a: usize, b: usize) -> usize {
        if self[a] <= self[b] { a } else { b }
    }
}
