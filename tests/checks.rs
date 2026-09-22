use rmq::{rmq_conformance, rmq_tree::RMQTreeFamily, sparse_table::SparseTableFamily};

rmq_conformance!(sparse_table, SparseTableFamily);
rmq_conformance!(rmq_tree, RMQTreeFamily);
