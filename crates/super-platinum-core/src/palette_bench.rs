//! Same-process paired comparison; no credentials, Slack requests, or UI mutation.
use super::*;
use crate::slack::models::{User, UserProfile};
use std::{hint::black_box, time::Instant};

#[test]
#[ignore = "run explicitly with --ignored --nocapture for paired performance measurements"]
fn large_workspace_benchmark() {
    for size in [10_000, 50_000] {
        let mut workspace = super::tests::ws();
        for index in 0..size {
            let id = format!("U{index:06}");
            workspace.users.insert(
                id.clone(),
                User {
                    id,
                    name: Some(format!("person-{index:06}")),
                    profile: Some(UserProfile {
                        display_name: Some(format!("Person {index:06} 東京")),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            );
            if index % 10 == 0 {
                let id = format!("C{index:06}");
                workspace.channels.insert(
                    id.clone(),
                    Channel {
                        id,
                        name: Some(format!("person-project-{index:06}")),
                        is_channel: true,
                        ..Default::default()
                    },
                );
            }
        }
        let remote = BTreeMap::new();
        for query in ["person", "person-00999", "東京", "no-match"] {
            let reference =
                || rank_with(black_box(&workspace), black_box(query), &remote, Vec::push);
            let bounded = || rank(black_box(&workspace), black_box(query), &remote);
            assert_eq!(reference(), bounded());
            for _ in 0..5 {
                black_box(reference());
                black_box(bounded());
            }
            let mut old = Vec::with_capacity(40);
            let mut new = Vec::with_capacity(40);
            for iteration in 0..40 {
                // Alternate order so thermal/load drift does not always favor one.
                let mut measure = |reference_first| {
                    let start = Instant::now();
                    if reference_first {
                        black_box(reference());
                    } else {
                        black_box(bounded());
                    }
                    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                    if reference_first {
                        old.push(elapsed);
                    } else {
                        new.push(elapsed);
                    }
                };
                measure(iteration % 2 == 0);
                measure(iteration % 2 != 0);
            }
            old.sort_by(f64::total_cmp);
            new.sort_by(f64::total_cmp);
            println!(
                "palette users={size} query={query:?} full_median_ms={:.3} bounded_median_ms={:.3} full_p95_ms={:.3} bounded_p95_ms={:.3}",
                old[20], new[20], old[38], new[38]
            );
        }
    }
}
