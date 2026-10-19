use rmq::{
    bit_blocks::BitBlocksSparseTableFamily,
    rmq_conformance,
    rmq_tree::RMQTreeFamily,
    sparse_table::{
        IndexSparseTableFamily, OffsetSparseTableFamily, PackedIndexSparseTableFamily,
        PackedValueSparseTableFamily, PositionOffsetSparseTableFamily, PositionOffsetSparseTable2Family, StridedIndexAffixMasksSparseTableFamily,
        StridedIndexMinHeapSparseTableFamily, StridedIndexSparseTableFamily,
        ValueSparseTableFamily,
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
rmq_conformance!(
    position_offset_sparse_table2,
    PositionOffsetSparseTable2Family
);
rmq_conformance!(sparse_table_on_blocks, SparseTableCartesianBlocksFamily);
rmq_conformance!(bit_blocks_sparse_table, BitBlocksSparseTableFamily);
rmq_conformance!(value_sparse_table, ValueSparseTableFamily);
rmq_conformance!(packed_value_sparse_table, PackedValueSparseTableFamily);
// B = 1 never takes the fallback path; 3 doesn't divide most sizes; 64 is a realistic block.
type StridedIndex2 = StridedIndexSparseTableFamily<2, {usize::MAX}>;
type StridedIndex3 = StridedIndexSparseTableFamily<3, {usize::MAX}>;
type StridedIndex64 = StridedIndexSparseTableFamily<64, 32>;
rmq_conformance!(strided_index_sparse_table_1, StridedIndex2);
rmq_conformance!(strided_index_sparse_table_3, StridedIndex3);
rmq_conformance!(strided_index_sparse_table_64, StridedIndex64);
rmq_conformance!(sparse_table3, SparseTable3Family);
// Same block sizes as StridedIndex; 64 is the widest mask a block can hold.
type StridedIndexAffixMasks2 = StridedIndexAffixMasksSparseTableFamily<2, {usize::MAX}>;
type StridedIndexAffixMasks3 = StridedIndexAffixMasksSparseTableFamily<3, {usize::MAX}>;
type StridedIndexAffixMasks64 = StridedIndexAffixMasksSparseTableFamily<64, 32>;
rmq_conformance!(strided_index_affix_masks_sparse_table_1, StridedIndexAffixMasks2);
rmq_conformance!(strided_index_affix_masks_sparse_table_3, StridedIndexAffixMasks3);
rmq_conformance!(strided_index_affix_masks_sparse_table_64, StridedIndexAffixMasks64);

// 32 is the widest block whose identifier fits in a u64.
type StridedIndexMinHeap2 = StridedIndexMinHeapSparseTableFamily<2, {usize::MAX}>;
type StridedIndexMinHeap3 = StridedIndexMinHeapSparseTableFamily<3, {usize::MAX}>;
type StridedIndexMinHeap32 = StridedIndexMinHeapSparseTableFamily<32, 16>;
rmq_conformance!(strided_index_min_heap_sparse_table_1, StridedIndexMinHeap2);
rmq_conformance!(strided_index_min_heap_sparse_table_3, StridedIndexMinHeap3);
rmq_conformance!(strided_index_min_heap_sparse_table_32, StridedIndexMinHeap32);
