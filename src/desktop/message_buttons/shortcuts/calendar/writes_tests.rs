use super::*;
#[test]
fn duplicate_unknown_writes_are_unreplayed_but_new_explicit_drafts_are_distinct() {
    let (mut s, id) = super::super::tests::setup();
    let e = actions::event(&s, id).unwrap();
    let (ctx, attempt) = admit(&mut s, id, Write::Event(e.clone())).unwrap();
    assert!(admit(&mut s, id, Write::Event(e.clone())).is_none());
    settle(&mut s, &ctx, attempt, Settlement::Unknown);
    assert!(admit(&mut s, id, Write::Event(e.clone())).is_none());
    let mut different = e;
    different.title.push_str(" different");
    assert!(admit(&mut s, id, Write::Event(different)).is_some());
    assert_eq!(s.message_buttons.shortcuts.calendar_attempts.len(), 2);
}
#[test]
fn late_result_settles_original_attempt_after_close_without_touching_new_dialog() {
    let (mut s, id) = super::super::tests::setup();
    let e = actions::event(&s, id).unwrap();
    let (ctx, attempt) = admit(&mut s, id, Write::Event(e)).unwrap();
    close(&mut s, id, true);
    fixture::install(&mut s, "app-calendar-modern");
    // Installing a synthetic fixture replaces transport; restore the captured real
    // owner for this test to isolate UI retirement from account retirement.
    s.core.transport = Some(ctx.transport.clone());
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .insert(ctx.session.team_id.clone(), ctx.session.clone());
    let newer = s.message_buttons.shortcuts.dialog.as_ref().unwrap().id;
    settle(
        &mut s,
        &ctx,
        attempt,
        Settlement::Created(Some("https://calendar.example.test/event".into())),
    );
    assert!(matches!(
        s.message_buttons.shortcuts.calendar_attempts[0].result,
        Settlement::Created(_)
    ));
    assert_eq!(
        s.message_buttons.shortcuts.dialog.as_ref().unwrap().id,
        newer
    );
    assert_eq!(
        s.message_buttons
            .shortcuts
            .dialog
            .as_ref()
            .unwrap()
            .calendar
            .as_ref()
            .unwrap()
            .stage,
        Stage::Editing
    );
}
#[test]
fn wrong_account_cannot_settle_and_unknown_book_does_not_evict_to_admit() {
    let (mut s, id) = super::super::tests::setup();
    let e = actions::event(&s, id).unwrap();
    let (ctx, attempt) = admit(&mut s, id, Write::Event(e.clone())).unwrap();
    s.core.account_epoch += 1;
    settle(&mut s, &ctx, attempt, Settlement::Created(None));
    assert_eq!(
        s.message_buttons.shortcuts.calendar_attempts[0].result,
        Settlement::Sending
    );
    s.core.account_epoch -= 1;
    settle(&mut s, &ctx, attempt, Settlement::Unknown);
    for i in 1..64 {
        let mut e = e.clone();
        e.title = i.to_string();
        let (c, a) = admit(&mut s, id, Write::Event(e)).unwrap();
        settle(&mut s, &c, a, Settlement::Unknown);
    }
    let mut other = e;
    other.title = "65".into();
    assert!(admit(&mut s, id, Write::Event(other)).is_none());
    assert_eq!(s.message_buttons.shortcuts.calendar_attempts.len(), 64);
}
#[test]
fn response_outcomes_preserve_lost_write_and_rate_limit_distinction() {
    assert_eq!(
        response(&Err(Error::Transport("lost".into())), false),
        Settlement::Unknown
    );
    assert_eq!(
        response(
            &Err(Error::RateLimited {
                retry_after_secs: Some(30)
            }),
            false
        ),
        Settlement::Unsent
    );
    assert_eq!(
        response(
            &Err(Error::Api("block_interaction_not_sent_offline".into())),
            true
        ),
        Settlement::Unsent
    );
    assert_eq!(
        response(&Ok(serde_json::json!({"ok":true})), true),
        Settlement::Unknown
    );
    assert_eq!(
        response(
            &Ok(serde_json::json!({"ok":false,"error":"invalid_date"})),
            false
        ),
        Settlement::Rejected
    );
}

#[test]
fn reopening_unknown_preserves_exact_draft_but_does_not_restore_write_admission() {
    let (mut s, id) = super::super::tests::setup();
    let e = actions::event(&s, id).unwrap();
    let (ctx, attempt) = admit(&mut s, id, Write::Event(e.clone())).unwrap();
    settle(&mut s, &ctx, attempt, Settlement::Unknown);
    let a = s.message_buttons.shortcuts.calendar_attempts[0].clone();
    let mut c = Calendar::new(a.provider, "C_OTHER".into(), vec![], vec![]);
    restore(&mut c, &a, &s, &ctx);
    c.stage = Stage::Uncertain;
    assert_eq!(c.title, e.title);
    assert_eq!(c.description, e.description);
    assert_eq!(c.start, Some(e.start));
    assert_eq!(c.end, Some(e.end));
    assert_eq!(
        c.guests.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
        e.attendees
    );
    s.message_buttons
        .shortcuts
        .dialog
        .as_mut()
        .unwrap()
        .calendar = Some(c);
    assert!(actions::event(&s, id).is_none());
    assert!(admit(&mut s, id, Write::Event(e)).is_none());
}
