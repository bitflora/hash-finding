//! Avalanche counts on an NVIDIA GPU (feature `gpu`): exact over all inputs,
//! or sampled with the same inputs as `bias::sampled`. The kernel
//! (`gpu_kernel.cu`) mirrors fast32's Harley-Seal counter and is compiled at
//! runtime with NVRTC, so the CUDA Toolkit must be installed and its NVRTC
//! DLL on PATH. Counts are exact integers and must equal fast32's bit for
//! bit (see tests/gpu.rs). Callers normally go through `crate::exact`, which
//! picks the backend.

use crate::bias::{Counts, SAMPLE_CHUNK};
use crate::mixers::{mask, Xmx};
use cudarc::driver::{CudaContext, CudaFunction, CudaStream, LaunchConfig, PushKernelArg};
use std::sync::{Arc, Mutex, OnceLock};

const SRC: &str = include_str!("gpu_kernel.cu");
/// Inputs per thread (PER_THREAD in the kernel).
const PER_THREAD: u64 = 256;
/// Input bits per thread (JB in the kernel).
const JB: u32 = 8;
const BLOCK: u32 = 256;
/// Inputs (summed over candidates) per launch: an exact 32-bit score takes 64
/// launches, keeping each well under the Windows display-driver watchdog
/// (TDR, ~2 s).
const LAUNCH_INPUTS: u64 = 1 << 26;

pub struct Gpu {
    stream: Arc<CudaStream>,
    /// [rounds-1][full as usize]
    exact: Vec<[CudaFunction; 2]>,
    sampled: Vec<[CudaFunction; 2]>,
    prep: CudaFunction,
    name: String,
}

fn err<E: std::fmt::Debug>(what: &'static str) -> impl Fn(E) -> String {
    move |e| format!("{what}: {e:?}")
}

fn check(x: &Xmx) -> Result<(), String> {
    if !(8..=32).contains(&x.bits) {
        return Err(format!("gpu path needs 8..=32 bits, got {}", x.bits));
    }
    if !(1..=4).contains(&x.rounds()) {
        return Err(format!("gpu path needs 1..=4 rounds, got {}", x.rounds()));
    }
    Ok(())
}

/// Whether the GPU kernels accept this mixer.
pub fn supports(x: &Xmx) -> bool {
    check(x).is_ok()
}

/// [mask, s0..s4, m0..m3] as the kernels take them.
fn params(x: &Xmx) -> [u32; 10] {
    let k = x.kernel();
    let mut p = [0u32; 10];
    p[0] = mask(x.bits) as u32;
    p[1..6].copy_from_slice(&k.s);
    for i in 0..4 {
        p[6 + i] = k.m[i] as u32;
    }
    p
}

impl Gpu {
    pub fn new() -> Result<Gpu, String> {
        // cudarc panics when it can't find a CUDA DLL; report that as an error
        // (without the panic message, since the caller may fall back to the CPU).
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let r = std::panic::catch_unwind(Self::init);
        std::panic::set_hook(hook);
        r.unwrap_or_else(|_| {
            Err("CUDA driver or NVRTC not found: needs an NVIDIA driver and the CUDA 13.x Toolkit with its bin\\x64 (or bin) on PATH".into())
        })
    }

