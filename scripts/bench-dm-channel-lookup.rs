//! CPU-only synthetic renderer lookup probe: real production ChannelVm layout.
//! No workspace/session/network, DOM, rich projection, or retained cache.
//! Run scripts/bench-dm-channel-lookup.sh after a normal desktop build. It uses
//! existing core artifacts and rustc at the app's opt-level=1, never Cargo.
//! The old two-scan path is retained here only as the paired reference.
#![allow(dead_code, unused_imports)]
#[path = "../src/desktop/view/secondary/dm_channels.rs"]
mod dm_channels;
#[path = "../src/desktop/model.rs"]
mod model;
use model::{ChannelVm, PresenceVm};
use std::{hint::black_box, time::Instant};
fn consume(row: Option<&ChannelVm>) -> u64 {
    row.map(|c| match c.presence {
        PresenceVm::Active => 3,
        PresenceVm::Away => 2,
        PresenceVm::Unknown => 1,
    })
    .unwrap_or(0)
}
#[inline(never)]
fn linear(channels: &[ChannelVm], entries: &[String]) -> u64 {
    let mut total = 0;
    for id in entries {
        // The current renderer's two independent per-entry ChannelVm searches.
        let avatar = channels
            .iter()
            .find(|c| c.id == *id)
            .and_then(|c| c.avatar.clone());
        let presence = channels.iter().find(|c| c.id == *id);
        total += u64::from(black_box(avatar).is_some()) + consume(black_box(presence));
    }
    total
}
#[inline(never)]
fn indexed(channels: &[ChannelVm], entries: &[String]) -> u64 {
    // Construction and destruction included on every render, not amortized.
    let index = dm_channels::index(channels);
    let mut total = 0;
    for id in entries {
        let row = index.get(id.as_str()).copied();
        let avatar = row.and_then(|c| c.avatar.clone());
        total += u64::from(black_box(avatar).is_some()) + consume(black_box(row));
    }
    total
}
fn stats(mut t: Vec<f64>) -> (f64, f64) {
    t.sort_by(f64::total_cmp);
    (t[t.len() / 2], t[(t.len() * 95).div_ceil(100) - 1])
}
fn measure(f: impl Fn() -> u64) -> (f64, u64) {
    let start = Instant::now();
    let mut value = 0;
    for _ in 0..5 {
        value = black_box(f());
    }
    (start.elapsed().as_secs_f64() * 1000.0 / 5.0, value)
}
fn main() {
    println!(
        "ChannelVm_bytes={} warmup=5 samples=40 batch=5 optimization=1",
        std::mem::size_of::<ChannelVm>()
    );
    for (count, loaded) in [
        (2000, 100),
        (2000, 500),
        (10000, 100),
        (10000, 500),
        (10000, 2000),
    ] {
        let channels: Vec<_> = (0..count)
            .map(|i| ChannelVm {
                id: format!("D{i:010}"),
                name: format!("Synthetic person {i}"),
                presence: if i % 2 == 0 {
                    PresenceVm::Active
                } else {
                    PresenceVm::Away
                },
                ..Default::default()
            })
            .collect();
        // Deterministic distributed hits; include one missing entry per ten rows.
        let entries: Vec<_> = (0..loaded)
            .map(|i| {
                if i % 10 == 0 {
                    format!("Dmissing{i:04}")
                } else {
                    channels[(i * 7919) % count].id.clone()
                }
            })
            .collect();
        assert_eq!(linear(&channels, &entries), indexed(&channels, &entries));
        let (mut a, mut b) = (vec![], vec![]);
        for round in 0..45 {
            let funcs: [fn(&[ChannelVm], &[String]) -> u64; 2] = if round % 2 == 0 {
                [linear, indexed]
            } else {
                [indexed, linear]
            };
            let mut times = [0.0; 2];
            for (j, f) in funcs.iter().enumerate() {
                let (t, out) = measure(|| f(black_box(&channels), black_box(&entries)));
                black_box(out);
                times[if round % 2 == 0 { j } else { 1 - j }] = t;
            }
            if round >= 5 {
                a.push(times[0]);
                b.push(times[1]);
            }
        }
        let (am, ap) = stats(a);
        let (bm, bp) = stats(b);
        println!(
            "channels={count} loaded_dms={loaded} linear_ms={am:.6}/{ap:.6} indexed_ms={bm:.6}/{bp:.6}"
        );
    }
}
