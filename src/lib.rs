extern crate rand;

pub mod base;
pub mod bit_blocks;
pub mod cartesian_tree;
pub mod naive;
pub mod rmq_tree;
pub mod sparse_table;
pub mod sparse_table2;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use bit_blocks::BitBlocksSparseTableFamily;
pub use naive::NaiveRMQFamily;
pub use rmq_tree::RMQTreeFamily;
pub use sparse_table::{
    IndexSparseTableFamily, OffsetSparseTableFamily, PackedIndexSparseTableFamily, PackedValueSparseTableFamily,
    PositionOffsetSparseTable2Family, PositionOffsetSparseTableFamily, StridedIndexSparseTableFamily, ValueSparseTableFamily, StridedIndexAffixMasksSparseTableFamily, StridedIndexMinHeapSparseTableFamily,
};
pub use sparse_table2::{SparseTable3Family, SparseTableCartesianBlocksFamily};
