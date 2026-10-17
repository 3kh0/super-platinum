use super::*;
fn choose(a: &Attempt) -> RetirementSelection {
    RetirementSelection::new(
        a,
        match &a.delivery {
            Delivery::Created { file_id } => Some(file_id.as_str()),
            _ => None,
        },
    )
    .unwrap()
}
fn retire(store: &mut Store, a: &Attempt) -> RetirementReceipt {
    let r = store.retirement_review(&a.owner).unwrap();
    store.retire_terminal(&r, &[choose(a)]).unwrap()
}
fn archived_created(store: &mut Store, temp: &Temp) -> (Draft, Attempt, Attempt, ArchiveReceipt) {
    let base = seed(temp, store);
    let (draft, started) = start(store, &base);
    let decision = store
        .archive_and_new(
            &draft,
            &started,
            ArchiveReason::AbandonedOutcomeUnknown,
            &writer(),
            &writer(),
            100,
        )
        .unwrap();
    let created = store
        .settle(
            &started,
            Outcome::Created {
                file_id: "F123".into(),
            },
        )
        .unwrap();
    (draft, started, created, decision)
}
#[test]
fn terminal_cycles_release_content_capacity_keep_old_unknown_and_exact_current_text() {
    let temp = Temp::new();
    let mut s = temp.store();
    let base = seed(&temp, &mut s);
    let (d, old) = start(&mut s, &base);
    s.archive_and_new(
        &d,
        &old,
        ArchiveReason::AbandonedOutcomeUnknown,
        &writer(),
        &writer(),
        100,
    )
    .unwrap();
    let empty = s.active(&temp.owner()).unwrap().unwrap();
    let mut current = s
        .save(&empty, &writer(), content("Never clear this Unicode text"))
        .unwrap();
    for _ in 0..256 {
        let r = s.reserve(&current, 100).unwrap();
        let canceled = s.cancel_reserved(&r).unwrap();
        assert!(retire(&mut s, &canceled).newly_committed());
        let next = s.active(&temp.owner()).unwrap().unwrap();
        assert!(
            next.content == current.content
                && next.writer == current.writer
                && next.revision == current.revision
        );
        assert!(s.admit(&r, 100).is_err());
        current = next;
    }
    assert_eq!(count(&s.conn, "retired_attempts").unwrap(), 256);
    assert_eq!(count(&s.conn, "attempts").unwrap(), 1);
    s.settle(
        &old,
        Outcome::Created {
            file_id: "FOLD".into(),
        },
    )
    .unwrap();
    assert!(s.active(&temp.owner()).unwrap() == Some(current));
}
#[test]
fn late_created_after_unknown_archive_is_reviewed_without_second_new_draft() {
    let temp = Temp::new();
    let mut s = temp.store();
    let (_, started, created, decision) = archived_created(&mut s, &temp);
    let next = s.active(&temp.owner()).unwrap().unwrap();
    let review = s.retirement_review(&temp.owner()).unwrap();
    assert!(RetirementSelection::new(&created, None).is_err());
    assert!(RetirementSelection::new(&created, Some("FOTHER")).is_err());
    let selected = choose(&created);
    let receipt = s.retire_terminal(&review, &[selected.clone()]).unwrap();
    assert!(receipt.newly_committed());
    assert!(
        !s.retire_terminal(&review, &[selected])
            .unwrap()
            .newly_committed()
    );
    assert!(s.active(&temp.owner()).unwrap() == Some(next));
    assert!(s.archived(&temp.owner()).unwrap().is_empty());
    assert_eq!(count(&s.conn, "drafts").unwrap(), 1);
    let recorded = s
        .retired_archive_receipt(&temp.owner(), decision.operation())
        .unwrap()
        .unwrap();
    assert!(recorded == decision && recorded.reason() == ArchiveReason::AbandonedOutcomeUnknown);
    let terminal = &receipt.retired()[0];
    assert!(
        terminal
            .same_result(
                &started,
                Outcome::Created {
                    file_id: "F123".into()
                }
            )
            .unwrap()
    );
    assert!(
        !terminal
            .same_result(
                &started,
                Outcome::Created {
                    file_id: "FOTHER".into()
                }
            )
            .unwrap()
    );
    assert!(
        s.settle(
            &started,
            Outcome::Created {
                file_id: "F123".into()
            }
        )
        .is_err()
    );
    drop(s);
    let fresh = temp.store();
    assert!(
        fresh
            .retired_attempt(&temp.owner(), created.id())
            .unwrap()
            .as_ref()
            == Some(terminal)
    );
}
#[test]
fn rate_retirement_preserves_cooldown_and_newer_canonical_revision() {
    let temp = Temp::new();
    let mut s = temp.store();
    let base = seed(&temp, &mut s);
    let (d, a) = start(&mut s, &base);
    let rate = s
        .settle(&a, Outcome::RateLimited { retry_at: 500 })
        .unwrap();
    let newer = s
        .save(&d, &writer(), content("Independent later revision"))
        .unwrap();
    retire(&mut s, &rate);
    assert!(s.active(&temp.owner()).unwrap() == Some(newer.clone()));
    assert_eq!(s.retry_at(&temp.owner()).unwrap(), 500);
    assert!(matches!(s.reserve(&newer, 499), Err(Fault::Cooldown)));
    assert!(s.reserve(&newer, 500).is_ok());
}
#[test]
fn unexpected_result_is_typed_durable_and_disposal_cannot_resurrect() {
    let temp = Temp::new();
    let mut s = temp.store();
    let (_, original, created, _) = archived_created(&mut s, &temp);
    let r = retire(&mut s, &created);
    let terminal = &r.retired()[0];
    let op = writer();
    let w = writer();
    let evidence = s
        .preserve_late_result(
            &original,
            terminal,
            Outcome::Created {
                file_id: "FOTHER".into(),
            },
            &w,
            &op,
        )
        .unwrap();
    assert!(evidence.original().content() == original.content());
    assert!(matches!(evidence.incoming(),Delivery::Created{file_id} if file_id=="FOTHER"));
    assert!(
        s.preserve_late_result(
            &original,
            terminal,
            Outcome::Created {
                file_id: "FOTHER".into()
            },
            &w,
            &op
        )
        .unwrap()
            == evidence
    );
    drop(s);
    let mut fresh = temp.store();
    assert!(fresh.late_results(&temp.owner()).unwrap() == vec![evidence.clone()]);
    let empty = fresh.active(&temp.owner()).unwrap().unwrap();
    let live = fresh.save(&empty, &writer(), content("New")).unwrap();
    assert!(matches!(fresh.reserve(&live, 100), Err(Fault::Blocked)));
    fresh.dispose_late_result(&evidence).unwrap();
    assert!(matches!(
        fresh.preserve_late_result(
            &original,
            terminal,
            Outcome::Created {
                file_id: "FOTHER".into()
            },
            &w,
            &op
        ),
        Err(Fault::Retired)
    ));
    assert!(fresh.reserve(&live, 100).is_ok());
}
#[test]
fn evidence_and_conflicts_share_sixteen_slots_and_failed_persistence_does_not_replace() {
    let temp = Temp::new();
    let mut s = temp.store();
    let (base, original, created, _) = archived_created(&mut s, &temp);
    let r = retire(&mut s, &created);
    for _ in 0..15 {
        s.preserve_conflict(&base, &writer(), &writer(), content("Held"))
            .unwrap();
    }
    let e = s
        .preserve_late_result(
            &original,
            &r.retired()[0],
            Outcome::Created {
                file_id: "FOTHER".into(),
            },
            &writer(),
            &writer(),
        )
        .unwrap();
    assert!(matches!(
        s.preserve_conflict(&base, &writer(), &writer(), content("17")),
        Err(Fault::Capacity)
    ));
    assert!(matches!(
        s.preserve_late_result(
            &original,
            &r.retired()[0],
            Outcome::Created {
                file_id: "FOTHER2".into()
            },
            &writer(),
            &writer()
        ),
        Err(Fault::Capacity)
    ));
    assert!(s.late_results(&temp.owner()).unwrap() == vec![e]);
}
#[test]
fn inactive_owner_metadata_has_no_lifetime_limit_and_late_capture_survives() {
    let temp = Temp::new();
    let mut s = temp.store();
    let base = seed(&temp, &mut s);
    s.discard(&base).unwrap();
    for i in 0..100 {
        let owner = Owner::new(&temp.account, &format!("TX{i}"), "U1").unwrap();
        let d = s.create(&owner, &writer(), content("Temporary")).unwrap();
        s.discard(&d).unwrap();
    }
    assert_eq!(count(&s.conn, "scopes").unwrap(), 101);
    assert_eq!(active_scope_count(&s.conn).unwrap(), 0);
    s.preserve_conflict(&base, &writer(), &writer(), content("Late captured edit"))
        .unwrap();
    drop(s);
    let fresh = temp.store();
    assert_eq!(fresh.conflicts(&temp.owner()).unwrap().len(), 1);
}
#[test]
fn paged_history_is_owner_scoped_bounded_and_uses_index() {
    let temp = Temp::new();
    let mut s = temp.store();
    let mut d = seed(&temp, &mut s);
    for _ in 0..70 {
        let r = s.reserve(&d, 100).unwrap();
        let a = s.cancel_reserved(&r).unwrap();
        retire(&mut s, &a);
        d = s.active(&temp.owner()).unwrap().unwrap();
    }
    let first = s.retired_attempt_page(&temp.owner(), None, 64).unwrap();
    assert_eq!(first.len(), 64);
    let next = s
        .retired_attempt_page(&temp.owner(), Some(first.last().unwrap().id()), 64)
        .unwrap();
    assert_eq!(next.len(), 6);
    assert!(first.last().unwrap().id() < next.first().unwrap().id());
    let other = Owner::new(&temp.account, "TOTHER", "UOTHER").unwrap();
    assert!(s.retired_attempt_page(&other, None, 64).unwrap().is_empty());
    assert!(s.retired_attempt_page(&temp.owner(), None, 65).is_err());
    let plans:Vec<String>=s.conn.prepare("EXPLAIN QUERY PLAN SELECT id,body FROM retired_attempts WHERE owner_key=?1 AND id>?2 ORDER BY id LIMIT ?3").unwrap().query_map(params![temp.owner().key(),"",64],|r|r.get(3)).unwrap().map(|r|r.unwrap()).collect();
    assert!(
        plans
            .iter()
            .any(|p| p.contains("SEARCH") && p.contains("retired_attempt_owner"))
    );
}
