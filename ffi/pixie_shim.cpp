// C ABI over pixie's HybridBTree (pixie/include/pixie/rmq/hybrid_btree.h),
// instantiated for one value type and every leaf layout the library supports.
// build.rs compiles this with:
//   PIXIE_VALUE_T  value type
//   PIXIE_NAME     name of the value type in the exported symbols
//
// The leaf size picks the layout: 248 and 496 use prefix/suffix mask leaves,
// 252 uses balanced-parentheses leaves (the only combinations upstream accepts).
#include <climits>
#include <cstddef>
#include <cstdint>
#include <span>

#include <pixie/rmq/hybrid_btree.h>

namespace {

template <std::size_t LeafSize>
using Hybrid = pixie::rmq::HybridBTree<
    PIXIE_VALUE_T, std::less<PIXIE_VALUE_T>, std::size_t, LeafSize, 256,
    LeafSize == 252 ? pixie::rmq::HybridBTreeLeafSelector::BP
                    : pixie::rmq::HybridBTreeLeafSelector::PrefixSuffix>;

}  // namespace

#define CONCAT_(a, b) a##b
#define CONCAT(a, b) CONCAT_(a, b)
#define FN(leaf, name) CONCAT(CONCAT(CONCAT(CONCAT(pixie_hybrid_, PIXIE_NAME), _), leaf), name)

#define PIXIE_INSTANCE(leaf)                                                        \
    void *FN(leaf, _new)(const PIXIE_VALUE_T *data, size_t n) {                     \
        return new Hybrid<leaf>(std::span<const PIXIE_VALUE_T>(data, n));           \
    }                                                                               \
    /* Upstream queries the half-open [l, r); the Rust side passes r + 1. */        \
    size_t FN(leaf, _rmq)(void *p, size_t l, size_t r) {                            \
        return static_cast<Hybrid<leaf> *>(p)->arg_min(l, r);                       \
    }                                                                               \
    /* Includes sizeof(*this), i.e. the heap-allocated object itself. */            \
    size_t FN(leaf, _mem)(void *p) {                                                \
        return static_cast<Hybrid<leaf> *>(p)->memory_usage_bytes();                \
    }                                                                               \
    void FN(leaf, _free)(void *p) { delete static_cast<Hybrid<leaf> *>(p); }

extern "C" {
PIXIE_INSTANCE(248)
PIXIE_INSTANCE(252)
PIXIE_INSTANCE(496)
}
