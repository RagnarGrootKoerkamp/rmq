use rmq::*;

rmq_conformance!(rmq_tree, RMQTreeFamily);
rmq_conformance!(packed_rmq_tree, PackedRMQTreeFamily);

// 2 matches the binary tree; 64 leaves only a level or two for small n.
type FanoutRMQTree2 = FanoutRMQTreeFamily<2>;
type FanoutRMQTree4 = FanoutRMQTreeFamily<4>;
type FanoutRMQTree8 = FanoutRMQTreeFamily<8>;
type FanoutRMQTree64 = FanoutRMQTreeFamily<64>;
rmq_conformance!(fanout_rmq_tree_2, FanoutRMQTree2);
rmq_conformance!(fanout_rmq_tree_4, FanoutRMQTree4);
rmq_conformance!(fanout_rmq_tree_8, FanoutRMQTree8);
rmq_conformance!(fanout_rmq_tree_64, FanoutRMQTree64);
