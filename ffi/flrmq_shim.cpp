// C ABI over the indexing FL-RMQ (FL-RMQ/include/fl_rmq.hpp), instantiated
// for one value type and every epsilon in a list. build.rs compiles this with:
//   FLRMQ_VALUE_T         value type
//   FLRMQ_NAME            name of the value type in the exported symbols
//   FLRMQ_FOR_EACH_EPS(X) X(eps) for every supported epsilon
#include <cstddef>
#include <cstdint>
#include <vector>

#include "fl_rmq.hpp"

namespace {

// FLRMQ::query takes the data as a std::vector, which a Rust slice can't be
// passed as without copying. This mirrors FLRMQ::query (with Samples = 0) but
// reads the data through a pointer, using the protected members the library
// exposes for subclasses (as its own ExcessFLRMQ does).
//
// Upstream reads first_segment[k + 1] out of bounds when k = lg(n), and has
// overwritten first_segment[lg(n)] by then, so the search for the longest
// diagonal is bounded by top_first and the last segment instead.
//
// Upstream's predict returns a size_t, so a prediction that falls before
// position 0 (which the error bound allows near the start of a diagonal) wraps
// around and reduce_interval returns an empty window. reduce_interval below is
// upstream's with the prediction kept signed.
//
// Rightmost = false makes the scans and the learned sparse table pick the
// leftmost minimum. The final comparison uses <= instead of upstream's <, which
// would return the right candidate on a tie even though the left one is the
// leftmost minimum.
template <typename K, size_t Epsilon>
class PtrFLRMQ : public FLRMQ<K, int64_t, int64_t, float, 0, Epsilon, false> {
    using Base = FLRMQ<K, int64_t, int64_t, float, 0, Epsilon, false>;

public:
    // Inputs this short never reach the learned model (every query is a
    // scan), and building it would read out of bounds, so it's skipped.
    explicit PtrFLRMQ(const std::vector<K> &data)
        : Base(data.size() > 2 * (Epsilon + 1) ? Base(data) : Base()) {
        if (data.size() > 2 * (Epsilon + 1)) {
            lg_n = sdsl::bits::hi(data.size());
            const auto first = this->segment_for_range(this->encode(0, (1UL << lg_n) - 1),
                                                       this->ranges.begin(), this->ranges.end());
            top_first = std::distance(this->ranges.begin(), first);
        }
    }

    size_t size() const {
        return Base::size() + (sizeof(lg_n) + sizeof(top_first)) * CHAR_BIT;
    }

    size_t query(const K *data, const size_t i, const size_t j) const {
        const auto len = j - i + 1;

        if (len <= 2 * (Epsilon + 1)) {
            return scan_min(data, i, j).second;
        }

        const auto k = sdsl::bits::hi(len);

        const auto e1 = this->encode(i, i + (1 << k) - 1);
        const auto e2 = this->encode(j - (1 << k) + 1, j);

        const auto first = std::next(this->ranges.begin(),
                                     k == lg_n ? top_first : this->first_segment[k]);
        const auto last = k == lg_n ? std::prev(this->ranges.end())
                                    : std::next(this->ranges.begin(), this->first_segment[k + 1]);

        const auto it_s1 = this->segment_for_range(e1, first, last + 1);
        const auto it_s2 = this->segment_for_range(e2, it_s1, last + 1);

        auto [lo1, hi1] = reduce_interval(std::distance(this->ranges.begin(), it_s1), e1,
                                          this->deltas[k], i, i + (1 << k) - 1);

        auto [lo2, hi2] = reduce_interval(std::distance(this->ranges.begin(), it_s2), e2,
                                          this->deltas[k], j - (1 << k) + 1, j);

        auto [m1, p1] = scan_min(data, lo1, hi1);
        auto [m2, p2] = scan_min(data, lo2, hi2);

        return (m1 <= m2) ? p1 : p2;
    }

private:
    size_t lg_n = 0;
    size_t top_first = 0;

    std::pair<size_t, size_t> reduce_interval(const size_t s, const size_t e, const int64_t delta,
                                              const size_t lo, const size_t hi) const {
        // int64_t rather than upstream's size_t: the slope can be negative.
        const auto pos = int64_t(this->slopes[s] * double(e - uint64_t(this->ranges[s])));
        const int64_t cp = pos + this->intercepts[s] - delta;
        const int64_t reduced_lo = std::max<int64_t>(lo, cp - int64_t(Epsilon + 1));
        const int64_t reduced_hi = std::min<int64_t>(hi, cp + int64_t(Epsilon + 2));
        return std::make_pair(size_t(reduced_lo), size_t(reduced_hi));
    }

    static std::pair<K, size_t> scan_min(const K *data, const size_t lo, const size_t hi) {
        K min = data[lo];
        size_t idx = lo;
        for (auto i = lo + 1; i <= hi; ++i)
            if (data[i] < min) {
                min = data[i];
                idx = i;
            }
        return std::make_pair(min, idx);
    }
};

}  // namespace

#define CONCAT_(a, b) a##b
#define CONCAT(a, b) CONCAT_(a, b)
#define FN(eps, name) CONCAT(CONCAT(CONCAT(CONCAT(flrmq_, FLRMQ_NAME), _), eps), name)

#define FLRMQ_INSTANCE(eps)                                                          \
    void *FN(eps, _new)(const FLRMQ_VALUE_T *data, size_t n) {                      \
        return new PtrFLRMQ<FLRMQ_VALUE_T, eps>(std::vector<FLRMQ_VALUE_T>(data, data + n)); \
    }                                                                                \
    size_t FN(eps, _rmq)(void *p, const FLRMQ_VALUE_T *data, size_t i, size_t j) {   \
        return static_cast<PtrFLRMQ<FLRMQ_VALUE_T, eps> *>(p)->query(data, i, j);    \
    }                                                                                \
    /* size() is in bits; add the heap-allocated object itself. */                   \
    size_t FN(eps, _mem)(void *p) {                                                  \
        auto *rmq = static_cast<PtrFLRMQ<FLRMQ_VALUE_T, eps> *>(p);                  \
        return rmq->size() / CHAR_BIT + sizeof(*rmq);                                \
    }                                                                                \
    void FN(eps, _free)(void *p) {                                                   \
        delete static_cast<PtrFLRMQ<FLRMQ_VALUE_T, eps> *>(p);                       \
    }

extern "C" {
FLRMQ_FOR_EACH_EPS(FLRMQ_INSTANCE)
}
