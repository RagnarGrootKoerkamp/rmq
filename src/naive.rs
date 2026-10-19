use crate::base::{MemoryUsage, RMQ};

pub struct NaiveRMQ<'a, T: Ord>(&'a [T]);

impl<'a, T: Ord> MemoryUsage for NaiveRMQ<'a, T> {
    fn memory_allocated(&self) -> usize {
        0
    }
}

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

crate::rmq_family! {
    pub NaiveRMQFamily => NaiveRMQ
}
