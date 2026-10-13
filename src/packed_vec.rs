// AI Generated

pub struct PackedVec {
    words: Vec<u64>,
    width: u32, // bits per element, 1..=128 (`get`/`set` need <= 64, wider needs `get_u128`/`set_u128`)
    mask: u128,
    len: usize,
}

impl PackedVec {
    pub fn memory_bytes(&self) -> usize {
        self.words.len() * std::mem::size_of::<u64>()
    }

    pub fn new(len: usize, width: u32) -> Self {
        assert!((1..=128).contains(&width));
        let bits = len * width as usize;
        // Two extra words of padding so accessors can always touch words[w + 1] and words[w + 2].
        let words = vec![0; bits.div_ceil(64) + 2];
        let mask = if width == 128 {
            u128::MAX
        } else {
            (1 << width) - 1
        };
        Self {
            words,
            width,
            mask,
            len,
        }
    }

    #[inline]
    fn locate(&self, i: usize) -> (usize, u32) {
        debug_assert!(i < self.len);
        let bit = i * self.width as usize;
        (bit / 64, (bit % 64) as u32)
    }

    #[inline]
    pub fn get(&self, i: usize) -> u64 {
        let (w, s) = self.locate(i);
        let x = self.words[w] as u128 | (self.words[w + 1] as u128) << 64;
        (x >> s) as u64 & self.mask as u64
    }

    #[inline]
    pub fn set(&mut self, i: usize, v: u64) {
        debug_assert!(self.width <= 64 && v as u128 <= self.mask);
        let (w, s) = self.locate(i);
        let mut x = self.words[w] as u128 | (self.words[w + 1] as u128) << 64;
        x &= !(self.mask << s);
        x |= (v as u128) << s;
        self.words[w] = x as u64;
        self.words[w + 1] = (x >> 64) as u64;
    }

    // Elements up to 128 bits wide can span three words. `<< 64 << (64 - s)` and
    // `>> 64 >> (64 - s)` are shifts by `128 - s` that yield 0 instead of overflowing when s == 0.

    #[inline]
    pub fn get_u128(&self, i: usize) -> u128 {
        let (w, s) = self.locate(i);
        let x = self.words[w] as u128 | (self.words[w + 1] as u128) << 64;
        let hi = (self.words[w + 2] as u128) << 64 << (64 - s);
        ((x >> s) | hi) & self.mask
    }

    #[inline]
    pub fn set_u128(&mut self, i: usize, v: u128) {
        debug_assert!(v <= self.mask);
        let (w, s) = self.locate(i);
        let mut x = self.words[w] as u128 | (self.words[w + 1] as u128) << 64;
        x &= !(self.mask << s);
        x |= v << s;
        self.words[w] = x as u64;
        self.words[w + 1] = (x >> 64) as u64;
        let hi_mask = (self.mask >> 64 >> (64 - s)) as u64;
        self.words[w + 2] = self.words[w + 2] & !hi_mask | (v >> 64 >> (64 - s)) as u64;
    }

    pub fn len(&self) -> usize {
        self.len
    }
}
