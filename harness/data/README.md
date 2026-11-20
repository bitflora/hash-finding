# Spectral seed provenance

`spectral.csv` transcribes the multiplier columns of Tables 4–7 of
Steele and Vigna, *Computationally Easy, Spectrally Good Multipliers for
Congruential Pseudorandom Number Generators*, arXiv:2001.05304v3.
Verified against [the versioned paper](https://arxiv.org/html/2001.05304v3)
on 2026-11-19. `bits` is the modulus exponent, not multiplier length.
The rows are factual numeric data; no spectral scores are used as mixer scores.

These are the compact published selections, not the authors' 14 GB candidate
database. Keep provenance when extending the table. Constants are never
truncated across widths. Their congruential-generator quality does not imply
good avalanche in a finalizer. The `matched` control matches multiplier lengths;
`random` samples the whole odd domain without imposing LCG residue restrictions.
