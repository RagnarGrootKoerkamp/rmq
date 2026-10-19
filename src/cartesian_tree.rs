pub fn cartesian_tree_identifier<T: Ord>(block: &[T], width: usize) -> u64 {
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

/// Given an identifier from `cartesian_tree_identifier`, return the index of the
/// (leftmost) minimum in `block[l..=r]`.
pub fn rmq_from_identifier(id: u64, width: usize, l: usize, r: usize) -> usize {
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
