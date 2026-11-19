//! Reference mixers from the literature (constants verified against primary
//! sources on 2026-11-18; see ../sources.md).

use crate::mixers::Xmx;

#[derive(Clone)]
pub enum Kind {
    Xmx(Xmx),
    Custom(fn(u64) -> u64),
}

#[derive(Clone)]
pub struct Reference {
    pub name: &'static str,
    pub bits: u32,
    pub kind: Kind,
    /// Published exact bias if known (32/16-bit, hash-prospector metric).
    pub published: Option<f64>,
    pub source: &'static str,
}

fn x(bits: u32, spec: &str) -> Kind {
    Kind::Xmx(Xmx::parse(bits, spec).expect("bad reference spec"))
}

fn rrmxmx(mut v: u64) -> u64 {
    const M: u64 = 0x9fb21c651e98df25;
    v ^= v.rotate_right(49) ^ v.rotate_right(24);
    v = v.wrapping_mul(M);
    v ^= v >> 28;
    v = v.wrapping_mul(M);
    v ^ (v >> 28)
}

fn nasam(mut x: u64) -> u64 {
    x ^= x.rotate_right(25) ^ x.rotate_right(47);
    x = x.wrapping_mul(0x9e6c63d0676a9a99);
    x ^= (x >> 23) ^ (x >> 51);
    x = x.wrapping_mul(0x9e6d62d06f6a9a9b);
    x ^ (x >> 23) ^ (x >> 51)
}

pub fn all() -> Vec<Reference> {
    vec![
        Reference { name: "hash16_xm2", bits: 16, kind: x(16, "[8 88b5 7 db2d 9]"), published: Some(0.0085905051336723701), source: "hash-prospector README" },
        Reference { name: "ironborn16", bits: 16, kind: x(16, "[8 a3d3 7 4b2d 9]"), published: Some(0.007252938393705358), source: "hash-prospector #19" },
        Reference { name: "murmur3_fmix32", bits: 32, kind: x(32, "[16 85ebca6b 13 c2b2ae35 16]"), published: None, source: "MurmurHash3" },
        Reference { name: "prospector32", bits: 32, kind: x(32, "[15 2c1b3c6d 12 297a2d39 15]"), published: Some(0.34968228323361017), source: "hash-prospector README" },
        Reference { name: "lowbias32", bits: 32, kind: x(32, "[16 7feb352d 15 846ca68b 16]"), published: Some(0.17353355999581582), source: "hash-prospector README" },
        Reference { name: "ironborn_1076", bits: 32, kind: x(32, "[16 21f0aaad 15 d35a2d97 15]"), published: Some(0.10760229515479501), source: "hash-prospector #19" },
        Reference { name: "ironborn_10704", bits: 32, kind: x(32, "[16 21f0aaad 15 735a2d97 15]"), published: Some(0.10704308166917044), source: "hash-prospector #19" },
        Reference { name: "triple32", bits: 32, kind: x(32, "[17 ed5ad4bb 11 ac4c1b51 15 31848bab 14]"), published: Some(0.020888578919738908), source: "hash-prospector README" },
        Reference { name: "murmur3_fmix64", bits: 64, kind: x(64, "[33 ff51afd7ed558ccd 33 c4ceb9fe1a85ec53 33]"), published: None, source: "MurmurHash3" },
        Reference { name: "stafford_mix13", bits: 64, kind: x(64, "[30 bf58476d1ce4e5b9 27 94d049bb133111eb 31]"), published: None, source: "Stafford 2011 / SplitMix64" },
        Reference { name: "moremur", bits: 64, kind: x(64, "[27 3c79ac492ba7b653 33 1c69b3f74ac4ae35 27]"), published: None, source: "Evensen 2019" },
        Reference { name: "mx3", bits: 64, kind: x(64, "[32 bea225f9eb34556d 29 bea225f9eb34556d 32 bea225f9eb34556d 29]"), published: None, source: "mx3 v3 (Maiga)" },
        Reference { name: "rrmxmx", bits: 64, kind: Kind::Custom(rrmxmx), published: None, source: "Evensen 2018" },
        Reference { name: "nasam", bits: 64, kind: Kind::Custom(nasam), published: None, source: "Evensen 2020" },
    ]
}

pub fn by_name(name: &str) -> Option<Reference> {
    all().into_iter().find(|r| r.name == name)
}
