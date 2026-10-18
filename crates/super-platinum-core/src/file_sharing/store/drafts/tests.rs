use super::*;
use serde_json::json;
use std::path::PathBuf;
const OWNER: &str = "4b02f26e-393b-4fd1-a7da-5d11f76296ae";
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("sp-share-unsubmitted-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn path(&self) -> PathBuf {
        self.0.join("private.sqlite")
    }
    fn open(&self) -> Journal {
        Journal::open(self.path(), OWNER).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn scope(n: usize) -> DraftScope {
    DraftScope {
        team: "T12345678".into(),
        user: "U12345678".into(),
        file: format!("FFILE{n}"),
    }
}
fn initial(n: usize) -> Content {
    Content {
        source: json!({"id":scope(n).file,"mode":"hosted","is_external":false,"future":{"keep":7}}),
        title: "File".into(),
        note: vec![],
        destination: None,
    }
}
fn note() -> Vec<Value> {
    vec![
        json!({"type":"rich_text","future":{"block":true},"elements":[{"type":"rich_text_section","section":7,"elements":[{"type":"text","text":"\u{65e5}\u{672c}\u{8a9e} \u{1f98a} e\u{301}","style":{"future":42}}]}]}),
    ]
}
fn claim(j: &mut Journal, n: usize) -> Draft {
    match j.claim_draft(scope(n), None, initial(n)).unwrap() {
        Claim::Claimed(d) => d,
        _ => panic!("claim"),
    }
}
fn saved(j: &mut Journal, n: usize) -> Draft {
    let d = claim(j, n);
    let mut c = d.content.clone();
    c.note = note();
    c.destination = Some(Destination {
        id: "C12345678".into(),
        name: "ship".into(),
        private: false,
    });
    applied(j.save_draft(&d, c).unwrap())
}
fn begin(j: &mut Journal, d: &Draft, now: u64) -> Receipt {
    match j.begin_from_draft(d, now).unwrap() {
        Begin::Started(r) => r,
        _ => panic!("begin"),
    }
}
fn links(j: &Journal) -> i64 {
    j.conn
        .query_row("SELECT COUNT(*) FROM file_share_draft_attempts", [], |r| {
            r.get(0)
        })
        .unwrap()
}
fn v1(path: &Path) -> Connection {
    let c = Connection::open(path).unwrap();
    c.execute_batch("CREATE TABLE identity(singleton INTEGER PRIMARY KEY CHECK(singleton=1),account TEXT NOT NULL);
 CREATE TABLE receipts(team TEXT NOT NULL,user TEXT NOT NULL,file TEXT NOT NULL,destination TEXT NOT NULL,attempt TEXT NOT NULL UNIQUE,terminal INTEGER NOT NULL CHECK(terminal IN(0,1)),updated_at INTEGER NOT NULL,body TEXT NOT NULL,PRIMARY KEY(team,user,file,destination));
 CREATE TABLE cooldowns(team TEXT NOT NULL,user TEXT NOT NULL,retry_at INTEGER NOT NULL CHECK(retry_at>=0),PRIMARY KEY(team,user)); PRAGMA user_version=1;").unwrap();
    c.execute("INSERT INTO identity VALUES(1,?1)", [OWNER])
        .unwrap();
    c
}

#[test]
fn draft_commit_reopen_preserves_full_metadata_and_optional_destination() {
    let t = Temp::new();
    let mut j = t.open();
    let d = claim(&mut j, 0);
    let mut content = d.content.clone();
    content.note = note();
    let saved = applied(j.save_draft(&d, content).unwrap());
    drop(j);
    let j = t.open();
    assert!(j.draft(&scope(0)).unwrap().as_ref() == Some(&saved));
    assert!(saved.content.destination.is_none());
    assert!(saved.content.note == note());
    assert_eq!(j.drafts(&scope(0).team, &scope(0).user).unwrap().len(), 1);
    assert!(
        j.unresolved(&scope(0).team, &scope(0).user)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn exact_claim_save_delete_conflicts_never_rebind_or_resurrect() {
    let t = Temp::new();
    let mut j = t.open();
    let d = saved(&mut j, 0);
    assert!(matches!(
        j.claim_draft(scope(0), None, initial(0)).unwrap(),
        Claim::Conflict(Some(_))
    ));
    let mut newer_initial = initial(0);
    newer_initial.source["future"] = json!("new source");
    let next = match j.claim_draft(scope(0), Some(&d), newer_initial).unwrap() {
        Claim::Claimed(d) => d,
        _ => panic!("claim"),
    };
    assert!(next.content == d.content);
    assert_ne!(next.token, d.token);
    assert!(next.revision > d.revision);
    assert!(conflict(j.save_draft(&d, d.content.clone()).unwrap()));
    assert!(conflict(j.discard_draft(&d).unwrap()));
    let mut forged = next.clone();
    forged.content.note = vec![];
    assert!(conflict(
        j.save_draft(&forged, next.content.clone()).unwrap()
    ));
    assert!(conflict(j.discard_draft(&forged).unwrap()));
    applied(j.discard_draft(&next).unwrap());
    assert!(conflict(j.save_draft(&next, next.content.clone()).unwrap()));
    let fresh = claim(&mut j, 0);
    assert_ne!(fresh.token, next.token);
    assert!(conflict(j.save_draft(&next, next.content.clone()).unwrap()));
    assert!(j.draft(&scope(0)).unwrap().as_ref() == Some(&fresh));
}
#[test]
fn scope_source_shape_size_and_revision_errors_preserve_existing() {
    let t = Temp::new();
    let mut j = t.open();
    let d = saved(&mut j, 0);
    for mutate in [0, 1, 2, 3, 4] {
        let mut c = d.content.clone();
        match mutate {
            0 => c.source["id"] = json!("FOTHER"),
            1 => c.source["large"] = json!("x".repeat(MAX_SOURCE_BYTES)),
            2 => c.note = vec![json!({"no_type":true})],
            3 => c.destination.as_mut().unwrap().id = "U12345678".into(),
            _ => c.title = "other".into(),
        };
        assert!(j.save_draft(&d, c).is_err());
    }
    assert!(j.draft(&scope(0)).unwrap().as_ref() == Some(&d));
    let mut changed = d.clone();
    changed.scope.team = "TOTHER".into();
    assert!(conflict(
        j.save_draft(&changed, changed.content.clone()).unwrap()
    ));
    let mut limit = d.clone();
    limit.revision = i64::MAX as u64;
    j.conn
        .execute(
            "UPDATE unsubmitted_drafts SET revision=?1,body=?2 WHERE file=?3",
            params![i64::MAX, encode(&limit).unwrap(), d.scope.file],
        )
        .unwrap();
    assert!(j.save_draft(&limit, limit.content.clone()).is_err());
    assert!(j.claim_draft(scope(0), Some(&limit), initial(0)).is_err());
    assert!(j.draft(&scope(0)).unwrap().as_ref() == Some(&limit));
}
#[test]
fn bounded_capacity_and_explicit_empty_reclamation_never_drop_saved_notes() {
    let t = Temp::new();
    let mut j = t.open();
    let retained = saved(&mut j, 0);
    let mut old_empty = None;
    for n in 1..MAX_DRAFTS {
        old_empty = Some(claim(&mut j, n));
    }
    assert!(matches!(
        j.claim_draft(scope(99), None, initial(99)).unwrap(),
        Claim::Capacity
    ));
    assert_eq!(
        j.cleanup_empty_drafts(&retained.scope.team, &retained.scope.user)
            .unwrap(),
        MAX_DRAFTS - 1
    );
    let old = old_empty.unwrap();
    let mut captured = old.content.clone();
    captured.note = note();
    assert!(conflict(j.save_draft(&old, captured).unwrap()));
    assert!(j.draft(&scope(0)).unwrap().as_ref() == Some(&retained));
    claim(&mut j, 99);
}
#[test]
fn malformed_indexed_body_aborts_all_empty_cleanup_and_lists() {
    let t = Temp::new();
    let mut j = t.open();
    let first = claim(&mut j, 0);
    let second = claim(&mut j, 1);
    j.conn
        .execute(
            "UPDATE unsubmitted_drafts SET token=?1 WHERE file=?2",
            params![uuid::Uuid::new_v4().to_string(), second.scope.file],
        )
        .unwrap();
    assert!(j.drafts(&first.scope.team, &first.scope.user).is_err());
    assert!(
        j.cleanup_empty_drafts(&first.scope.team, &first.scope.user)
            .is_err()
    );
    assert!(j.draft(&first.scope).unwrap().as_ref() == Some(&first));
    assert_eq!(
        j.conn
            .query_row("SELECT COUNT(*) FROM unsubmitted_drafts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn v1_migration_preserves_receipts_cooldown_and_reopens_v2() {
    let t = Temp::new();
    let old = v1(&t.path());
    old.execute(
        "INSERT INTO cooldowns VALUES('T12345678','U12345678',123)",
        [],
    )
    .unwrap();
    let original = Receipt {
        attempt_id: uuid::Uuid::new_v4().to_string(),
        scope: Scope {
            team: "T12345678".into(),
            user: "U12345678".into(),
            file: "FLEGACY".into(),
            destination: "C12345678".into(),
        },
        intent: Intent::new("FLEGACY".into(), "C12345678".into(), Some(note()), None).unwrap(),
        file_title: "Legacy".into(),
        destination_name: "ship".into(),
        created_at: 1,
        updated_at: 2,
        outcome: Some(Outcome::Response(json!({"ok":true}))),
    };
    old.execute(
        "INSERT INTO receipts VALUES(?1,?2,?3,?4,?5,1,2,?6)",
        params![
            original.scope.team,
            original.scope.user,
            original.scope.file,
            original.scope.destination,
            original.attempt_id,
            super::super::encode(&original).unwrap()
        ],
    )
    .unwrap();
    drop(old);
    let mut j = t.open();
    assert!(j.get(&original.scope).unwrap().as_ref() == Some(&original));
    assert_eq!(j.retry_at("T12345678", "U12345678").unwrap(), 123);
    let d = saved(&mut j, 0);
    assert_eq!(
        j.conn
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    drop(j);
    assert!(t.open().draft(&scope(0)).unwrap().as_ref() == Some(&d));
    assert!(Journal::open(t.path(), "3d9c8a3e-7736-42b4-b6cf-d93833d89ef2").is_err());
}
#[test]
fn begin_association_and_accepted_cleanup_survive_reopen_atomically() {
    let t = Temp::new();
    let mut j = t.open();
    let d = saved(&mut j, 0);
    let r = begin(&mut j, &d, 10);
    assert_eq!(links(&j), 1);
    drop(j);
    let mut j = t.open();
    assert!(j.draft(&d.scope).unwrap().as_ref() == Some(&d));
    assert!(matches!(
        j.begin_from_draft(&d, 11).unwrap(),
        Begin::Blocked(_)
    ));
    let result = j
        .finish_result(&r.scope, &r.attempt_id, Ok(json!({"ok":true})), 20)
        .unwrap()
        .unwrap();
    assert!(!result.unresolved());
    assert!(j.draft(&d.scope).unwrap().is_none());
    assert_eq!(links(&j), 0);
    drop(j);
    let j = t.open();
    assert!(j.get(&r.scope).unwrap().as_ref() == Some(&result));
    assert!(j.draft(&d.scope).unwrap().is_none());
}
#[test]
fn begin_failure_after_receipt_insert_rolls_back_receipt_and_association() {
    let t = Temp::new();
    let mut j = t.open();
    let d = saved(&mut j, 0);
    j.conn.execute_batch("CREATE TRIGGER fail_link BEFORE INSERT ON file_share_draft_attempts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(j.begin_from_draft(&d, 10).is_err());
    assert_eq!(links(&j), 0);
    assert!(
        j.unresolved(&d.scope.team, &d.scope.user)
            .unwrap()
            .is_empty()
    );
    assert!(j.draft(&d.scope).unwrap().as_ref() == Some(&d));
    j.conn.execute_batch("DROP TRIGGER fail_link;").unwrap();
    begin(&mut j, &d, 10);
}
#[test]
fn settlement_failure_after_unlink_and_receipt_update_rolls_everything_back() {
    let t = Temp::new();
    let mut j = t.open();
    let d = saved(&mut j, 0);
    let r = begin(&mut j, &d, 10);
    j.conn.execute_batch("CREATE TRIGGER fail_draft_delete BEFORE DELETE ON unsubmitted_drafts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(
        j.finish_result(&r.scope, &r.attempt_id, Ok(json!({"ok":true})), 20)
            .is_err()
    );
    assert!(j.get(&r.scope).unwrap().as_ref() == Some(&r));
    assert!(j.draft(&d.scope).unwrap().as_ref() == Some(&d));
    assert_eq!(links(&j), 1);
    j.conn
        .execute_batch("DROP TRIGGER fail_draft_delete;")
        .unwrap();
    j.retry_result(&r, Ok(json!({"ok":true})), 20)
        .unwrap()
        .unwrap();
    assert!(j.draft(&d.scope).unwrap().is_none());
    assert_eq!(links(&j), 0);
}
#[test]
fn accepted_keeps_newer_claim_or_saved_version_and_other_outcomes_keep_note() {
    for claim_again in [false, true] {
        let t = Temp::new();
        let mut j = t.open();
        let d = saved(&mut j, 0);
        let r = begin(&mut j, &d, 10);
        // Defensive independent replacement: current public mutations below
        // are tested to refuse this exact unresolved version. Simulate a valid
        // newer stored version so cleanup cannot assume it still owns the row.
        let mut next = d.clone();
        next.revision += 1;
        if claim_again {
            next.token = uuid::Uuid::new_v4().to_string();
        } else {
            next.content.note[0]["newer"] = json!(true);
        }
        j.conn
            .execute(
                "UPDATE unsubmitted_drafts SET token=?1,revision=?2,body=?3 WHERE file=?4",
                params![
                    next.token,
                    number(next.revision).unwrap(),
                    encode(&next).unwrap(),
                    next.scope.file
                ],
            )
            .unwrap();
        j.finish_result(&r.scope, &r.attempt_id, Ok(json!({"ok":true})), 20)
            .unwrap();
        assert!(j.draft(&d.scope).unwrap().as_ref() == Some(&next));
        assert_eq!(links(&j), 0);
    }
    for raw in [
        json!({"ok":false,"error":"dlp_user_warning"}),
        json!({"ok":false,"error":"dlp_processing"}),
        json!({"ok":false,"error":"unknown"}),
    ] {
        let t = Temp::new();
        let mut j = t.open();
        let d = saved(&mut j, 0);
        let r = begin(&mut j, &d, 10);
        j.finish_result(&r.scope, &r.attempt_id, Ok(raw), 20)
            .unwrap();
        assert!(j.draft(&d.scope).unwrap().as_ref() == Some(&d));
        assert_eq!(links(&j), 1);
    }
}
#[test]
fn pre_migration_connection_cannot_update_upsert_or_delete_linked_receipt() {
    let t = Temp::new();
    let old = v1(&t.path());
    let mut j = t.open();
    let d = saved(&mut j, 0);
    let r = begin(&mut j, &d, 10);
    for sql in [
        "UPDATE receipts SET updated_at=updated_at+1 WHERE attempt=?1",
        "DELETE FROM receipts WHERE attempt=?1",
        "INSERT INTO receipts SELECT * FROM receipts WHERE attempt=?1 ON CONFLICT(team,user,file,destination) DO UPDATE SET terminal=1",
    ] {
        assert!(old.execute(sql, [&r.attempt_id]).is_err());
    }
    assert!(j.get(&r.scope).unwrap().as_ref() == Some(&r));
    assert!(j.draft(&d.scope).unwrap().as_ref() == Some(&d));
    assert_eq!(links(&j), 1);
    j.cancel_before_dispatch(&r.scope, &r.attempt_id, 20)
        .unwrap();
    assert_eq!(links(&j), 1);
    let r2 = begin(&mut j, &d, 30);
    assert_eq!(links(&j), 1);
    assert_ne!(r2.attempt_id, r.attempt_id);
    j.finish_result(&r2.scope, &r2.attempt_id, Ok(json!({"ok":true})), 40)
        .unwrap();
    assert_eq!(links(&j), 0);
    assert!(
        old.execute(
            "UPDATE receipts SET updated_at=updated_at WHERE attempt=?1",
            [r2.attempt_id]
        )
        .is_ok()
    );
}
#[test]
fn malformed_association_refuses_settlement_without_touching_any_draft() {
    let t = Temp::new();
    let mut j = t.open();
    let d = saved(&mut j, 0);
    let other = saved(&mut j, 1);
    let r = begin(&mut j, &d, 10);
    j.conn
        .execute(
            "UPDATE file_share_draft_attempts SET body=?1 WHERE attempt=?2",
            params![encode(&other).unwrap(), r.attempt_id],
        )
        .unwrap();
    assert!(
        j.finish_result(&r.scope, &r.attempt_id, Ok(json!({"ok":true})), 20)
            .is_err()
    );
    assert!(j.get(&r.scope).unwrap().as_ref() == Some(&r));
    assert!(j.draft(&d.scope).unwrap().as_ref() == Some(&d));
    assert!(j.draft(&other.scope).unwrap().as_ref() == Some(&other));
}
#[test]
fn late_terminal_self_prune_cleans_only_accepted_and_never_restores_orphan() {
    for accepted in [false, true] {
        let t = Temp::new();
        let mut j = t.open();
        let d = saved(&mut j, 0);
        let delayed = begin(&mut j, &d, 10);
        for n in 1..=MAX_TERMINAL {
            let s = Scope {
                team: d.scope.team.clone(),
                user: d.scope.user.clone(),
                file: format!("FNEW{n}"),
                destination: "C12345678".into(),
            };
            let intent = Intent::new(s.file.clone(), s.destination.clone(), None, None).unwrap();
            let r = match j
                .begin(s, intent, "New".into(), "ship".into(), 100 + n as u64)
                .unwrap()
            {
                Begin::Started(r) => r,
                _ => panic!("begin"),
            };
            j.finish_result(
                &r.scope,
                &r.attempt_id,
                Ok(json!({"ok":true})),
                100 + n as u64,
            )
            .unwrap();
        }
        if accepted {
            j.finish_result(
                &delayed.scope,
                &delayed.attempt_id,
                Ok(json!({"ok":true})),
                20,
            )
            .unwrap();
        } else {
            j.cancel_before_dispatch(&delayed.scope, &delayed.attempt_id, 20)
                .unwrap();
        }
        assert!(j.get(&delayed.scope).unwrap().is_none());
        assert_eq!(links(&j), 0);
        assert_eq!(j.draft(&d.scope).unwrap().is_none(), accepted);
    }
}

fn applied<T>(result: Mutation<T>) -> T {
    match result {
        Mutation::Applied(value) => value,
        _ => panic!("Expected exact committed mutation"),
    }
}
fn conflict<T>(result: Mutation<T>) -> bool {
    matches!(result, Mutation::Conflict(_))
}

#[test]
fn exact_unresolved_link_blocks_mutations_and_destination_change_inside_transaction() {
    let t = Temp::new();
    let mut j = t.open();
    let d = saved(&mut j, 0);
    let r = begin(&mut j, &d, 10);
    assert!(
        matches!(j.claim_draft(d.scope.clone(),Some(&d),initial(0)).unwrap(),Claim::Blocked(ref receipt) if receipt==&r)
    );
    let mut changed = d.content.clone();
    changed.destination.as_mut().unwrap().id = "C87654321".into();
    assert!(
        matches!(j.save_draft(&d,changed).unwrap(),Mutation::Blocked(ref receipt) if receipt==&r)
    );
    assert!(matches!(j.discard_draft(&d).unwrap(),Mutation::Blocked(ref receipt) if receipt==&r));
    assert!(j.draft(&d.scope).unwrap().as_ref() == Some(&d));
    // A historical terminal association is not an unresolved edit prohibition.
    j.cancel_before_dispatch(&r.scope, &r.attempt_id, 20)
        .unwrap();
    let newer = match j
        .claim_draft(d.scope.clone(), Some(&d), initial(0))
        .unwrap()
    {
        Claim::Claimed(d) => d,
        _ => panic!("terminal claim"),
    };
    let mut content = newer.content.clone();
    content.destination.as_mut().unwrap().id = "C87654321".into();
    let newer = applied(j.save_draft(&newer, content).unwrap());
    let r2 = begin(&mut j, &newer, 30);
    assert_ne!(r2.scope.destination, r.scope.destination);
    // Old recorded NotSent cannot be upgraded by a late accepted callback.
    j.finish_result(&r.scope, &r.attempt_id, Ok(json!({"ok":true})), 40)
        .unwrap();
    assert!(j.draft(&d.scope).unwrap().as_ref() == Some(&newer));
}
#[test]
fn association_orphan_wrong_index_and_overcapacity_refuse_unrelated_mutations() {
    for defect in [0, 1, 2] {
        let t = Temp::new();
        let mut j = t.open();
        let d = saved(&mut j, 0);
        let other = claim(&mut j, 1);
        let r = begin(&mut j, &d, 10);
        match defect {
            0 => {
                j.conn
                    .execute(
                        "UPDATE file_share_draft_attempts SET attempt=?1 WHERE attempt=?2",
                        params![uuid::Uuid::new_v4().to_string(), r.attempt_id],
                    )
                    .unwrap();
            }
            1 => {
                j.conn
                    .execute(
                        "UPDATE file_share_draft_attempts SET team='TOTHER' WHERE attempt=?1",
                        [r.attempt_id],
                    )
                    .unwrap();
            }
            _ => {
                for _ in 0..MAX_UNRESOLVED + MAX_TERMINAL {
                    j.conn.execute("INSERT INTO file_share_draft_attempts SELECT ?1,team,user,file,token,revision,body FROM file_share_draft_attempts LIMIT 1",[uuid::Uuid::new_v4().to_string()]).unwrap();
                }
            }
        }
        assert!(
            j.cleanup_empty_drafts(&other.scope.team, &other.scope.user)
                .is_err()
        );
        assert!(j.save_draft(&other, other.content.clone()).is_err());
        assert!(j.draft(&other.scope).unwrap().as_ref() == Some(&other));
        assert!(j.draft(&d.scope).unwrap().as_ref() == Some(&d));
    }
}
#[test]
fn v2_reopen_refuses_missing_or_replaced_linked_receipt_guards() {
    for replaced in [false, true] {
        let t = Temp::new();
        drop(t.open());
        let conn = Connection::open(t.path()).unwrap();
        conn.execute_batch("DROP TRIGGER file_share_linked_update;")
            .unwrap();
        if replaced {
            conn.execute_batch("CREATE TRIGGER file_share_linked_update BEFORE UPDATE ON receipts BEGIN SELECT 1; END;").unwrap();
        }
        drop(conn);
        assert!(Journal::open(t.path(), OWNER).is_err());
    }
}

#[test]
fn pruning_linked_terminal_receipt_keeps_unsent_draft_and_validates_link_before_delete() {
    for corrupt in [false, true] {
        let t = Temp::new();
        let mut j = t.open();
        let d = saved(&mut j, 0);
        let old = begin(&mut j, &d, 10);
        j.cancel_before_dispatch(&old.scope, &old.attempt_id, 20)
            .unwrap();
        for n in 1..MAX_TERMINAL {
            let s = Scope {
                team: d.scope.team.clone(),
                user: d.scope.user.clone(),
                file: format!("FRECENT{n}"),
                destination: "C12345678".into(),
            };
            let intent = Intent::new(s.file.clone(), s.destination.clone(), None, None).unwrap();
            let r = match j
                .begin(s, intent, "New".into(), "ship".into(), 100 + n as u64)
                .unwrap()
            {
                Begin::Started(r) => r,
                _ => panic!("begin"),
            };
            j.finish_result(
                &r.scope,
                &r.attempt_id,
                Ok(json!({"ok":true})),
                100 + n as u64,
            )
            .unwrap();
        }
        if corrupt {
            j.conn
                .execute(
                    "UPDATE file_share_draft_attempts SET revision=revision+1 WHERE attempt=?1",
                    [&old.attempt_id],
                )
                .unwrap();
        }
        let s = Scope {
            team: d.scope.team.clone(),
            user: d.scope.user.clone(),
            file: "FLAST".into(),
            destination: "C12345678".into(),
        };
        let intent = Intent::new(s.file.clone(), s.destination.clone(), None, None).unwrap();
        let last = match j
            .begin(s, intent, "New".into(), "ship".into(), 500)
            .unwrap()
        {
            Begin::Started(r) => r,
            _ => panic!("begin"),
        };
        let result = j.finish_result(&last.scope, &last.attempt_id, Ok(json!({"ok":true})), 501);
        assert_eq!(result.is_err(), corrupt);
        assert_eq!(j.get(&old.scope).unwrap().is_some(), corrupt);
        assert_eq!(links(&j), i64::from(corrupt));
        assert!(j.draft(&d.scope).unwrap().as_ref() == Some(&d));
        if corrupt {
            assert!(j.get(&last.scope).unwrap().as_ref() == Some(&last));
        }
    }
}

#[test]
fn two_current_connections_preserve_ordered_claims_and_equal_text_aba() {
    for save_first in [false, true] {
        let t = Temp::new();
        let mut first = t.open();
        let original = saved(&mut first, 0);
        let mut second = t.open();
        let observed = second.draft(&original.scope).unwrap().unwrap();
        assert!(observed == original);
        let mut captured = original.content.clone();
        captured.note[0]["captured_newer"] = json!(true);
        let base = if save_first {
            let committed = applied(first.save_draft(&original, captured.clone()).unwrap());
            match second
                .claim_draft(original.scope.clone(), Some(&observed), initial(0))
                .unwrap()
            {
                Claim::Conflict(Some(actual)) => assert!(actual == committed),
                _ => panic!("old claim must conflict"),
            }
            committed
        } else {
            original.clone()
        };
        let claimed = match second
            .claim_draft(base.scope.clone(), Some(&base), initial(0))
            .unwrap()
        {
            Claim::Claimed(d) => d,
            _ => panic!("exact claim"),
        };
        assert!(claimed.content == base.content);
        let rejected = first.save_draft(&base, captured.clone()).unwrap();
        match rejected {
            Mutation::Conflict(Some(actual)) => assert!(actual == claimed),
            _ => panic!("old save must conflict"),
        }
        assert!(matches!(
            first.discard_draft(&base).unwrap(),
            Mutation::Conflict(Some(_))
        ));
        assert!(captured.note[0]["captured_newer"] == true);
        assert!(second.draft(&base.scope).unwrap().as_ref() == Some(&claimed));
        // Identical text/body saved at a newer revision still rejects the old version.
        let same_text = applied(
            second
                .save_draft(&claimed, claimed.content.clone())
                .unwrap(),
        );
        assert!(same_text.content == claimed.content);
        assert!(same_text.revision > claimed.revision);
        assert!(matches!(
            first.save_draft(&claimed, claimed.content.clone()).unwrap(),
            Mutation::Conflict(Some(_))
        ));
        assert!(matches!(
            first.discard_draft(&claimed).unwrap(),
            Mutation::Conflict(Some(_))
        ));
        assert!(first.draft(&base.scope).unwrap().as_ref() == Some(&same_text));
    }
}

mod direct;
