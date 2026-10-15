//! Pure staged index experiment. The runner selects one timed path per process.
#[path = "../scripts/completion_baseline/corpus.rs"]
mod corpus;
use std::{hint::black_box, time::Instant};
use super_platinum_core::{
    composer_completion::{
        Trigger,
        index::{CheckedIndex, Snapshot},
    },
    state::Workspace,
};

fn measured(
    mode: &str,
    index: &CheckedIndex,
    ws: &Workspace,
    trigger: &Trigger,
) -> Option<Vec<Suggestion>> {
    match mode {
        "checked" => Some(
            index
                .suggestions(black_box(ws), black_box(trigger))
                .expect("canonical mismatch"),
        ),
        "index" => Some(index.unchecked_index().suggestions(black_box(trigger))),
        "scan" => {
            assert!(black_box(index.matches(black_box(ws))));
            None
        }
        _ => unreachable!(),
    }
}

fn build(ws: &Workspace, phase: &str) -> (CheckedIndex, serde_json::Value) {
    let start = Instant::now();
    let snapshot = Snapshot::capture(black_box(ws));
    let capture_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    let index = CheckedIndex::build(black_box(snapshot));
    let build_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    let current = black_box(index.matches(black_box(ws)));
    let publication_scan_ms = start.elapsed().as_secs_f64() * 1000.0;
    assert!(current);
    let record = serde_json::json!({"phase":phase,"capture_ms":capture_ms,"build_ms":build_ms,"publication_scan_ms":publication_scan_ms,"retire_ms":0.0,"snapshot_string_capacity_bytes":index.snapshot_string_capacity_bytes(),"index_string_capacity_bytes":index.index_string_capacity_bytes()});
    (index, record)
}
use super_platinum_core::composer_completion::{Broadcast, LIMIT, Suggestion, Target, suggestions};

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    assert_eq!(
        arguments.len(),
        4,
        "usage: completion_index_baseline <rounds 1..100> <seed 0..1000000> <checked|index|scan>"
    );
    let rounds: usize = arguments[1].parse().expect("rounds");
    let seed: usize = arguments[2].parse().expect("seed");
    let mode = arguments[3].as_str();
    assert!(matches!(mode, "checked" | "index" | "scan"));
    assert!((1..=100).contains(&rounds) && seed <= 1_000_000);
    let mut inventories = Vec::new();
    for n in 0..corpus::SIZES.len() {
        let total = corpus::SIZES[(n + seed) % corpus::SIZES.len()];
        let started = Instant::now();
        let mut ws = corpus::workspace(total);
        let construction_ms = started.elapsed().as_secs_f64() * 1000.0;
        let triggers: Vec<_> = corpus::QUERIES.iter().map(|q| corpus::trigger(q)).collect();
        let expected: Vec<Vec<Suggestion>> = triggers.iter().map(|t| suggestions(&ws, t)).collect();
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
        // Lifecycle measurements are separate from timed query loops. Keep the
        // old index alive during replacement, as an async publication would.
        let (mut prepared, initial) = build(&ws, "initial");
        let mut lifecycle = vec![initial];
        let original_name = ws.users["UPERF000001"].name.clone();
        ws.users.get_mut("UPERF000001").unwrap().name = Some("validation-rename".into());
        assert!(!prepared.matches(&ws));
        let (next, mut renamed) = build(&ws, "rename");
        for trigger in &triggers {
            assert_eq!(
                next.suggestions(&ws, trigger).unwrap(),
                suggestions(&ws, trigger)
            );
        }
        let retired = Instant::now();
        drop(prepared);
        renamed["retire_ms"] = serde_json::json!(retired.elapsed().as_secs_f64() * 1000.0);
        prepared = next;
        lifecycle.push(renamed);
        ws.users.get_mut("UPERF000001").unwrap().name = original_name;
        assert!(!prepared.matches(&ws));
        let (next, mut restored) = build(&ws, "restore");
        let retired = Instant::now();
        drop(prepared);
        restored["retire_ms"] = serde_json::json!(retired.elapsed().as_secs_f64() * 1000.0);
        prepared = next;
        lifecycle.push(restored);
        for (i, trigger) in triggers.iter().enumerate() {
            assert_eq!(prepared.suggestions(&ws, trigger).unwrap(), expected[i]);
        }
        for _ in 0..2 {
            for t in &triggers {
                black_box(measured(
                    mode,
                    black_box(&prepared),
                    black_box(&ws),
                    black_box(t),
                ));
            }
        }
        let mut samples: Vec<Vec<f64>> = vec![Vec::with_capacity(rounds); triggers.len()];
        for round in 0..rounds {
            // Coprime stride visits all 24 cases; shift each round/process.
            for offset in 0..triggers.len() {
                let index = (offset * 7 + round + seed) % triggers.len();
                let started = Instant::now();
                let result = black_box(measured(
                    mode,
                    black_box(&prepared),
                    black_box(&ws),
                    black_box(&triggers[index]),
                ));
                let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                samples[index].push(elapsed);
                // Assertion, JSON, and result deallocation are outside timing.
                if let Some(result) = result {
                    assert_eq!(
                        result, expected[index],
                        "ranking changed within immutable inventory"
                    );
                }
            }
        }
        inventories.push(serde_json::json!({
            "total":total, "counts":corpus::counts(&ws), "construction_ms":construction_ms, "lifecycle":lifecycle,
            "cases":triggers.iter().enumerate().map(|(i,t)|serde_json::json!({
                "query":corpus::QUERIES[i], "kind":corpus::kind_name(t.kind),
                "expected":expected[i], "samples_ms":samples[i]
            })).collect::<Vec<_>>()
        }));
    }
    println!(
        "{}",
        serde_json::json!({
            "schema":1, "experiment":"completion-index-v1", "mode":mode, "corpus":corpus::VERSION, "rounds":rounds, "seed":seed, "warmup_rounds":2,
            "measurement":match mode {"checked"=>"exact canonical equality scan plus prepared query wall time", "index"=>"prepared query only; unsafe without separate canonical validation", _=>"exact canonical equality scan only; no query work"},
            "inventories":inventories
        })
    );
}
