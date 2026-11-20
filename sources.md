# Sources

## Core hobbyist work
- **Chris Wellons (skeeto), "Prospecting for Hash Functions" (2018)** —
  https://nullprogram.com/blog/2018/07/31/ . Introduces the automated search
  and the avalanche-bias metric used throughout. Repo:
  https://github.com/skeeto/hash-prospector (lowbias32 ≈ 0.1735, triple32 ≈
  0.0209 ≈ random-permutation floor).
- **hash-prospector issue #19, "New best known functions" (TheIronBorn)** —
  https://github.com/skeeto/hash-prospector/issues/19 . 2-round 32-bit
  `[16 21f0aaad 15 d35a2d97 15]` bias ≈ 0.1076, later ≈ 0.10704. Method:
  metaheuristic on sampled estimate (2²⁵ inputs) constrained near good
  functions, then exhaustive exact search in the neighbourhood.
- **hash-prospector issue #28, "About the avalanche bias"** —
  https://github.com/skeeto/hash-prospector/issues/28 . The lowest-bias
  function is not necessarily best by RRC-PractRand / TFR. Motivates
  multi-objective evaluation.
- **David Stafford, "Better Bit Mixing — Improving on MurmurHash3's 64-bit
  Finalizer" (2011)** —
  https://zimbry.blogspot.com/2011/09/better-bit-mixing-improving-on.html .
  Mix01–Mix14; Mix13 (shifts 30/27/31, 0xbf58476d1ce4e5b9, 0x94d049bb133111eb)
  became SplitMix64's finalizer and Java's.
- **Pelle Evensen, "Mostly Mangling" blog** — http://mostlymangling.blogspot.com/
  - On the mixing functions in SplitMix, Murmur3 and Stafford (2018-07):
    http://mostlymangling.blogspot.com/2018/07/on-mixing-functions-in-fast-splittable.html
    (rrmxmx; introduces RRC testing).
  - Moremur (2019-12):
    https://mostlymangling.blogspot.com/2019/12/stronger-better-morer-moremur-better.html
  - NASAM (2020-01):
    http://mostlymangling.blogspot.com/2020/01/nasam-not-another-strange-acronym-mixer.html
    (passes RRC-64-42-TF2-0.94, fails RRC-64-44 after 13 TB).
  - RRC = feed counters (with gammas/strides) reversed, rotated and
    complemented through the mixer and run PractRand; a much harsher test
    than avalanche alone.
