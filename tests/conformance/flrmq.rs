use rmq::*;

// Inputs up to 2 * EPSILON + 2 elements are only scanned, so the small
// epsilons are the ones that exercise the learned model on these test sizes.
rmq_conformance!(flrmq_16, FLRMQFamily<16>);
rmq_conformance!(flrmq_32, FLRMQFamily<32>);
rmq_conformance!(flrmq_64, FLRMQFamily<64>);
rmq_conformance!(flrmq_1024, FLRMQFamily<1024>);
