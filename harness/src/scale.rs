//! Width scaling: project a mixer to a smaller word size so that it can be
//! scored exactly, and test whether the small-width ranking predicts the
//! full-width one.

use crate::mixers::{mask, Xmx};
use std::str::FromStr;

/// How shift amounts are mapped from width `b` to width `w`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShiftMap {
    /// s' = round(s * w / b)
    Prop,
    /// s' = w/2 + (s - b/2): keeps the offset from half-width
    /// (good mixers sit near half-width, so this preserves 15 vs 16 vs 17).
    Offset,
}

/// How multipliers are mapped from width `b` to width `w` (result forced odd).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultMap {
    /// m mod 2^w: the low bits, which alone govern the low output bits.
    Low,
    /// m >> (b - w): the leading bits (the "shape" of the constant).
    High,
}

impl FromStr for ShiftMap {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "prop" => Ok(ShiftMap::Prop),
            "offset" => Ok(ShiftMap::Offset),
            _ => Err(format!("unknown shift map {s} (prop|offset)")),
        }
    }
}

impl FromStr for MultMap {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "low" => Ok(MultMap::Low),
            "high" => Ok(MultMap::High),
            _ => Err(format!("unknown mult map {s} (low|high)")),
        }
    }
}

pub fn map_shift(s: u32, b: u32, w: u32, how: ShiftMap) -> u32 {
    let v = match how {
        ShiftMap::Prop => (s as f64 * w as f64 / b as f64).round() as i64,
        ShiftMap::Offset => (w / 2) as i64 + s as i64 - (b / 2) as i64,
    };
    v.clamp(1, w as i64 - 1) as u32
}

pub fn map_mult(m: u64, b: u32, w: u32, how: MultMap) -> u64 {
    let v = match how {
        MultMap::Low => m & mask(w),
        MultMap::High => (m & mask(b)) >> (b - w),
    };
    v | 1
}

pub fn project(x: &Xmx, w: u32, sm: ShiftMap, mm: MultMap) -> Xmx {
    assert!(w <= x.bits && w >= 3);
    Xmx {
        bits: w,
        shifts: x.shifts.iter().map(|&s| map_shift(s, x.bits, w, sm)).collect(),
        mults: x.mults.iter().map(|&m| map_mult(m, x.bits, w, mm)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projections() {
        let x = Xmx::parse(32, "[16 21f0aaad 15 735a2d97 15]").unwrap();
        let p = project(&x, 16, ShiftMap::Offset, MultMap::Low);
        assert_eq!(p.to_string(), Xmx::parse(16, "[8 aaad 7 2d97 7]").unwrap().to_string());
        let p = project(&x, 16, ShiftMap::Prop, MultMap::High);
        assert_eq!(p.to_string(), Xmx::parse(16, "[8 21f1 8 735b 8]").unwrap().to_string());
    }
}
