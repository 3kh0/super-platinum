use super::*;

#[test]
fn retained_legacy_capture_keeps_original_visit_when_a_new_visit_is_created() {
    let (shell, target) = sample();
    let source = admission::Source::capture(&shell, &target).unwrap();
    let mut held = recovery::Custody::default();
    let old = held.visit(&source).unwrap();
    assert!(held.capture_note(&old, note(), None).unwrap());
    let newer = held.visit(&source).unwrap();
    assert!(!old.active() && newer.active());
    let saved = &held.drafts[&old.key];
    assert!(Arc::ptr_eq(&saved.visit, &old));
    assert_eq!(saved.note, note());
    assert!(!held.capture_note(&old, vec![], None).unwrap());
    assert!(Arc::ptr_eq(&held.drafts[&old.key].visit, &old));
}
#[test]
fn abandoned_empty_visits_reclaim_capacity_but_pending_reads_and_captured_notes_remain_charged() {
    let (shell, target) = sample();
    let base = admission::Source::capture(&shell, &target).unwrap();
    let mut held = recovery::Custody::default();
    for n in 0..100 {
        let mut source = base.clone();
        source.id = format!("FEMPTY{n}");
        let visit = held.visit(&source).unwrap();
        drop(visit);
        assert!(held.drafts.is_empty());
    }
    let mut live = vec![];
    for n in 0..32 {
        let mut source = base.clone();
        source.id = format!("FLIVE{n}");
        live.push(held.visit(&source).unwrap());
    }
    assert!(matches!(held.visit(&base), Err(Refusal::Capacity)));
    let read = live.pop().unwrap();
    held.capture_note(&read, note(), None).unwrap();
    drop(read);
    drop(live);
    assert!(held.visit(&base).is_ok());
    assert_eq!(held.drafts.len(), 1);
    assert!(held.draft_visits.len() <= 2);
}

#[test]
fn accepted_receipt_clears_only_exact_submitted_visit_and_frees_capacity() {
    let temp = Temp::new();
    let (shell, target) = sample();
    let source = admission::Source::capture(&shell, &target).unwrap();
    let mut held = recovery::Custody::default();
    let destination = Destination {
        id: "C00000001".into(),
        name: "ship".into(),
        private: false,
    };
    let mut journal = Journal::open(temp.0.join("receipts.sqlite"), ACCOUNT).unwrap();
    for index in 0..40 {
        let mut source = source.clone();
        source.id = format!("FFILE{index}");
        source.file["id"] = json!(source.id);
        let visit = held.visit(&source).unwrap();
        held.capture_note(&visit, note(), Some(destination.clone()))
            .unwrap();
        let scope = Scope {
            team: source.context.key.team.clone(),
            user: source.context.key.user.clone(),
            file: source.id.clone(),
            destination: destination.id.clone(),
        };
        let intent = Intent::new(
            scope.file.clone(),
            scope.destination.clone(),
            Some(note()),
            None,
        )
        .unwrap();
        let Begin::Started(receipt) = journal
            .begin(scope.clone(), intent, "file".into(), "ship".into(), 10)
            .unwrap()
        else {
            panic!("new")
        };
        held.accepted_note(&visit, &receipt);
        assert_eq!(held.drafts.len(), 1);
        let accepted = journal
            .finish_result(&scope, &receipt.attempt_id, Ok(json!({"ok":true})), 11)
            .unwrap()
            .unwrap();
        held.accepted_note(&visit, &accepted);
        assert!(held.drafts.is_empty() && held.draft_visits.is_empty());
        assert!(
            !held
                .capture_note(&visit, note(), Some(destination.clone()))
                .unwrap()
        );
    }
    let visit = held.visit(&source).unwrap();
    held.capture_note(&visit, note(), Some(destination.clone()))
        .unwrap();
    let newer = held.visit(&source).unwrap();
    held.capture_note(&newer, note(), Some(destination.clone()))
        .unwrap();
    let scope = Scope {
        team: source.context.key.team.clone(),
        user: source.context.key.user.clone(),
        file: source.id.clone(),
        destination: destination.id.clone(),
    };
    let intent = Intent::new(
        scope.file.clone(),
        scope.destination.clone(),
        Some(note()),
        None,
    )
    .unwrap();
    let Begin::Started(receipt) = journal
        .begin(scope.clone(), intent, "file".into(), "ship".into(), 12)
        .unwrap()
    else {
        panic!("new")
    };
    let accepted = journal
        .finish_result(&scope, &receipt.attempt_id, Ok(json!({"ok":true})), 13)
        .unwrap()
        .unwrap();
    held.accepted_note(&visit, &accepted);
    assert_eq!(held.drafts.len(), 1);
    assert!(Arc::ptr_eq(
        &held.drafts.values().next().unwrap().visit,
        &newer
    ));
}

#[tokio::test]
async fn preparing_without_receipt_cannot_be_synchronously_replaced_or_dismissed() {
    let temp = Temp::new();
    let (mut shell, target) = sample();
    let id = open(&mut shell, target.clone(), "source".into(), &temp)
        .await
        .unwrap();
    shell.file_share.dialog.as_mut().unwrap().phase = Phase::Preparing;
    assert_eq!(
        open(&mut shell, target, "other".into(), &temp).await,
        Err(Refusal::Busy)
    );
    dismiss(&mut shell);
    assert_eq!(visible(&shell).unwrap().id, id);
}
