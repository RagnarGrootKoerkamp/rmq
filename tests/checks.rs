use rmq::{
    rmq_conformance,
    rmq_tree::RMQTreeFamily,
    sparse_table::{IndexSparseTableFamily, SparseTableFamily},
};

rmq_conformance!(sparse_table, SparseTableFamily);
rmq_conformance!(rmq_tree, RMQTreeFamily);
rmq_conformance!(index_sparse_table, IndexSparseTableFamily);
