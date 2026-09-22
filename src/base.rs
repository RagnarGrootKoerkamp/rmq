pub trait RMQ<'a, T: Sized> {
    fn new(data: &'a [T]) -> Self
    where
        Self: Sized; // Rename to build?
    fn rmq(&self, l: usize, r: usize) -> usize;
}

pub trait OwningRMQ<T>: for<'a> RMQ<'a, T> {}
