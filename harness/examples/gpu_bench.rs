//! Time an exact-count kernel variant against the production GPU kernel.
//!
//! `gpu_bench KERNEL.cu SPEC [--log2 L] [--reps N] [--name exact_r3_full]
//!   [--per-thread 256] [--jb 8] [--block 256] [--launch-log2 26] [--ptx OUT.ptx] [--pair]`
//!
//! `--pair`: the variant has the pair-kernel signature `(s0..s4, m0..m3, out,
//! base, end)` (no bits or mask) and must be run over the full 2^32 domain.
//!
//! The variant must export a kernel with the production signature
//! `(bits, mask, s0..s4, m0..m3, u64* out, base, end)`. Its counts over
//! inputs [0, 2^L) must equal `gpu::Gpu::counts_range` bit for bit; the run
//! fails otherwise. Timings are the best of N reps, synchronised.
use cudarc::driver::{CudaContext, LaunchConfig, PushKernelArg};
use mixlab::mixers::{mask, Xmx};
use std::time::Instant;

fn arg(args: &[String], k: &str, d: u64) -> u64 {
    args.iter().position(|a| a == k).map_or(d, |i| args[i + 1].parse().unwrap())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let src = std::fs::read_to_string(&args[1]).unwrap();
    let x = Xmx::parse(32, &args[2]).unwrap();
    let lg = arg(&args, "--log2", 32) as u32;
    let reps = arg(&args, "--reps", 3);
    let per_thread = arg(&args, "--per-thread", 256);
    let jb = arg(&args, "--jb", 8) as u32;
    let block = arg(&args, "--block", 256) as u32;
    let launch = 1u64 << arg(&args, "--launch-log2", 26);
    let name = args.iter().position(|a| a == "--name").map_or("exact_r3_full".to_string(), |i| args[i + 1].clone());
    let bits = x.bits;
    let pair = args.iter().any(|a| a == "--pair");
    let (lo, hi) = (0u64, 1u64 << lg);

    // Production reference (and its timing).
    let g = mixlab::gpu::Gpu::new().unwrap();
    let mut t_ref = f64::MAX;
    let mut want = None;
    for _ in 0..reps {
        let t = Instant::now();
        // the production path: the pair kernel over the full 32-bit domain
        let c = if lo == 0 && hi == 1 << 32 { g.exact_xmx(&x).unwrap() } else { g.counts_range(&x, lo, hi).unwrap() };
        t_ref = t_ref.min(t.elapsed().as_secs_f64());
        want = Some(c.bins);
    }
    let want = want.unwrap();

    let ctx = CudaContext::new(0).unwrap();
    let (maj, min) = ctx.compute_capability().unwrap();
    let arch: &'static str = Box::leak(format!("compute_{maj}{min}").into_boxed_str());
    let opts = cudarc::nvrtc::CompileOptions { arch: Some(arch), options: vec!["-std=c++17".into()], ..Default::default() };
    let ptx = cudarc::nvrtc::compile_ptx_with_opts(&src, opts).unwrap();
    if let Some(i) = args.iter().position(|a| a == "--ptx") {
        std::fs::write(&args[i + 1], ptx.to_src()).unwrap();
    }
    let module = ctx.load_module(ptx).unwrap();
    let f = module.load_function(&name).unwrap();
    let stream = ctx.default_stream();
    let k = x.kernel();
    let mut p = [0u32; 10];
    p[0] = mask(bits) as u32;
    p[1..6].copy_from_slice(&k.s);
    for i in 0..4 {
        p[6 + i] = k.m[i] as u32;
    }
    let mut t_new = f64::MAX;
    let mut got = vec![];
    for _ in 0..reps {
        let mut out = stream.alloc_zeros::<u64>((bits * bits) as usize).unwrap();
        stream.synchronize().unwrap();
        let t = Instant::now();
        let mut base = lo;
        while base < hi {
            let end = hi.min(base + launch);
            let threads = (end - base) / per_thread;
            let b = block.min(threads as u32);
            let cfg = LaunchConfig {
                grid_dim: (threads.div_ceil(b as u64) as u32, bits.div_ceil(jb), 1),
                block_dim: (b, 1, 1),
                shared_mem_bytes: 0,
            };
            let mut l = stream.launch_builder(&f);
            if !pair {
                l.arg(&bits).arg(&p[0]);
            }
            for v in &p[1..] {
                l.arg(v);
            }
            l.arg(&mut out).arg(&base).arg(&end);
            unsafe { l.launch(cfg) }.unwrap();
            base = end;
        }
        got = stream.clone_dtoh(&out).unwrap();
        t_new = t_new.min(t.elapsed().as_secs_f64());
    }
    let ok = got == want;
    println!(
        "{x} 2^{lg}: production {t_ref:.4} s, variant {t_new:.4} s, speedup x{:.3}, counts {}",
        t_ref / t_new,
        if ok { "match" } else { "DIFFER" }
    );
    if !ok {
        std::process::exit(1);
    }
}