    fn init() -> Result<Gpu, String> {
        let ctx = CudaContext::new(0).map_err(err("CUDA init"))?;
        let name = ctx.name().unwrap_or_else(|_| "unknown GPU".into());
        let (maj, min) = ctx.compute_capability().map_err(err("compute capability"))?;
        let arch: &'static str = Box::leak(format!("compute_{maj}{min}").into_boxed_str());
        let opts = cudarc::nvrtc::CompileOptions { arch: Some(arch), options: vec!["-std=c++17".into()], ..Default::default() };
        let ptx = cudarc::nvrtc::compile_ptx_with_opts(SRC, opts).map_err(err("NVRTC"))?;
        let module = ctx.load_module(ptx).map_err(err("load module"))?;
        let f = |name: String| module.load_function(&name).map_err(err("load function"));
        let mut exact = Vec::new();
        let mut sampled = Vec::new();
        for r in 1..=4 {
            exact.push([f(format!("exact_r{r}_mask"))?, f(format!("exact_r{r}_full"))?]);
            sampled.push([f(format!("sampled_r{r}_mask"))?, f(format!("sampled_r{r}_full"))?]);
        }
        let prep = f("sample_prep".into())?;
        Ok(Gpu { stream: ctx.default_stream(), exact, sampled, prep, name })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Exact counts over all 2^bits inputs (8 <= bits <= 32, 1..=4 rounds).
    pub fn exact_xmx(&self, x: &Xmx) -> Result<Counts, String> {
        self.counts_range(x, 0, 1u64 << x.bits)
    }

    /// Exact counts over inputs [lo, hi); lo and hi must be multiples of 256.
    pub fn counts_range(&self, x: &Xmx, lo: u64, hi: u64) -> Result<Counts, String> {
        check(x)?;
        let bits = x.bits;
        if lo % PER_THREAD != 0 || hi % PER_THREAD != 0 || lo > hi || hi > 1u64 << bits {
            return Err(format!("bad input range [{lo}, {hi})"));
        }
        let p = params(x);
        let f = &self.exact[x.rounds() - 1][(bits == 32) as usize];
        let mut out = self.stream.alloc_zeros::<u64>((bits * bits) as usize).map_err(err("alloc"))?;
        let mut base = lo;
        while base < hi {
            let end = hi.min(base + LAUNCH_INPUTS);
            let threads = (end - base) / PER_THREAD;
            let block = BLOCK.min(threads as u32);
            let cfg = LaunchConfig {
                grid_dim: (threads.div_ceil(block as u64) as u32, bits.div_ceil(JB), 1),
                block_dim: (block, 1, 1),
                shared_mem_bytes: 0,
            };
            let mut b = self.stream.launch_builder(f);
            b.arg(&bits);
            for v in &p {
                b.arg(v);
            }
            b.arg(&mut out).arg(&base).arg(&end);
            unsafe { b.launch(cfg) }.map_err(err("launch"))?;
            base = end;
        }
        let bins = self.stream.clone_dtoh(&out).map_err(err("copy"))?;
        Ok(Counts { bits, n: hi - lo, bins })
    }

    /// Sampled counts for several mixers at once: mixer i gets exactly the
    /// counts of `bias::sampled(f_i, bits, n, seeds[i])`. All mixers must share
    /// bits and rounds.
    pub fn sampled_batch(&self, xs: &[Xmx], n: u64, seeds: &[u64]) -> Result<Vec<Counts>, String> {
        assert_eq!(xs.len(), seeds.len());
        let Some(x0) = xs.first() else { return Ok(vec![]) };
        let (bits, rounds) = (x0.bits, x0.rounds());
        for x in xs {
            check(x)?;
            if x.bits != bits || x.rounds() != rounds {
                return Err("sampled_batch needs equal bits and rounds".into());
            }
        }
        let nc = xs.len() as u64;
        if n == 0 {
            return Ok(xs.iter().map(|_| Counts::new(bits)).collect());
        }
        let nb = (bits * bits) as usize;
        let ps: Vec<u32> = xs.iter().flat_map(params).collect();
        let s = &self.stream;
        let params_d = s.clone_htod(&ps).map_err(err("copy"))?;
        let seeds_d = s.clone_htod(seeds).map_err(err("copy"))?;
        let mut out = s.alloc_zeros::<u64>(nb * xs.len()).map_err(err("alloc"))?;
        // positions per candidate per launch: a multiple of SAMPLE_CHUNK
        let span_max = ((LAUNCH_INPUTS / nc) / SAMPLE_CHUNK).max(1) * SAMPLE_CHUNK;
        let total = n.div_ceil(SAMPLE_CHUNK) * SAMPLE_CHUNK;
        let span_max = span_max.min(total);
        let mut states = s.alloc_zeros::<u64>((nc * span_max / PER_THREAD * 4) as usize).map_err(err("alloc"))?;
        let f = &self.sampled[rounds - 1][(bits == 32) as usize];
        let mut pos0 = 0u64;
        while pos0 < n {
            let span = span_max.min(total - pos0);
            let chunks = span / SAMPLE_CHUNK;
            let pb = 64u32.min(chunks as u32);
            let cfg = LaunchConfig {
                grid_dim: (chunks.div_ceil(pb as u64) as u32, nc as u32, 1),
                block_dim: (pb, 1, 1),
                shared_mem_bytes: 0,
            };
            let mut b = s.launch_builder(&self.prep);
            b.arg(&seeds_d).arg(&n).arg(&pos0).arg(&span).arg(&mut states);
            unsafe { b.launch(cfg) }.map_err(err("launch prep"))?;

            let threads = span / PER_THREAD;
            let block = BLOCK.min(threads as u32);
            let cfg = LaunchConfig {
                grid_dim: (threads.div_ceil(block as u64) as u32, bits.div_ceil(JB), nc as u32),
                block_dim: (block, 1, 1),
                shared_mem_bytes: 0,
            };
            let mut b = s.launch_builder(f);
            b.arg(&bits).arg(&n).arg(&pos0).arg(&span).arg(&params_d).arg(&states).arg(&mut out);
            unsafe { b.launch(cfg) }.map_err(err("launch sampled"))?;
            pos0 += span;
        }
        let bins = s.clone_dtoh(&out).map_err(err("copy"))?;
        Ok(bins.chunks(nb).map(|c| Counts { bits, n, bins: c.to_vec() }).collect())
    }
}

/// Process-wide GPU handle, created on first use.
pub fn global() -> Result<&'static Mutex<Gpu>, String> {
    static G: OnceLock<Result<Mutex<Gpu>, String>> = OnceLock::new();
    G.get_or_init(|| Gpu::new().map(Mutex::new)).as_ref().map_err(|e| e.clone())
}
