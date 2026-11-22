//! Exact score under hash-prospector issue #12's alternative metric
//! (camel-cdr): 1000 * mean over the 32x32 bins of |count - n/2| / n.
//! Random-function floor = 1000/sqrt(2*pi*n) = 0.006087 at n = 2^32.
//! `cargo run --release --example metric12 -- "[16 21f0aaad 15 735a2d97 15]" ...`
use mixlab::mixers::Xmx;
fn main() {
    for s in std::env::args().skip(1) {
        let x = Xmx::parse(32, &s).unwrap();
        let c = mixlab::fast32::exact_xmx(&x, true);
        let m = c.diffs().map(|d| d.abs() / 2.0).sum::<f64>() / c.bins.len() as f64;
        println!("{x}  issue12={:.17}  hp_bias={:.17}", 1000.0 * m, c.score());
    }
}
