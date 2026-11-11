pub fn convert_to_minheap<T: Ord>(block: &[T], width: usize) -> u64 {
    let mut stack: Vec<&T> = Vec::with_capacity(width);
    let mut id = 0u64;
    let mut bits = 0u32;
    for x in block {
        while let Some(&top) = stack.last() {
            if top <= x {
                break;
            }
            stack.pop();
            id <<= 1;
            bits += 1;
        }
        stack.push(x);
        id = (id << 1) | 1;
        bits += 1;
    }
    // Pad with +inf
    let padding = (width - block.len()) as u32;
    id = (id << padding) | ((1u64 << padding) - 1);
    bits += padding;
    id << (2 * width as u32 - bits)
}

/// Given an identifier from `convert_to_minheap`, return the index of the
/// (leftmost) minimum in `block[l..=r]`.
pub fn rmq_on_minheap(id: u64, width: usize, l: usize, r: usize) -> usize {
    debug_assert!(l <= r && r < width && width <= 32);
    // Bit j of `stack` is set iff index j is on the stack
    let mut stack = 0u64;
    let mut bit = 2 * width as u32;
    let mut i = 0;
    while i <= r {
        bit -= 1;
        if (id >> bit) & 1 == 1 {
            stack |= 1 << i;
            i += 1;
        } else {
            stack &= !(1 << (63 - stack.leading_zeros()));
        }
    }
    (stack & (!0u64 << l)).trailing_zeros() as usize
}

// region: rmq_on_minheap using nibble groups (Fable)

#[cfg(all(target_arch = "x86_64", target_feature = "sse4.1", target_feature = "bmi2"))]
mod fast {
    use core::arch::x86_64::*;

    /// Returns q-1, where q in 1..=64 is the smallest prefix length maximising
    /// (#ones - #zeros) among the q lowest bits of `y`.
    #[inline(always)]
    fn argmax_excess(y: u64) -> u32 {
        unsafe {
            // Per-nibble tables, indexed by nibble value 0..15.
            let nexc = _mm_setr_epi8(4, 2, 2, 0, 2, 0, 0, -2, 2, 0, 0, -2, 0, -2, -2, -4);
            let key = _mm_setr_epi8(65, 63, 64, 62, 65, 63, 63, 61, 65, 63, 64, 62, 64, 62, 62, 60);
            let pos = _mm_setr_epi8(0, 0, 1, 1, 0, 0, 2, 2, 0, 0, 1, 1, 3, 3, 3, 3);
            let base = _mm_setr_epi8(0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 60);

            // 16 bytes, one nibble of y each.
            let v = _mm_cvtsi64_si128(y as i64);
            let v = _mm_and_si128(
                _mm_unpacklo_epi8(v, _mm_srli_epi64(v, 4)),
                _mm_set1_epi8(15),
            );
            // -(net excess), 64 - (local max), global position of local max.
            let e = _mm_shuffle_epi8(nexc, v);
            let k = _mm_shuffle_epi8(key, v);
            let p = _mm_add_epi8(_mm_shuffle_epi8(pos, v), base);
            // Exclusive prefix sum over the 16 nibbles.
            let e = _mm_slli_si128(e, 1);
            let e = _mm_add_epi8(e, _mm_slli_si128(e, 1));
            let e = _mm_add_epi8(e, _mm_slli_si128(e, 2));
            let e = _mm_add_epi8(e, _mm_slli_si128(e, 4));
            let e = _mm_add_epi8(e, _mm_slli_si128(e, 8));
            let e = _mm_add_epi8(e, k);
            // Words of (64 - excess) << 8 | position; the minimum wins.
            let m = _mm_min_epu16(_mm_unpacklo_epi8(p, e), _mm_unpackhi_epi8(p, e));
            (_mm_cvtsi128_si32(_mm_minpos_epu16(m)) as u32) & 0xFF
        }
    }

    /// Given an identifier from `convert_to_minheap`, return the index of the
    /// (leftmost) minimum in `block[l..=r]`.
    pub fn rmq_on_minheap(id: u64, width: usize, l: usize, r: usize) -> usize {
        debug_assert!(l <= r && r < width && width <= 32);
        let id = id & (!0u64 >> (64 - 2 * width as u32));
        let ones = id.count_ones() as usize;

        // Select: bit positions of push(r) (lo) and push(l) (hi).
        let (lo, hi) = unsafe {
            (
                _pdep_u64(1u64 << (ones - 1 - r), id).trailing_zeros(),
                _pdep_u64(1u64 << (ones - 1 - l), id).trailing_zeros(),
            )
        };
        // Operations from push(l) to push(r); push(r) ends up in bit 0.
        let y = (id >> lo) & (2u64 << (hi - lo)).wrapping_sub(1);

        // Last excess minimum in the window = first excess maximum from the low end.
        let q = argmax_excess(y);

        // Rank: pushes in the window before that point, offset by l.
        l + ((y >> q) >> 1).count_ones() as usize
    }
}

// endregion
