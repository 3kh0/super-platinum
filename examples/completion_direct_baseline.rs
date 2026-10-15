//! Pure direct Person scan experiment. No production module wiring.
#[path = "../scripts/completion_baseline/corpus.rs"]
mod corpus;
#[path = "../scripts/completion_baseline/legacy_oracle.rs"]
mod legacy_oracle;
#[path = "../scripts/completion_baseline/person_scan.rs"]
mod person_scan;
use std::{hint::black_box, time::Instant};
use super_platinum_core::composer_completion::{Broadcast, LIMIT, Suggestion, Target, suggestions};

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    assert_eq!(
        arguments.len(),
        4,
        "usage: completion_direct_baseline <rounds 1..100> <seed 0..1000000> <direct|legacy>"
    );
    let rounds: usize = arguments[1].parse().expect("rounds");
    let seed: usize = arguments[2].parse().expect("seed");
    assert!((1..=100).contains(&rounds) && seed <= 1_000_000);
    let mode = arguments[3].as_str();
    let measure: fn(
        &super_platinum_core::state::Workspace,
        &super_platinum_core::composer_completion::Trigger,
    ) -> Vec<Suggestion> = match mode {
        "direct" => person_scan::suggestions,
        "legacy" => legacy_oracle::suggestions,
        _ => panic!("expected direct or legacy"),
    };
    let mut inventories = Vec::new();
    for n in 0..corpus::SIZES.len() {
        let total = corpus::SIZES[(n + seed) % corpus::SIZES.len()];
        let started = Instant::now();
        let ws = corpus::workspace(total);
        let construction_ms = started.elapsed().as_secs_f64() * 1000.0;
        let triggers: Vec<_> = corpus::QUERIES.iter().map(|q| corpus::trigger(q)).collect();
        let expected: Vec<Vec<Suggestion>> = triggers.iter().map(|t| suggestions(&ws, t)).collect();
        for (trigger, expected) in triggers.iter().zip(&expected) {
            assert_eq!(
                &legacy_oracle::suggestions(&ws, trigger),
                expected,
                "active legacy source drift"
            );
            assert_eq!(
                &person_scan::suggestions(&ws, trigger),
                expected,
                "candidate preflight parity"
            );
        }
        assert!(expected.iter().all(|rows| rows.len() <= LIMIT));
        assert!(
            expected
                .iter()
                .all(|rows| rows.iter().all(|r| r.target.token().is_some()))
        );
        assert_eq!(expected[4][0].target, Target::User("UPERF000001".into()));
        assert_eq!(expected[9][0].target, Target::Broadcast(Broadcast::Here));
        assert_eq!(
            expected[18][0].target,
            Target::Channel("CPERF000001".into())
        );
        assert!(expected[11].is_empty() && expected[22].is_empty());
        for choice in expected.iter().flatten() {
            match &choice.target {
                Target::User(id) => assert!(!ws.users[id].deleted),
                Target::Channel(id) => {
                    let c = &ws.channels[id];
                    assert!(!c.is_archived && !c.is_im && !c.is_mpim);
                }
                Target::Group(id) => assert_eq!(ws.usergroups[id].date_delete, 0),
                Target::Broadcast(_) => (),
            }
        }
        for _ in 0..2 {
            for t in &triggers {
                black_box(measure(black_box(&ws), black_box(t)));
            }
        }
        let mut samples: Vec<Vec<f64>> = vec![Vec::with_capacity(rounds); triggers.len()];
        for round in 0..rounds {
            // Coprime stride visits all 24 cases; shift each round/process.
            for offset in 0..triggers.len() {
                let index = (offset * 7 + round + seed) % triggers.len();
                let started = Instant::now();
                let result = black_box(measure(black_box(&ws), black_box(&triggers[index])));
                let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                samples[index].push(elapsed);
                // Assertion, JSON, and result deallocation are outside timing.
                assert_eq!(
                    result, expected[index],
                    "ranking changed within immutable inventory"
                );
            }
        }
        inventories.push(serde_json::json!({
            "total":total, "counts":corpus::counts(&ws), "construction_ms":construction_ms,
            "cases":triggers.iter().enumerate().map(|(i,t)|serde_json::json!({
                "query":corpus::QUERIES[i], "kind":corpus::kind_name(t.kind),
                "expected":expected[i], "samples_ms":samples[i]
            })).collect::<Vec<_>>()
        }));
    }
    println!(
        "{}",
        serde_json::json!({
            "schema":1, "experiment":"completion-direct-v1", "mode":mode, "corpus":corpus::VERSION, "rounds":rounds, "seed":seed, "warmup_rounds":2,
            "measurement":"selected direct or frozen legacy call wall time via function pointer; construction, trigger parsing, comparison, serialization, and result deallocation excluded",
            "inventories":inventories
        })
    );
}
