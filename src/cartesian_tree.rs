
pub fn cartesian_tree_identifier(block: &[u64], width: usize) -> u64 {
    let mut stack: Vec<u64> = Vec::with_capacity(width);
    let mut id = 0u64;
    let mut bits = 0u32;
    for &x in block {
        while let Some(&top) = stack.last() {
            if top <= x {break;}
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
    id = (id << padding) | ((1u64 << padding) -1);
    bits += padding;
    id << (2*width as u32 - bits)
}