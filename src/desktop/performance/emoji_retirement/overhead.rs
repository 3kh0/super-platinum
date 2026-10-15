//! Exact ignored, root-owned overhead experiment. No Catalog or native fixture.
use super::{Payload, building, enabled, generation, ledger, payload};
use std::{collections::BTreeMap, hint::black_box, path::PathBuf, time::Instant};

#[test]
#[ignore = "root-authorized single finite overhead run; requires fresh owned artifact directory"]
fn finite_actual_empty_wrapper_observer_overhead() {
    assert!(cfg!(windows), "Pinned Windows serial-thread experiment");
    let directory = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_EMOJI_RETIREMENT_OVERHEAD_DIR")
            .expect("explicit owned overhead output"),
    );
    assert!(directory.is_absolute());
    assert!(
        directory
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("emoji-retirement-overhead-")
    );
    std::fs::create_dir(&directory).expect("fresh overhead output directory");
    let manifest = std::env::var("SUPER_PLATINUM_EMOJI_RETIREMENT_MANIFEST_SHA256")
        .expect("root-sealed observer manifest");
    assert!(
        manifest.len() == 64
            && manifest
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    let disabled = ledger::TestLedger::new();
    let observed = ledger::TestLedger::new();
    let mut blocks = Vec::with_capacity(100);
    let mut next = 1u64;
    for block in 0..100 {
        let on = block % 2 == 1;
        let owner = if on { &observed } else { &disabled };
        let _arm = owner.activate(on);
        assert_eq!(enabled(), on);
        // Both arms use the same feature wrapper and empty Vec payload type.
        // Construction/ID creation is outside the measured drop interval.
        let children: [Payload<Vec<u8>>; 100] = std::array::from_fn(|_| {
            let id = if on {
                let id = generation(0, next, false);
                next += 1;
                id
            } else {
                0
            };
            let _build = building(id);
            payload(Vec::new(), 1)
        });
        let children = black_box(children);
        let record_start_ns = owner.elapsed();
        let at = Instant::now();
        drop(children);
        let ns = at.elapsed().as_nanos() as u64;
        let record_end_ns = owner.elapsed();
        black_box(ns);
        blocks.push(serde_json::json!({"block":block,"enabled":on,"drops":100,"nanoseconds":ns,"record_start_ns":record_start_ns,"record_end_ns":record_end_ns}));
    }
    assert_eq!(next, 5001);
    let off = disabled.words();
    let on = observed.words();
    // Retain both raw ledgers even if completeness or the timing screen fails.
    std::fs::write(directory.join("disabled.bin"), words_bytes(&off)).unwrap();
    std::fs::write(directory.join("enabled.bin"), words_bytes(&on)).unwrap();
    let mut children = BTreeMap::<(u64, u64), Vec<u64>>::new();
    let mut publication_ok = on[4] == 0 && off[4] == 0 && off[3] == 0;
    for index in 0..on[3] as usize {
        let row = &on[ledger::HEADER_WORDS + index * ledger::WORDS..][..ledger::WORDS];
        publication_ok &= row[0] == index as u64 + 1 && row[6] == on[5] && row[7] == 1;
        if (1..=5).contains(&row[2]) {
            children.entry((row[3], row[4])).or_default().push(row[2]);
        }
    }
    let complete = publication_ok
        && on[3] == 35_000
        && children.len() == 5000
        && children
            .iter()
            .all(|(&(id, child), events)| id & 3 == 1 && child == 1 && events == &[1, 2, 3, 4, 5]);
    let median = |enabled| {
        let mut values: Vec<_> = blocks
            .iter()
            .filter(|b| b["enabled"] == enabled)
            .map(|b| b["nanoseconds"].as_u64().unwrap())
            .collect();
        values.sort_unstable();
        (values[24] as f64 + values[25] as f64) / 200.0
    };
    let disabled_ns = median(false);
    let enabled_ns = median(true);
    let difference = enabled_ns - disabled_ns;
    let ok = complete && difference <= 10_000.0;
    let receipt = serde_json::json!({"schema":1,"kind":"empty-wrapper-overhead",
        "ok":ok,"manifest_sha256":manifest,"pid":std::process::id(),"nonce":"00000000000000000000000000000000",
        "process":"W1","blocks":blocks,"disabled_median_ns_per_drop":disabled_ns,
        "enabled_median_ns_per_drop":enabled_ns,"difference_ns_per_drop":difference,
        "threshold_ns_per_drop":10_000,"complete":complete,"enabled_child_events":children.values().map(Vec::len).sum::<usize>(),
        "enabled_total_events":on[3],"disabled_total_events":off[3],
        "measurement":"Actual same feature Payload<Vec<u8>> array destruction; construction outside timer; shared array-drop loop and two Instant readings retained; no Catalog or native fixture."});
    std::fs::write(
        directory.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    eprintln!("emoji-retirement-overhead: {receipt}");
    assert!(
        ok,
        "Observer overhead/completeness gate failed; retain this single attempt"
    );
}

fn words_bytes(words: &[u64]) -> Vec<u8> {
    words.iter().flat_map(|n| n.to_le_bytes()).collect()
}
