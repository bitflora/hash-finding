//! Exact xmx counts with a run-time choice of backend. Every exact score in
//! the harness (eval, exact lists, race and search verification, traces)
//! goes through `exact_xmx`, so `--gpu` (see `use_gpu`) moves all of them to
//! the GPU at once. Mixers the GPU kernel can't take (fewer than 8 bits)
//! stay on `fast32`; both backends give identical counts.

use crate::bias::Counts;
use crate::mixers::Xmx;
use std::sync::atomic::{AtomicBool, Ordering};

static GPU: AtomicBool = AtomicBool::new(false);

/// Route exact scoring to the GPU. Initialises CUDA immediately so a missing
/// GPU or toolkit is reported up front, not mid-search.
pub fn use_gpu() -> Result<String, String> {
    #[cfg(feature = "gpu")]
    {
        let name = crate::gpu::global()?.lock().unwrap().name().to_string();
        GPU.store(true, Ordering::Relaxed);
        Ok(name)
    }
    #[cfg(not(feature = "gpu"))]
    Err("--gpu needs a build with `cargo build --release --features gpu`".into())
}

pub fn gpu_enabled() -> bool {
    GPU.load(Ordering::Relaxed)
}

/// Exact counts for an xmx mixer with 3 <= bits <= 32. `parallel` only
/// affects the CPU path (the GPU is always fully used).
pub fn exact_xmx(x: &Xmx, parallel: bool) -> Counts {
    #[cfg(feature = "gpu")]
    if gpu_enabled() && (8..=32).contains(&x.bits) && (1..=4).contains(&x.rounds()) {
        return crate::gpu::exact_xmx(x).unwrap_or_else(|e| panic!("gpu exact scoring of {x} failed: {e}"));
    }
    crate::fast32::exact_xmx(x, parallel)
}
