use crate::base::{RMQ, RMQFamily};

pub struct NaiveRMQ<'a, T: Ord>(&'a [T]);

impl<'a, T: Ord> RMQ<'a, T> for NaiveRMQ<'a, T> {
    fn new(data: &'a [T]) -> Self {
        NaiveRMQ(data)
    }
    fn rmq(&self, l: usize, r: usize) -> usize {
        let mut min_index = l;
        for i in l + 1..r + 1 {
            if self.0[i] < self.0[min_index] {
                min_index = i;
            }
        }
        min_index
    }
}

/// Family type for [`NaiveRMQ`]. Never instantiated; it exists only so generic
/// code can name the structure without committing to an input lifetime.
pub struct NaiveRMQFamily;

impl<T: Ord> RMQFamily<T> for NaiveRMQFamily {
    type Rmq<'a>
        = NaiveRMQ<'a, T>
    where
        T: 'a;
}
