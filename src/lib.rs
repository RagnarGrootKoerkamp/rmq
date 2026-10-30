extern crate rand;

pub mod base;
pub mod cartesian_tree;
pub mod naive;
pub mod rmq_tree;
pub mod sparse_table;
pub mod sparse_table2;
pub mod sparse_table_blocks;
pub mod sparse_table_levels;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use naive::NaiveRMQFamily;
pub use rmq_tree::{PackedRMQTreeFamily, RMQTreeFamily};
pub use sparse_table::{
    IndexSparseTableFamily, OffsetSparseTableFamily, OwningOffsetSparseTableFamily,
    PackedIndexSparseTableFamily, PackedValueSparseTableFamily, PositionOffsetSparseTableFamily,
    ValueSparseTableFamily,
};
pub use sparse_table_blocks::{
    BitBlocksSparseTableFamily, StridedIndexAffixMasksSparseTableFamily,
    StridedIndexMinHeapSparseTableFamily, StridedIndexSparseTableFamily,
};
pub use sparse_table_levels::LevelOffsetSparseTableFamily;
pub use sparse_table2::{SparseTable3Family, SparseTableCartesianBlocksFamily};
