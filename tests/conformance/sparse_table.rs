use rmq::*;

rmq_conformance!(sparse_table, OffsetSparseTableFamily);
rmq_conformance!(index_sparse_table, IndexSparseTableFamily);
rmq_conformance!(packed_index_sparse_table, PackedIndexSparseTableFamily);
rmq_conformance!(
    position_offset_sparse_table,
    PositionOffsetSparseTableFamily
);
rmq_conformance!(owning_offset_sparse_table, OwningOffsetSparseTableFamily);
rmq_conformance!(value_sparse_table, ValueSparseTableFamily);
rmq_conformance!(packed_value_sparse_table, PackedValueSparseTableFamily);
