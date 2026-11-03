use rmq::*;

// K_EXP = 0 makes every element its own block; 3 and 6 exercise boundary scans
// at small sizes; 14 is the paper's default, a single block for most test sizes.
rmq_conformance!(bbst_0, BbSTFamily<0>);
rmq_conformance!(bbst_3, BbSTFamily<3>);
rmq_conformance!(bbst_6, BbSTFamily<6>);
rmq_conformance!(bbst_14, BbSTFamily<14>);

rmq_conformance!(bbst2_3_1, BbST2Family<3, 1>);
rmq_conformance!(bbst2_6_3, BbST2Family<6, 3>);
rmq_conformance!(bbst2_8_8, BbST2Family<8, 8>);
rmq_conformance!(bbst2_14_6, BbST2Family<14, 6>);
