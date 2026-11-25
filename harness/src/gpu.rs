//! Exact avalanche counts on an NVIDIA GPU (feature `gpu`). The kernel
//! (`gpu_kernel.cu`) mirrors fast32's Harley-Seal counter and is compiled at
//! runtime with NVRTC, so the CUDA Toolkit must be installed and its NVRTC
//! DLL on PATH. Counts are exact integers and must equal
//! `fast32::exact_xmx` bit for bit (see tests/gpu.rs).

use crate::bias::Counts;
use crate::mixers::{mask, Xmx};
use cudarc::driver::{CudaContext, CudaFunction, CudaStream, LaunchConfig, PushKernelArg};
use std::sync::{Arc, Mutex, OnceLock};

const SRC: &str = include_str!("gpu_kernel.cu");
/// Inputs per thread (G * NB in the kernel).
const PER_THREAD: u64 = 256;
/// Input bits per thread (JB in the kernel).
const JB: u32 = 8;
const BLOCK: u32 = 256;
/// Inputs per launch: 2^32 inputs take 64 launches, keeping each well under
/// the Windows display-driver watchdog (TDR, ~2 s).
const LAUNCH_INPUTS: u64 = 1 << 26;

pub struct Gpu {
    stream: Arc<CudaStream>,
    /// [rounds-1][full as usize]
    funcs: Vec<[CudaFunction; 2]>,
    name: String,
}

impl Gpu {
    pub fn new() -> Result<Gpu, String> {
        let ctx = CudaContext::new(0).map_err(|e| format!("CUDA init: {e:?}"))?;
        let name = ctx.name().unwrap_or_else(|_| "unknown GPU".into());
        let (maj, min) = ctx.compute_capability().map_err(|e| format!("{e:?}"))?;
        let arch: &'static str = Box::leak(format!("compute_{maj}{min}").into_boxed_str());
        let opts = cudarc::nvrtc::CompileOptions { arch: Some(arch), options: vec!["-std=c++17".into()], ..Default::default() };
        // cudarc panics when it can't find the NVRTC DLL; report that as an error.
        let ptx = std::panic::catch_unwind(|| cudarc::nvrtc::compile_ptx_with_opts(SRC, opts))
            .map_err(|_| "NVRTC not found: install the CUDA 13.x Toolkit and put its bin\\x64 (or bin) on PATH".to_string())?
            .map_err(|e| format!("NVRTC: {e:?}"))?;
        let module = ctx.load_module(ptx).map_err(|e| format!("load module: {e:?}"))?;
        let mut funcs = Vec::new();
        for r in 1..=4 {
            let f = |kind: &str| module.load_function(&format!("exact_r{r}_{kind}")).map_err(|e| format!("{e:?}"));
            funcs.push([f("mask")?, f("full")?]);
        }
        Ok(Gpu { stream: ctx.default_stream(), funcs, name })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Exact counts over all 2^bits inputs (8 <= bits <= 32, 1..=4 rounds).
    pub fn exact_xmx(&self, x: &Xmx) -> Result<Counts, String> {
        self.counts_range(x, 0, 1u64 << x.bits)
    }

    /// Counts over inputs [lo, hi); lo and hi must be multiples of 256.
    pub fn counts_range(&self, x: &Xmx, lo: u64, hi: u64) -> Result<Counts, String> {
        let bits = x.bits;
        if !(8..=32).contains(&bits) {
            return Err(format!("gpu path needs 8..=32 bits, got {bits}"));
        }
        let r = x.rounds();
        if !(1..=4).contains(&r) {
            return Err(format!("gpu path needs 1..=4 rounds, got {r}"));
        }
        if lo % PER_THREAD != 0 || hi % PER_THREAD != 0 || lo > hi || hi > 1u64 << bits {
            return Err(format!("bad input range [{lo}, {hi})"));
        }
        let k = x.kernel();
        let msk = mask(bits) as u32;
        let s: Vec<u32> = k.s.to_vec();
        let m: Vec<u32> = k.m.iter().map(|&v| v as u32).collect();
        let f = &self.funcs[r - 1][(bits == 32) as usize];
        let nb = (bits * bits) as usize;
        let mut out = self.stream.alloc_zeros::<u64>(nb).map_err(|e| format!("{e:?}"))?;
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
            b.arg(&bits).arg(&msk);
            for v in &s {
                b.arg(v);
            }
            for v in &m {
                b.arg(v);
            }
            b.arg(&mut out).arg(&base).arg(&end);
            unsafe { b.launch(cfg) }.map_err(|e| format!("launch: {e:?}"))?;
            base = end;
        }
        let bins = self.stream.clone_dtoh(&out).map_err(|e| format!("{e:?}"))?;
        Ok(Counts { bits, n: hi - lo, bins })
    }
}

/// Process-wide GPU handle, created on first use.
pub fn global() -> Result<&'static Mutex<Gpu>, String> {
    static G: OnceLock<Result<Mutex<Gpu>, String>> = OnceLock::new();
    G.get_or_init(|| Gpu::new().map(Mutex::new)).as_ref().map_err(|e| e.clone())
}

/// Exact counts on the shared GPU handle.
pub fn exact_xmx(x: &Xmx) -> Result<Counts, String> {
    global()?.lock().unwrap().exact_xmx(x)
}
