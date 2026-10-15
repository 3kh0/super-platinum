use super::*;
fn completed_retry(conn: &mut Connection, seconds: u64) -> Record {
    let p = prepared("T1", "C1");
    let d = armed(&p);
    let a = ack(&d, Some(seconds));
    apply(conn, None, &p, &now(0)).unwrap();
    apply(conn, Some(&p), &d, &now(1)).unwrap();
    apply(conn, Some(&d), &a, &now(10)).unwrap();
    a
}
#[test]
fn monotonic_cooldown_blocks_across_destinations_until_exact_deadline() {
    let mut conn = db();
    completed_retry(&mut conn, 30);
    let p = prepared("T1", "C2");
    assert_eq!(
        apply(&mut conn, None, &p, &now(30_009)),
        Err(Failure::RateLimited)
    );
    apply(&mut conn, None, &p, &now(30_010)).unwrap();
    apply(&mut conn, Some(&p), &armed(&p), &now(30_010)).unwrap();
}
#[test]
fn wall_clock_jumps_never_shorten_live_monotonic_deadline() {
    let mut conn = db();
    completed_retry(&mut conn, 30);
    let p = prepared("T1", "C2");
    for wall in [1, 9_000_000_000] {
        let mut tick = now(20);
        tick.wall = wall;
        assert_eq!(apply(&mut conn, None, &p, &tick), Err(Failure::RateLimited));
    }
}
#[test]
fn restart_rebase_is_durable_and_does_not_restart_wait_on_each_denial() {
    let mut conn = db();
    completed_retry(&mut conn, 30);
    let p = prepared("T1", "C2");
    let mut tick = now(5);
    tick.run = OTHER_RUN.into();
    tick.wall = 9_000_000_000;
    assert_eq!(apply(&mut conn, None, &p, &tick), Err(Failure::RateLimited));
    let deadline: i64 = conn
        .query_row("SELECT until_mono FROM sp_slash_cooldowns", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(deadline, 30_005);
    tick.mono = 30_004;
    assert_eq!(apply(&mut conn, None, &p, &tick), Err(Failure::RateLimited));
    assert_eq!(
        conn.query_row("SELECT until_mono FROM sp_slash_cooldowns", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        deadline
    );
    tick.mono = 30_005;
    apply(&mut conn, None, &p, &tick).unwrap();
}
#[test]
fn terminal_disk_retry_and_retirement_never_extend_cooldown() {
    let mut conn = db();
    let p = prepared("T1", "C1");
    let d = armed(&p);
    let a = ack(&d, Some(20));
    apply(&mut conn, None, &p, &now(0)).unwrap();
    apply(&mut conn, Some(&p), &d, &now(1)).unwrap();
    apply(&mut conn, Some(&d), &a, &now(10)).unwrap();
    apply(&mut conn, Some(&d), &a, &now(500)).unwrap();
    apply(&mut conn, Some(&a), &retired(&a), &now(900)).unwrap();
    assert_eq!(
        conn.query_row("SELECT until_mono FROM sp_slash_cooldowns", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        20_010
    );
}
#[test]
fn absent_or_overflowing_retry_after_becomes_permanent_guard() {
    for retry in [None, Some(u64::MAX)] {
        let mut conn = db();
        let p = prepared("T1", "C1");
        let d = armed(&p);
        apply(&mut conn, None, &p, &now(0)).unwrap();
        apply(&mut conn, Some(&p), &d, &now(1)).unwrap();
        let result = next(&d, json!("Unknown"), Some(429), retry);
        apply(&mut conn, Some(&d), &result, &now(10)).unwrap();
        assert_eq!(
            conn.query_row("SELECT permanent FROM sp_slash_cooldowns", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            apply(
                &mut conn,
                None,
                &prepared("T1", "C2"),
                &now(i64::MAX - 1_000_000)
            ),
            Err(Failure::RateLimited)
        );
    }
}
#[test]
fn cooldown_and_unresolved_429_are_independent_guards() {
    let mut conn = db();
    let p = prepared("T1", "C1");
    let d = armed(&p);
    apply(&mut conn, None, &p, &now(0)).unwrap();
    apply(&mut conn, Some(&p), &d, &now(1)).unwrap();
    let result = next(&d, json!("Unknown"), Some(429), Some(1));
    apply(&mut conn, Some(&d), &result, &now(10)).unwrap();
    assert_eq!(
        apply(&mut conn, None, &prepared("T1", "C2"), &now(500)),
        Err(Failure::RateLimited)
    );
    assert_eq!(
        apply(&mut conn, None, &prepared("T1", "C2"), &now(1010)),
        Err(Failure::NoReplay)
    );
    assert!(only(&mut conn) == result);
}
#[test]
fn malformed_or_foreign_cooldown_cannot_be_ignored() {
    let mut conn = db();
    completed_retry(&mut conn, 20);
    conn.execute("UPDATE sp_slash_cooldowns SET run=NULL", [])
        .unwrap();
    assert!(apply(&mut conn, None, &prepared("T2", "C2"), &now(1)).is_err());
    assert!(
        conn.query_row("SELECT run FROM sp_slash_cooldowns", [], |r| r
            .get::<_, Option<String>>(0))
            .unwrap()
            .is_none()
    );
}
#[test]
fn zero_retry_after_has_no_delay_but_does_not_resolve_unknown_outcome() {
    let mut conn = db();
    completed_retry(&mut conn, 0);
    apply(&mut conn, None, &prepared("T1", "C2"), &now(10)).unwrap();
}
