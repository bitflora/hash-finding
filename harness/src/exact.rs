//! Backend choice for xmx avalanche counts. Every exact score in the harness
//! (eval, exact lists, race and search verification, traces) goes through
//! `exact_xmx`, and race sampling through `sampled_batch`, so one switch moves
//! them all between the CPU (`fast32`) and the GPU (`gpu`, feature `gpu`).
//! Both backends give identical counts. Mixers the GPU kernels can't take
//! (fewer than 8 or more than 32 bits) always stay on the CPU.
//!
//! The library defaults to the CPU; the binaries select `Backend::Auto`
//! unless told `--cpu` or `--gpu`.

use crate::bias::Counts;
use crate::mixers::Xmx;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Backend {
    /// Always the CPU.
    Cpu = 0,
    /// The GPU when it can be initialised (on first use); otherwise warn once
    /// and use the CPU.
    Auto = 1,
    /// The GPU; `set_backend` fails if it can't be initialised.
    Gpu = 2,
}

static MODE: AtomicU8 = AtomicU8::new(Backend::Cpu as u8);

/// Select the backend. `Gpu` initialises CUDA immediately so a missing GPU or
/// toolkit is reported up front; returns the device name when it does.
pub fn set_backend(b: Backend) -> Result<Option<String>, String> {
    let name = match b {
        Backend::Gpu => Some(gpu_name()?),
        _ => None,
    };
    MODE.store(b as u8, Ordering::Relaxed);
    Ok(name)
}

pub fn backend() -> Backend {
    match MODE.load(Ordering::Relaxed) {
        0 => Backend::Cpu,
        1 => Backend::Auto,
        _ => Backend::Gpu,
    }
}

#[cfg(feature = "gpu")]
fn gpu_name() -> Result<String, String> {
    Ok(crate::gpu::global()?.lock().unwrap().name().to_string())
}

#[cfg(not(feature = "gpu"))]
fn gpu_name() -> Result<String, String> {
    Err("this build has no GPU support (rebuild with the default `gpu` feature)".into())
}

/// Whether scores will run on the GPU (initialising it in `Auto` mode).
pub fn gpu_active() -> bool {
    #[cfg(feature = "gpu")]
    {
        gpu().is_some()
    }
    #[cfg(not(feature = "gpu"))]
    false
}

#[cfg(feature = "gpu")]
fn gpu() -> Option<&'static std::sync::Mutex<crate::gpu::Gpu>> {
    static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    match backend() {
        Backend::Cpu => None,
        b => match crate::gpu::global() {
            Ok(g) => Some(g),
            Err(e) if b == Backend::Gpu => panic!("GPU unavailable: {e}"),
            Err(e) => {
                if !WARNED.swap(true, Ordering::Relaxed) {
                    eprintln!("note: GPU unavailable, scoring on the CPU ({e}); pass --cpu to silence");
                }
                None
            }
        },
    }
}

/// Exact counts for an xmx mixer with 3 <= bits <= 32. `parallel` only
/// affects the CPU path (the GPU is always fully used).
pub fn exact_xmx(x: &Xmx, parallel: bool) -> Counts {
    #[cfg(feature = "gpu")]
    if crate::gpu::supports(x) {
        if let Some(g) = gpu() {
            return g.lock().unwrap().exact_xmx(x).unwrap_or_else(|e| panic!("gpu exact scoring of {x} failed: {e}"));
        }
    }
    crate::fast32::exact_xmx(x, parallel)
}

/// Sampled counts for several mixers with a common sample size: entry i is
/// `bias::sampled(f_i, bits, n, seeds[i])`. On the GPU, mixers with the same
/// width and round count share one batch of launches.
pub fn sampled_batch(xs: &[Xmx], n: u64, seeds: &[u64]) -> Vec<Counts> {
    use rayon::prelude::*;
    assert_eq!(xs.len(), seeds.len());
    let mut out: Vec<Option<Counts>> = vec![None; xs.len()];
    #[cfg(feature = "gpu")]
    if let Some(g) = xs.iter().any(crate::gpu::supports).then(gpu).flatten() {
        let mut groups: std::collections::BTreeMap<(u32, usize), Vec<usize>> = Default::default();
        for (i, x) in xs.iter().enumerate().filter(|(_, x)| crate::gpu::supports(x)) {
            groups.entry((x.bits, x.rounds())).or_default().push(i);
        }
        let g = g.lock().unwrap();
        for idx in groups.values() {
            let gx: Vec<Xmx> = idx.iter().map(|&i| xs[i].clone()).collect();
            let gs: Vec<u64> = idx.iter().map(|&i| seeds[i]).collect();
            let cs = g.sampled_batch(&gx, n, &gs).unwrap_or_else(|e| panic!("gpu sampling failed: {e}"));
            for (&i, c) in idx.iter().zip(cs) {
                out[i] = Some(c);
            }
        }
    }
    let rest: Vec<(usize, Counts)> = out
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_none())
        .map(|(i, _)| i)
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|i| (i, crate::racing::sample_counts_cpu(&xs[i], n, seeds[i])))
        .collect();
    for (i, c) in rest {
        out[i] = Some(c);
    }
    out.into_iter().map(Option::unwrap).collect()
}
