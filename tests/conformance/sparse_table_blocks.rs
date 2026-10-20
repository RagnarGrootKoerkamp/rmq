use rmq::*;

rmq_conformance!(bit_blocks_sparse_table_1, BitBlocksSparseTableFamily<1>);
rmq_conformance!(bit_blocks_sparse_table_7, BitBlocksSparseTableFamily<7>);
rmq_conformance!(bit_blocks_sparse_table_32, BitBlocksSparseTableFamily<32>);
rmq_conformance!(bit_blocks_sparse_table_45, BitBlocksSparseTableFamily<45>);
rmq_conformance!(bit_blocks_sparse_table_58, BitBlocksSparseTableFamily<58>);
rmq_conformance!(bit_blocks_sparse_table_60, BitBlocksSparseTableFamily<60>);
rmq_conformance!(bit_blocks_sparse_table_64, BitBlocksSparseTableFamily<64>);

// B = 2 never takes the fallback path; 3 doesn't divide most sizes; 64 is a realistic block.
type StridedIndex2 = StridedIndexSparseTableFamily<2, { usize::MAX }>;
type StridedIndex3 = StridedIndexSparseTableFamily<3, { usize::MAX }>;
type StridedIndex64 = StridedIndexSparseTableFamily<64, 32>;
rmq_conformance!(strided_index_sparse_table_2, StridedIndex2);
rmq_conformance!(strided_index_sparse_table_3, StridedIndex3);
rmq_conformance!(strided_index_sparse_table_64, StridedIndex64);

// Same block sizes as StridedIndex; 64 is the widest mask a block can hold.
type StridedIndexAffixMasks2 = StridedIndexAffixMasksSparseTableFamily<2, { usize::MAX }>;
type StridedIndexAffixMasks3 = StridedIndexAffixMasksSparseTableFamily<3, { usize::MAX }>;
type StridedIndexAffixMasks64 = StridedIndexAffixMasksSparseTableFamily<64, 32>;
rmq_conformance!(
    strided_index_affix_masks_sparse_table_2,
    StridedIndexAffixMasks2
);
rmq_conformance!(
    strided_index_affix_masks_sparse_table_3,
    StridedIndexAffixMasks3
);
rmq_conformance!(
    strided_index_affix_masks_sparse_table_64,
    StridedIndexAffixMasks64
);

// 32 is the widest block whose identifier fits in a u64.
type StridedIndexMinHeap2 = StridedIndexMinHeapSparseTableFamily<2, { usize::MAX }>;
type StridedIndexMinHeap3 = StridedIndexMinHeapSparseTableFamily<3, { usize::MAX }>;
type StridedIndexMinHeap32 = StridedIndexMinHeapSparseTableFamily<32, 16>;
rmq_conformance!(strided_index_min_heap_sparse_table_2, StridedIndexMinHeap2);
rmq_conformance!(strided_index_min_heap_sparse_table_3, StridedIndexMinHeap3);
rmq_conformance!(
    strided_index_min_heap_sparse_table_32,
    StridedIndexMinHeap32
);
