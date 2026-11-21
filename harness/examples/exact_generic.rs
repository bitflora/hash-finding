//! Independent exact check via the generic (closure-based, non-SIMD) counter,
//! the path validated against hash-prospector's published 32-bit values.
//! `cargo run --release --example exact_generic -- "[16 9c013335 15 335a2d97 15]" ...`
use mixlab::mixers::Xmx;
use mixlab::with_mixer;
fn main() {
    for s in std::env::args().skip(1) {
        let x = Xmx::parse(32, &s).unwrap();
        let t = std::time::Instant::now();
        let c = with_mixer!(x, f => mixlab::bias::exact(&f, 32));
        println!("{x}  generic exact bias={:.17}  n={}  [{:.0}s]", c.score(), c.n, t.elapsed().as_secs_f64());
    }
}