- **Jon Kågström (Maiga)** — mx3 (https://github.com/jonmaiga/mx3),
  "The construction of bit mixers" (https://jonkagstrom.com/bit-mixer-construction/),
  "Tuning bit mixers" (https://jonkagstrom.com/tuning-bit-mixers/index.html).
  Stack-machine program search for mixer *structures*, then constant tuning;
  mxmxmx mixers passing PractRand beyond 2⁴⁴.

## Academic / adoption
- **Webster & Tavares, "On the Design of S-Boxes", CRYPTO '85 (LNCS 218)** —
  Strict Avalanche Criterion; Bit Independence Criterion.
- **Steele & Vigna, "LXM: Better Splittable Pseudorandom Number Generators"** —
  https://vigna.di.unimi.it/ftp/papers/LXM.pdf . Compares Stafford, Lea and
  degski mixers as output functions.
- **Steele, Lea & Flood, "Fast Splittable Pseudorandom Number Generators"
  (OOPSLA 2014)** — SplitMix.
- **Apache Commons RNG MixFunctions / RNG-94** — production use of Stafford
  Mix13 and RotateRotateMultiplyXorMultiplyXor (rrmxmx).
- **Zig std.hash.int** — adopted skeeto's constants after finding the old
  default mixer very biased.
- **SMHasher (rurban) / SMHasher3 (Frank Wojcik)** — broader hash test suites.
- **CAFxX/mix-inst** — https://github.com/CAFxX/mix-inst : hardware
  (RISC-V instruction) finalizer search with a staged validation battery.

## Mathematics for candidate search (literature review, 2026-11-19)

These sources were selected against the corrected findings in `FINDINGS.md`,
particularly F3 (parity relations), F5 (lost dependencies), F6 (poor width
projections), and F10 (unproven low-bit-core hypothesis). They provide methods
and seed families, not a proof that particular constants beat the current record.
Applications below are proposed experiments, not reported results.

### 1. Constant multiplication with dependent carries: strongest modelling lead

**Nicky Mouha, Vesselin Velichkov, Christophe De Cannière and Bart Preneel,
“The Differential Analysis of S-Functions,” SAC 2010, proceedings 2011,
LNCS 6544, pp. 36–56.**
[Publication and abstract](https://www.research.ed.ac.uk/en/publications/the-differential-analysis-of-s-functions/);
[author-hosted full text in thesis](https://mouha.be/wp-content/uploads/phdthesis.pdf)
(printed pp. 121–142, especially Appendix D).

The paper computes differential probabilities using finite-state graphs and
transition matrices, including multiplication by a constant modulo 2^n.
Appendix D gives an instructive example: for multiplication by 3, the exact
probability is 2^-15 while modelling it as addition of independently distributed
operands gives 2^-25. The shifted copies of the same input are dependent.

**Application:** prototype a joint carry/value-state model at small widths,
validate against exact enumeration, then test whether retained state improves
ranking within multiplier families. This addresses F5 more directly than adding
more first-order marginal features. Matrix size and interleaved right shifts
may make the full 32/64-bit construction expensive; the paper's small-constant
examples do not establish a cheap full-width finalizer evaluator.

Related short precursor: **the same authors, “Toolkit for the Differential
Cryptanalysis of ARX-based Cryptographic Constructions” (2010)**,
[paper](https://cosicdatabase.esat.kuleuven.be/backend/publications/files/conferencepaper/1472).
It describes automatically generated/minimized matrices, including XOR
differentials of constant multiplication.

### 2. Autocorrelation / DLCT: strongest connection to our actual objective

**Anne Canteaut et al., “On the Differential-Linear Connectivity Table of
Vectorial Boolean Functions” (2019 preprint).**
[Paper](https://arxiv.org/abs/1908.07445).

Connects the differential-linear connectivity table (DLCT) with vectorial
autocorrelation, Walsh transforms and differential distribution tables, and
studies its behaviour under equivalences. It supplies the mathematical language
for correlations of output parities after an input difference.

**Application:** score selected output masks, including weight 2/3 and the
structurally forced masks from F3, rather than only individual output bits.
This can distinguish SAC improvement from merely moving a defect to another
parity. Do not use the worst unrestricted autocorrelation as the sole objective
within the present two-multiply family: F3 already forces magnitude 1 for some
nonzero input difference/output mask, irrespective of the constants.

### 3. Multiplication plus binary linear layers: candidate architectures

**Olivier Bronchain, Sebastian Faust, Virginie Lallemand, Gregor Leander,
Léo Perrin and François-Xavier Standaert, “MOE: Multiplication Operated
Encryption with Trojan Resilience,” ToSC 2021(1), pp. 78–129.**
[DOI](https://doi.org/10.46586/tosc.v2021.i1.78-129);
[institutional full text](https://dial.uclouvain.be/pr/boreal/object/boreal%3A254119/datastream/PDF_01/view).

Sections 4.5.1–4.5.2 discuss the deterministic top-bit differential of odd
multiplication modulo 2^m, and combining two multiplications with an intervening
binary matrix. This is unusually close to the project's operation family.

**Application:** jointly search the intervening linear map and the multipliers,
using cheap invertible rotate-XOR maps or other sparse binary maps as candidates.
Treat this as a separate architecture experiment from the fixed xorshift record.
The paper does not establish that a particular cheap map or two multiplies
suffice for our statistical goals. Verify invertibility and measure actual cost.

### 4. Spectral-test multiplier tables: easiest source of new seeds

**Guy L. Steele Jr. and Sebastiano Vigna, “Computationally easy, spectrally
good multipliers for congruential pseudorandom number generators,”
Software: Practice and Experience 52(2), pp. 443–458 (2022; preprint 2020).**
[Paper](https://arxiv.org/abs/2001.05304);
[authors' code and tables](https://github.com/vigna/CPRNG).

Provides multipliers selected for lattice quality across dimensions and lags,
including power-of-two moduli and different multiplier sizes.

**Application:** draw a separate seed population from their 32/64-bit tables,
pair constants with several shift triples, and compare against uniform odd
constants and existing mixer-derived seeds at equal evaluation cost. Spectral
quality concerns congruential sequences, not XOR avalanche of composed mixers;
transfer is a hypothesis. Do not impose their LCG/MCG residue restrictions on
all mixer candidates: invertible multiplication here only requires oddness.

### 5. Exact addition differentials: useful supporting mathematics

**Helger Lipmaa and Shiho Moriai, “Efficient Algorithms for Computing
Differential Properties of Addition,” FSE 2001.**
[Author-hosted paper](https://kodu.ut.ee/~lipmaa/papers/lm01/dpa.pdf).

Develops efficient algorithms for differential probabilities and good
differentials of modular addition.

**Application:** use as a reference for carry-transition implementations and
small-width checks. Decomposing multiplication into shifts and additions is
valid algebraically, but multiplying independent addition probabilities is not
valid when the addends share input bits. Use the S-function treatment above to
preserve those dependencies.

### 6. Boolean-function theory: broader scoring vocabulary

**Claude Carlet, “Boolean Functions for Cryptography and Coding Theory”
(2021), book rather than paper.**
[Author-hosted text](https://www.math.univ-paris13.fr/~carlet/book-fcts-Bool-vect-crypt-codes.pdf).

The propagation-criterion and global-avalanche sections explain SAC as PC(1),
higher-weight input differences, autocorrelation, and sum-of-squares indicators.

**Application:** select extra diagnostics deliberately: low-weight differences,
output parity masks, and behaviour under fixed input coordinates. Full global
tables are small-width tools; use restricted masks/sampling at 32/64 bits.
Finite-field APN or bent constructions are not drop-in recipes for odd integer
multipliers modulo 2^n.

### 7. Sequential statistics: retain promising candidates reliably

**Steven R. Howard, Aaditya Ramdas, Jon McAuliffe and Jasjeet Sekhon,
“Time-uniform, nonparametric, nonasymptotic confidence sequences,”
Annals of Statistics 49(2), pp. 1055–1080 (2021).**
[Paper](https://arxiv.org/abs/1810.08240).

Develops confidence intervals valid across repeated looks, directly relevant
to `racing.rs` and the review's false-elimination concerns.

**Application:** derive sequential bounds for the actual estimator before
replacing the current approximate Gaussian rule. The debiased squared-bias
estimate is not an ordinary mean of independent Bernoulli observations; its
quadratic structure and correlated SAC entries require an appropriate argument.
Coverage over time for one candidate also does not automatically cover every
adaptively proposed candidate. Benchmark power and cost against exact truths.

### A concrete mathematical reformulation (our derivation)

For uniform n-bit x, define normalized component autocorrelation

    C_F(a,b) = E_x (-1)^(b · (F(x XOR a) XOR F(x))),

where the dot product is over GF(2). Then C_F(e_i,e_j) = 1 - 2*p_ij,
so the project's exact squared score divided by 10^6 is precisely the mean
of C_F(e_i,e_j)^2. This identifies SAC as a small slice of the autocorrelation
table, rather than a complete measure of mixing.

Write a two-multiply mixer as F = L_out ◦ G ◦ L_in, with
G = M_m1 ◦ R_s1 ◦ M_m0 and the outer right-xorshifts represented by invertible
binary matrices. A uniform change of variable gives the exact identity

    C_F(a,b) = C_G(L_in a, L_out^T b).

Consequently, changing only the outer shifts selects different input
directions/output masks of the same nonlinear core. It can improve coordinate
SAC without removing its worst unrestricted parity defect.

**Proposed search experiment:** for a fixed (m0, s1, m1), cache the relevant
core correlations and evaluate several (s0,s2) choices together. At small widths
this can be exact; at full widths use only the masks needed by the permitted
outer shifts. Measure total runtime and sampling covariance before claiming
a saving. This is an algebraic proposal, not an algorithm benchmark from the
cited paper, and does not assume independence between rounds.

### Recommended experimental order

1. Try spectral-table seeds alongside uniform and existing-family controls;
   judge exact 32-bit finalists and improvement per wall-clock budget.
2. Test the outer-shift/core factorization at small width, then benchmark the
   restricted-mask version against direct evaluation at 32 bits.
3. Prototype the S-function joint-state screen; evaluate held-out families and
   recall of genuinely good candidates, not only overall rank correlation.
4. If broader quality is the target, run a separate linear-layer/three-round
   search with parity diagnostics and independent stream testing.

Keep the statistical validation in `results/REVIEW_STATUS.md` as a prerequisite
for interpreting noisy racing comparisons. None of these papers validates
freezing the low 16 multiplier bits or proves that small-width rankings transfer.
