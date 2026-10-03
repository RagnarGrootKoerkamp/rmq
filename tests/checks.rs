use rmq::{
    rmq_conformance,
    rmq_tree::RMQTreeFamily,
    sparse_table::{IndexSparseTableFamily, OffsetSparseTableFamily},
    sparse_table2::SparseTableOnBlocksFamily,
};

rmq_conformance!(sparse_table, OffsetSparseTableFamily);
rmq_conformance!(rmq_tree, RMQTreeFamily);
rmq_conformance!(index_sparse_table, IndexSparseTableFamily);
rmq_conformance!(sparse_table_on_blocks, SparseTableOnBlocksFamily);

#[test]
fn sparse_table_on_blocks_large_blocks() {
    let mut state = 0x9E3779B97F4A7C15u64;
    let mut next = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        state >> 33
    };
    for n in [256, 257, 258, 4095, 4096, 4097, 65536, 65537, 65539] {
        for distinct in [3, u64::MAX] {
            let data: Vec<u64> = (0..n).map(|_| next() % distinct).collect();
            rmq::testing::compare_with_naive::<SparseTableOnBlocksFamily>(&data);
        }
    }
}
