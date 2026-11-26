//! The console ray-tracing structure builder's group prefix sums, run as
//! translated for a 32-lane host and checked against a sum on the CPU.
//!
//! Needs the title's `XboxBvhBuilder.dll` (`VICTORY_BVH_DLL=<path>`; game
//! data, never in the repository); skipped without it.

use super::host::{Dispatch, Host, Root};
use super::{HostShader, translate};

/// File offsets of the prefix-sum programs' containers in the DLL.
const SCANS: [usize; 3] = [0x9a830, 0x9be80, 0x99160];

fn container(dll: &[u8], at: usize) -> &[u8] {
    assert_eq!(&dll[at..at + 4], b"DXBC", "no container at {at:#x}");
    let size = u32::from_le_bytes(dll[at + 24..at + 28].try_into().unwrap()) as usize;
    &dll[at..at + size]
}

fn shaders() -> Option<Vec<(usize, HostShader)>> {
    let path = std::env::var_os("VICTORY_BVH_DLL")?;
    let dll = std::fs::read(path).expect("read the DLL");
    super::set_host_max_wave(32);
    Some(
        SCANS
            .iter()
            .map(|&at| (at, translate(container(&dll, at)).expect("translate")))
            .collect(),
    )
}

#[test]
fn builder_scans_describe() {
    let Some(shaders) = shaders() else {
        eprintln!("VICTORY_BVH_DLL not set; skipped");
        return;
    };
    for (at, s) in &shaders {
        eprintln!(
            "{at:#x}: user_data {:?} root_cbvs {:?} root_srvs {:?} root_uavs {:?} dwords {}",
            s.user_data,
            s.root_cbvs,
            s.root_srvs,
            s.root_uavs,
            s.user_data_dwords()
        );
    }
}

/// Each scan program: its offset, threads per group, dwords per thread.
const SHAPES: [(usize, u32, u32); 3] = [(0x9a830, 512, 4), (0x9be80, 256, 4), (0x99160, 512, 8)];

#[test]
fn builder_scans_match_a_cpu_prefix_sum() {
    let Some(shaders) = shaders() else {
        eprintln!("VICTORY_BVH_DLL not set; skipped");
        return;
    };
    let _one = super::valu_tests::ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(host) = Host::new(false) else {
        eprintln!("no SM 6.6 hardware device; skipped");
        return;
    };
    let mut failed = Vec::new();
    for (at, s) in &shaders {
        let &(_, threads, per) = SHAPES.iter().find(|x| x.0 == *at).unwrap();
        let groups = 3u32;
        let n = (groups * threads * per) as usize;
        // Small counts, as the builder sums (node and primitive counts).
        let mut seed = 0x1234_5678u32 ^ *at as u32;
        let input: Vec<u32> = (0..n)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed % 7
            })
            .collect();
        let (root, pso) = host.compute_pipeline(s).expect("compute pipeline");
        let mut ud = vec![0u32; s.user_data_dwords().max(1) as usize];
        for u in &s.user_data {
            if u.root == crate::native::RootType::Constants {
                ud[u.index as usize] = 1; // also the group totals
            }
        }
        let out = host
            .dispatch(
                &root,
                &pso,
                &Dispatch {
                    roots: vec![
                        (0, Root::Constants(ud)),
                        (1, Root::Uav { data: input.clone() }),
                        (2, Root::Uav { data: vec![0xdead_beef; groups as usize] }),
                    ],
                    views: &[],
                    samplers: &[],
                    groups: [groups, 1, 1],
                },
            )
            .expect("dispatch");
        let span = (threads * per) as usize;
        let mut expect = vec![0u32; n];
        let mut totals = vec![0u32; groups as usize];
        for g in 0..groups as usize {
            let mut sum = 0u32;
            for i in g * span..(g + 1) * span {
                expect[i] = sum;
                sum += input[i];
            }
            totals[g] = sum;
        }
        let wrong: Vec<usize> = (0..n).filter(|&i| out[0][i] != expect[i]).collect();
        eprintln!(
            "{at:#x}: {} of {n} dwords wrong; totals {:?} expected {:?}",
            wrong.len(),
            out[1],
            totals
        );
        for &i in wrong.iter().take(12) {
            eprintln!("  [{i}] (thread {} of group {}): got {} expected {}", (i % span) / per as usize, i / span, out[0][i], expect[i]);
        }
        if !wrong.is_empty() || out[1] != totals {
            failed.push(*at);
        }
    }
    assert!(failed.is_empty(), "scans that differ: {failed:x?}");
}
