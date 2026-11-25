//! `victory precompile`: console shader containers translated into the
//! shader cache ahead of a run (`victory_gpu::shader_cache`), with progress.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{Context, Result};

fn containers(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = paths.to_vec();
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                stack.extend(rd.flatten().map(|e| e.path()));
            }
        } else if p.is_file() {
            out.push(p);
        }
    }
    out.sort();
    out
}

pub fn run(paths: Vec<PathBuf>, jobs: Option<usize>) -> Result<()> {
    let cfg = victory_core::Config::load()?;
    let dir = cfg.cache_subdir("shaders")?;
    victory_hle::modules::d3d12x::use_shader_cache(dir.clone());
    if !victory_hle::modules::d3d12x::precompile_init() {
        anyhow::bail!("no host D3D12 device: the translation depends on it");
    }
    let files = containers(&paths);
    let total = files.len();
    println!("shader cache {}", dir.display());
    println!("files        {total}");
    let threads = jobs.unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()));
    let next = AtomicUsize::new(0);
    let (done, failed, skipped) = (AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0));
    let started = Instant::now();
    let report = |n: usize| {
        let (hits, misses) = victory_hle::modules::d3d12x::precompile_counts();
        let secs = started.elapsed().as_secs_f64();
        let left = if n > 0 { secs / n as f64 * (total - n) as f64 } else { 0.0 };
        println!(
            "[{n:>6}/{total}] {:5.1}%  translated {misses:>6}  already cached {hits:>6}  failed {:>4}  not a container {:>4}  {:>4.0}s, ~{:.0}s left",
            n as f64 / total.max(1) as f64 * 100.0,
            failed.load(Ordering::Relaxed),
            skipped.load(Ordering::Relaxed),
            secs,
            left
        );
    };
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(path) = files.get(i) else { break };
                match std::fs::read(path).with_context(|| path.display().to_string()) {
                    Ok(bytes) if bytes.starts_with(b"DXBC") => {
                        if victory_hle::modules::d3d12x::precompile(&bytes).is_err() {
                            failed.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    _ => {
                        skipped.fetch_add(1, Ordering::Relaxed);
                    }
                }
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                if n % 1000 == 0 {
                    report(n);
                }
            });
        }
    });
    report(total);
    Ok(())
}
