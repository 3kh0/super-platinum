//! Reproducible renderer projection benchmark, synthetic data only.
#[test]
#[ignore = "manual timing probe; run alone with --nocapture"]
fn mixed_reply_projection() {
    let core = crate::fixture::fixture_core();
    let media = crate::media::MediaRegistry::default();
    for count in [200, 2000, 10000] {
        let bag = super::fixture_bag(count);
        let history = super_platinum_core::thread_history::ThreadHistory::default();
        let mut timings = Vec::new();
        for _ in 0..11 {
            let now = std::time::Instant::now();
            let rows = super::project(
                &core.workspaces["T1"],
                &bag,
                &history,
                "1719800400.000000",
                &media,
            );
            std::hint::black_box(&rows);
            timings.push(now.elapsed().as_secs_f64() * 1000.0);
        }
        timings.remove(0);
        timings.sort_by(f64::total_cmp);
        println!(
            "replies={count} projected={} median_ms={:.3} min_ms={:.3} max_ms={:.3}",
            super::project(
                &core.workspaces["T1"],
                &bag,
                &history,
                "1719800400.000000",
                &media
            )
            .len(),
            (timings[4] + timings[5]) / 2.0,
            timings[0],
            timings[9]
        );
    }
}
