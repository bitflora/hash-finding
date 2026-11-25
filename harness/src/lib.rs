//! mixlab: evaluation and search harness for multiply-xorshift integer mixers.
//!
//! The central metric is hash-prospector's avalanche bias:
//! for every (input bit j, output bit k) pair, count how often flipping input
//! bit j flips output bit k; `diff = (count - n/2) / (n/2)`;
//! `score = 1000 * sqrt(mean(diff^2))`.

pub mod bias;
pub mod exact;
pub mod extra;
pub mod fast32;
#[cfg(feature = "gpu")]
pub mod gpu;
pub mod mixers;
pub mod racing;
pub mod refs;
pub mod rng;
pub mod scale;
pub mod search;
pub mod surrogate;
