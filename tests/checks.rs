use rmq::{
    bit_blocks::BitBlocksSparseTableFamily,
    rmq_conformance,
    rmq_tree::RMQTreeFamily,
    sparse_table::{
        IndexSparseTableFamily, OffsetSparseTableFamily, PackedIndexSparseTableFamily,
        PackedValueSparseTableFamily, PositionOffsetSparseTableFamily, ValueSparseTableFamily,
    },
    sparse_table2::{SparseTable3Family, SparseTableCartesianBlocksFamily},
};

rmq_conformance!(sparse_table, OffsetSparseTableFamily);
rmq_conformance!(rmq_tree, RMQTreeFamily);
rmq_conformance!(index_sparse_table, IndexSparseTableFamily);
rmq_conformance!(packed_index_sparse_table, PackedIndexSparseTableFamily);
rmq_conformance!(
    position_offset_sparse_table,
    PositionOffsetSparseTableFamily
);
rmq_conformance!(sparse_table_on_blocks, SparseTableCartesianBlocksFamily);
rmq_conformance!(bit_blocks_sparse_table, BitBlocksSparseTableFamily);
rmq_conformance!(value_sparse_table, ValueSparseTableFamily);
rmq_conformance!(packed_value_sparse_table, PackedValueSparseTableFamily);
rmq_conformance!(sparse_table3, SparseTable3Family);
