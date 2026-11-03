// C ABI over BbST (BbST/bbst.cpp), instantiated once per value type and variant.
// build.rs compiles this with:
//   BBST_VALUE_T, BBST_VALUE_MAX  value type and its maximum (sentinel)
//   BBST_PREFIX                   prefix for the exported symbols
//   MINI_BLOCKS                   optional, selects the BbST2 variant
// common.h hardcodes int32_t values, so we pre-empt its include guard and
// provide the same definitions with our value type instead.
#define COMMON_H
#include <algorithm>
#include <climits>
#include <cstddef>
#include <cstdint>
#include <vector>

typedef BBST_VALUE_T t_value;
typedef unsigned int t_array_size;
typedef long long int t_array_size_2x;
#define MAX_T_VALUE BBST_VALUE_MAX
#define MAX_T_ARRAYSIZE UINT32_MAX
using namespace std;

// Each instantiation gets its own class name so they can be linked together.
#define CONCAT_(a, b) a##b
#define CONCAT(a, b) CONCAT_(a, b)
#define BbST CONCAT(BBST_PREFIX, _class)
#include "bbst.cpp"

#define FN(name) CONCAT(BBST_PREFIX, name)

extern "C" {

#ifdef MINI_BLOCKS
void *FN(_new)(const t_value *data, t_array_size n, int k_exp, int mini_k_exp) {
    return new BbST(data, n, k_exp, mini_k_exp);
}
#else
void *FN(_new)(const t_value *data, t_array_size n, int k_exp) {
    return new BbST(data, n, k_exp);
}
#endif

t_array_size FN(_rmq)(void *p, t_array_size l, t_array_size r) {
    return static_cast<BbST *>(p)->rmq(l, r);
}

// Tables plus the heap-allocated object itself.
size_t FN(_mem)(void *p) {
    return static_cast<BbST *>(p)->memUsageInBytes() + sizeof(BbST);
}

void FN(_free)(void *p) {
    delete static_cast<BbST *>(p);
}

}
