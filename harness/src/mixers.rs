//! Parametrised xorshift-multiply mixers ("xmx" chains) of width 2..=64 bits.
//!
//! A mixer with R rounds is
//!   x ^= x >> s0; x *= m0; x ^= x >> s1; x *= m1; ... ; x ^= x >> sR
//! (all arithmetic mod 2^bits). Notation follows hash-prospector:
//!   [s0 m0 s1 m1 s2]   e.g. [16 21f0aaad 15 735a2d97 15]

use std::fmt;

pub const MAX_ROUNDS: usize = 4;

#[inline]
pub fn mask(bits: u32) -> u64 {
    if bits >= 64 {
        !0
    } else {
        (1u64 << bits) - 1
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Xmx {
    pub bits: u32,
    pub shifts: Vec<u32>,
    pub mults: Vec<u64>,
}

/// Fixed-size copy of an `Xmx` for hot loops.
#[derive(Clone, Copy, Debug)]
pub struct Kernel {
    pub mask: u64,
    pub s: [u32; MAX_ROUNDS + 1],
    pub m: [u64; MAX_ROUNDS],
}

#[inline(always)]
pub fn kapply<const R: usize>(k: &Kernel, mut x: u64) -> u64 {
    let mut i = 0;
    while i < R {
        x ^= x >> k.s[i];
        x = x.wrapping_mul(k.m[i]) & k.mask;
        i += 1;
    }
    x ^ (x >> k.s[R])
}

/// Bind `$f` to a fully-inlined closure `Fn(u64) -> u64` for the given `Xmx`
/// and evaluate `$body` (monomorphised per round count).
#[macro_export]
macro_rules! with_mixer {
    ($x:expr, $f:ident => $body:expr) => {{
        let __k = $x.kernel();
        match $x.rounds() {
            1 => {
                let $f = move |v: u64| $crate::mixers::kapply::<1>(&__k, v);
                $body
            }
            2 => {
                let $f = move |v: u64| $crate::mixers::kapply::<2>(&__k, v);
                $body
            }
            3 => {
                let $f = move |v: u64| $crate::mixers::kapply::<3>(&__k, v);
                $body
            }
            4 => {
                let $f = move |v: u64| $crate::mixers::kapply::<4>(&__k, v);
                $body
            }
            r => panic!("unsupported round count {}", r),
        }
    }};
}

impl Xmx {
    pub fn new(bits: u32, shifts: Vec<u32>, mults: Vec<u64>) -> Result<Self, String> {
        if !(2..=64).contains(&bits) {
            return Err(format!("bits must be 2..=64, got {bits}"));
        }
        if mults.is_empty() || mults.len() > MAX_ROUNDS || shifts.len() != mults.len() + 1 {
            return Err("need 1..=4 multipliers and one more shift than multipliers".into());
        }
        for &s in &shifts {
            if s == 0 || s >= bits {
                return Err(format!("shift {s} out of range 1..{bits}"));
            }
        }
        for &m in &mults {
            if m & 1 == 0 || m & !mask(bits) != 0 {
                return Err(format!("multiplier {m:#x} must be odd and fit in {bits} bits"));
            }
        }
        Ok(Xmx { bits, shifts, mults })
    }

    /// Parse "[16 21f0aaad 15 735a2d97 15]" (brackets/commas optional,
    /// multipliers hex with optional 0x, shifts decimal).
    pub fn parse(bits: u32, spec: &str) -> Result<Self, String> {
        let cleaned: String = spec
            .chars()
            .map(|c| if c == '[' || c == ']' || c == ',' { ' ' } else { c })
            .collect();
        let toks: Vec<&str> = cleaned.split_whitespace().collect();
        let mut shifts = vec![];
        let mut mults = vec![];
        for (i, t) in toks.iter().enumerate() {
            if i % 2 == 0 {
                shifts.push(t.parse::<u32>().map_err(|e| format!("bad shift {t}: {e}"))?);
            } else {
                let h = t.trim_start_matches("0x").trim_start_matches("0X");
                mults.push(u64::from_str_radix(h, 16).map_err(|e| format!("bad mult {t}: {e}"))?);
            }
        }
        Xmx::new(bits, shifts, mults)
    }

    pub fn rounds(&self) -> usize {
        self.mults.len()
    }

    pub fn kernel(&self) -> Kernel {
        let mut s = [0u32; MAX_ROUNDS + 1];
        let mut m = [0u64; MAX_ROUNDS];
        s[..self.shifts.len()].copy_from_slice(&self.shifts);
        m[..self.mults.len()].copy_from_slice(&self.mults);
        Kernel { mask: mask(self.bits), s, m }
    }

    /// Slow generic evaluation (use `with_mixer!` in hot loops).
    pub fn apply(&self, x: u64) -> u64 {
        let msk = mask(self.bits);
        let mut x = x & msk;
        for i in 0..self.mults.len() {
            x ^= x >> self.shifts[i];
            x = x.wrapping_mul(self.mults[i]) & msk;
        }
        x ^ (x >> self.shifts[self.mults.len()])
    }

    /// Exact inverse (proves bijectivity in tests).
    pub fn invert(&self, y: u64) -> u64 {
        let msk = mask(self.bits);
        let mut x = y & msk;
        let r = self.mults.len();
        x = unxorshift(x, self.shifts[r], self.bits);
        for i in (0..r).rev() {
            x = x.wrapping_mul(mul_inverse(self.mults[i])) & msk;
            x = unxorshift(x, self.shifts[i], self.bits);
        }
        x
    }
}

/// Inverse of y = x ^ (x >> s) on `bits`-bit words.
pub fn unxorshift(y: u64, s: u32, bits: u32) -> u64 {
    let mut x = y;
    let mut covered = s;
    while covered < bits {
        x = y ^ (x >> s);
        covered += s;
    }
    x & mask(bits)
}

/// Multiplicative inverse of odd `a` mod 2^64 (Newton iteration).
pub fn mul_inverse(a: u64) -> u64 {
    let mut x = a; // correct to 3 bits
    for _ in 0..5 {
        x = x.wrapping_mul(2u64.wrapping_sub(a.wrapping_mul(x)));
    }
    x
}

impl fmt::Display for Xmx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let w = ((self.bits + 3) / 4) as usize;
        write!(f, "[")?;
        for i in 0..self.mults.len() {
            write!(f, "{} {:0w$x} ", self.shifts[i], self.mults[i], w = w)?;
        }
        write!(f, "{}]", self.shifts[self.mults.len()])
    }
}
