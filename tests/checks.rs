use rmq::{
    bit_blocks::BitBlocksSparseTableFamily,
    rmq_conformance,
    rmq_tree::RMQTreeFamily,
    sparse_table::{IndexSparseTableFamily, OffsetSparseTableFamily, PackedIndexSparseTableFamily},
    sparse_table2::SparseTableOnBlocksFamily,
};

rmq_conformance!(sparse_table, OffsetSparseTableFamily);
rmq_conformance!(rmq_tree, RMQTreeFamily);
rmq_conformance!(index_sparse_table, IndexSparseTableFamily);
rmq_conformance!(packed_index_sparse_table, PackedIndexSparseTableFamily);
rmq_conformance!(sparse_table_on_blocks, SparseTableOnBlocksFamily);
rmq_conformance!(bit_blocks_sparse_table, BitBlocksSparseTableFamily);
