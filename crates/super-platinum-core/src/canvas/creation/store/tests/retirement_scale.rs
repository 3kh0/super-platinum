use super::*;
use std::time::Instant;

/// Root-only opt-in scale gate. Uses only the Temp helper's isolated synthetic DB.
#[test]
#[ignore = "isolated 1k/10k/100k storage and indexed-access measurements"]
fn indexed_retirement_scale_costs_and_queries() {
    let temp = Temp::new();
    let mut s = temp.store();
    let base = seed(&temp, &mut s);
    let mut inserted = 0usize;
    for target in [1_000usize, 10_000, 100_000] {
        let tx = s
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        for i in inserted..target {
            let r = RetiredAttempt {
                id: uuid::Uuid::from_u128((i + 1) as u128).to_string(),
                owner: temp.owner(),
                draft_id: base.id.clone(),
                draft_revision: 1,
                result_revision: 2,
                delivery: Delivery::CanceledBeforeAdmission,
                decision: None,
            };
            insert(&tx, &r).unwrap();
        }
        tx.commit().unwrap();
        inserted = target;
        let mut hit = Vec::new();
        let mut miss = Vec::new();
        let mut read_times = Vec::new();
        let hit_id = uuid::Uuid::from_u128((target / 2) as u128).to_string();
        for _ in 0..31 {
            let started = Instant::now();
            assert!(s.retired_attempt(&temp.owner(), &hit_id).unwrap().is_some());
            hit.push(started.elapsed().as_nanos());
            let started = Instant::now();
            assert!(
                s.retired_attempt(&temp.owner(), &uuid::Uuid::from_u128(u128::MAX).to_string())
                    .unwrap()
                    .is_none()
            );
            miss.push(started.elapsed().as_nanos());
            let started = Instant::now();
            assert!(s.active(&temp.owner()).unwrap() == Some(base.clone()));
            read_times.push(started.elapsed().as_nanos());
        }
        let page_size: i64 = s
            .conn
            .query_row("PRAGMA page_size", [], |r| r.get(0))
            .unwrap();
        let page_count: i64 = s
            .conn
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        let free: i64 = s
            .conn
            .query_row("PRAGMA freelist_count", [], |r| r.get(0))
            .unwrap();
        let file_bytes = std::fs::metadata(temp.path()).unwrap().len();
        let started = Instant::now();
        let reopened = temp.store();
        let open_ns = started.elapsed().as_nanos();
        drop(reopened);
        hit.sort();
        miss.sort();
        read_times.sort();
        let plans: Vec<String> = s
            .conn
            .prepare("EXPLAIN QUERY PLAN SELECT body FROM retired_attempts WHERE id=?1")
            .unwrap()
            .query_map([&hit_id], |r| r.get(3))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(
            plans
                .iter()
                .any(|p| p.contains("SEARCH") && p.contains("INDEX"))
        );
        println!(
            "canvas-retirement-scale: {}",
            serde_json::json!({"rows":target,"fileBytes":file_bytes,"pageSize":page_size,"pageCount":page_count,"freelistCount":free,"openNs":open_ns,"hitP50Ns":hit[15],"hitMaxNs":hit[30],"missP50Ns":miss[15],"missMaxNs":miss[30],"activeP50Ns":read_times[15],"activeMaxNs":read_times[30],"plans":plans})
        );
    }
    let started = Instant::now();
    let reservation = s.reserve(&base, 100).unwrap();
    let permit = s.admit(&reservation, 100).unwrap();
    let admission_ns = started.elapsed().as_nanos();
    drop(permit);
    println!(
        "canvas-retirement-admission: {}",
        serde_json::json!({"retiredRows":100000,"reserveAndAdmitNs":admission_ns,"noTransport":true})
    );
}
