extern crate rand;

pub mod base;
pub mod cartesian_tree;
pub mod rmq_tree;
pub mod sparse_table;
//mod sparse_table2;
pub mod naive;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
