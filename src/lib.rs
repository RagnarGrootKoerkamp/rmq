extern crate rand;

pub mod base;
pub mod rmq_tree;
pub mod sparse_table;
pub mod cartesian_tree;
//mod sparse_table2;
pub mod naive;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
