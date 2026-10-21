use super::*;
#[test]
fn exact_singleton_issued_429_allows_only_explicit_retry_after_cooldown() {
    let mut s = setup();
    let work = start(&mut s);
    dispatch(&mut s, &work);
    let error = Error::RateLimited {
        retry_after_secs: Some(2),
    };
    let deadline = failure_deadline(&error);
    settle(&work, Err(ListCommentReactionFailure::Unconfirmed(error)));
    work.update(|a| a.retry_at = deadline);
    finish(&mut s, &work);
    assert_eq!(work.attempt().unwrap().phase.label(), "rejected");
    assert_eq!(work.attempt().unwrap().issued, Some(true));
    assert!(!available(&s, generation(&s), TS, "wave"));
    assert!(!own(&s, &work.user));
    work.update(|a| a.retry_at = now() - 1);
    assert!(available(&s, generation(&s), TS, "wave"));
    assert!(!own(&s, &work.user));
}
const BASE: &str = "thumbsup";
const TONE: &str = "thumbsup::skin-tone-3";
const THIRD: &str = "thumbsup::skin-tone-2";
const SELECTED: &str = "thumbsup::skin-tone-6";
const ALIAS: &str = "+1::skin-tone-3";
fn family_setup() -> ShellState {
    let mut s = setup();
    let user = s
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .owner
        .scope
        .stable_owner()
        .2;
    let mut row = raw(vec![]);
    row["reactions"] = json!([
        {"name":BASE,"count":2,"users":[user,"U_OTHER"]},
        {"name":TONE,"count":1,"users":[user]},
        {"name":THIRD,"count":1,"users":[user]},
        {"name":SELECTED,"count":1,"users":["U_OTHER"]},
        {"name":ALIAS,"count":1,"users":[user]},
        {"name":"wave","count":1,"users":["U_OTHER"]}
    ]);
    load(&mut s, row);
    s
}
fn start_family(s: &mut ShellState) -> (Work, Vec<Work>) {
    let g = generation(s);
    let parent = begin(s, g, TS, SELECTED).unwrap();
    let entry = s.slack_lists.comments.entry.as_ref().unwrap();
    let page = entry.page.as_ref().unwrap();
    let mut intent = ReactionFamilyIntent::capture(
        &entry.target,
        page.root().unwrap(),
        message(entry, TS).unwrap(),
        json!({"id":entry.target.channel_id()}),
        &parent.user,
        SELECTED,
    )
    .unwrap();
    let mut intents = vec![];
    while let Some(i) = intent.take_next() {
        intents.push(i);
    }
    let children = family::children(&parent, &intents).unwrap();
    s.slack_lists.comments.entry.as_mut().unwrap().loading = false;
    (parent, children)
}
fn event(s: &mut ShellState, w: &Work, name: &str, user: &str, add: bool) -> bool {
    let frame = if add {
        RtEvent::ReactionAdded {
            channel: w.target.channel_id().into(),
            ts: TS.into(),
            user: user.into(),
            reaction: name.into(),
        }
    } else {
        RtEvent::ReactionRemoved {
            channel: w.target.channel_id().into(),
            ts: TS.into(),
            user: user.into(),
            reaction: name.into(),
        }
    };
    reactions::apply(s, "T1", 7, &frame)
}
fn mine(s: &ShellState, w: &Work, name: &str) -> bool {
    member(
        message(s.slack_lists.comments.entry.as_ref().unwrap(), TS).unwrap(),
        &w.user,
        name,
    )
}
fn ack(s: &mut ShellState, w: &Work) {
    w.update(|a| a.phase = Phase::Sent);
    settle(w, Ok(json!({"ok":true})));
    finish(s, w);
}

#[test]
fn unowned_selection_fixed_children_alias_exclusion_and_socket_before_ack() {
    let mut s = family_setup();
    let (parent, children) = start_family(&mut s);
    assert_eq!(
        children.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(),
        vec![BASE, TONE, THIRD]
    );
    assert!(!available(&s, generation(&s), TS, BASE));
    assert!(!available(&s, generation(&s), TS, SELECTED));
    assert!(available(&s, generation(&s), TS, ALIAS));
    assert!(event(&mut s, &parent, BASE, &parent.user, false));
    assert!(!event(&mut s, &parent, BASE, &parent.user, false));
    ack(&mut s, &children[0]);
    assert!(children[1].current(&s));
    // Actual unrelated reaction counts are preserved in the fixed shadow.
    assert!(event(&mut s, &parent, BASE, "U_NEW", true));
    assert!(children[1].current(&s));
    ack(&mut s, &children[1]);
    assert!(children[2].current(&s));
    ack(&mut s, &children[2]);
    for name in [BASE, TONE, THIRD] {
        assert!(!mine(&s, &parent, name));
    }
    assert!(mine(&s, &parent, ALIAS));
    let base = message(s.slack_lists.comments.entry.as_ref().unwrap(), TS)
        .unwrap()
        .reactions
        .iter()
        .find(|r| r.name == BASE)
        .unwrap();
    assert_eq!(base.count, 2);
    assert_eq!(base.users, vec!["U_OTHER", "U_NEW"]);
}
#[test]
fn ack_then_unknown_keeps_success_and_stops_queued_without_replay_after_reopen() {
    let mut s = family_setup();
    let (parent, children) = start_family(&mut s);
    ack(&mut s, &children[0]);
    children[1].update(|a| a.phase = Phase::Sent);
    settle(
        &children[1],
        Err(ListCommentReactionFailure::Unconfirmed(unavailable())),
    );
    family::stop(&parent);
    finish(&mut s, &children[1]);
    assert!(!mine(&s, &parent, BASE));
    assert!(mine(&s, &parent, TONE));
    assert_eq!(children[2].attempt().unwrap().phase.label(), "not-sent");
    assert_eq!(children[2].attempt().unwrap().issued, Some(false));
    let mut loaded = family_setup();
    carry_pending(&mut s, &mut loaded);
    assert!(!available(&loaded, generation(&loaded), TS, SELECTED));
    assert!(
        status(&loaded, generation(&loaded))
            .unwrap()
            .contains("1 confirmed, 1 unconfirmed, 1 not sent")
    );
    let full = json!({"ok":true,"type":"message","channel":"CCOMMENT01","message":{"ts":TS,"thread_ts":ROOT,"reactions":[{"name":THIRD,"count":1,"users":[parent.user]},{"name":SELECTED,"count":1,"users":["U_OTHER"]}]}});
    let owner = loaded
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .owner
        .clone();
    apply_readback(
        &mut loaded,
        &owner,
        &[recovery::Observation {
            id: children[1].id,
            revision: 0,
            deadline: 0,
            raw: Some(full),
        }],
    );
    assert!(available(&loaded, generation(&loaded), TS, SELECTED));
    let g = generation(&loaded);
    let next = begin(&mut loaded, g, TS, SELECTED).unwrap();
    assert_eq!(next.family.unwrap().names, vec![THIRD]);
    assert_eq!(children[0].attempt().unwrap().phase.label(), "confirmed");
}
#[test]
fn opposite_or_new_self_family_membership_poison_remaining_authority() {
    for opposite in [true, false] {
        let mut s = family_setup();
        let (parent, children) = start_family(&mut s);
        if opposite {
            assert!(event(&mut s, &parent, BASE, &parent.user, false));
            assert!(event(&mut s, &parent, BASE, &parent.user, true));
        } else {
            assert!(event(&mut s, &parent, SELECTED, &parent.user, true));
        }
        ack(&mut s, &children[0]);
        assert!(!children[1].current(&s));
        family::stop(&parent);
        assert_eq!(children[1].attempt().unwrap().phase.label(), "not-sent");
    }
}
#[test]
fn queued_child_owner_read_and_body_retirement_refuse_without_claiming_dispatch() {
    for mode in 0..3 {
        let mut s = family_setup();
        let (parent, children) = start_family(&mut s);
        ack(&mut s, &children[0]);
        match mode {
            0 => s.core.account_epoch += 1,
            1 => {
                controller::begin(&mut s, false).unwrap();
            }
            _ => {
                let e = s.slack_lists.comments.entry.as_mut().unwrap();
                let mut raw = e.page.as_ref().unwrap().raw().clone();
                raw["messages"][1]["text"] = json!("Different body");
                e.page = Some(Page::parse(&e.target, raw).unwrap());
            }
        }
        assert!(!children[1].current(&s));
        family::stop(&parent);
        assert_eq!(children[1].attempt().unwrap().issued, Some(false));
        assert!(children[1].attempt().unwrap().raw.is_none());
    }
}
#[test]
fn group_pruning_retains_confirmed_siblings_of_uncertainty_and_reserves_capacity() {
    let mut s = family_setup();
    let (parent, children) = start_family(&mut s);
    ack(&mut s, &children[0]);
    settle(
        &children[1],
        Err(ListCommentReactionFailure::Unconfirmed(unavailable())),
    );
    family::stop(&parent);
    let mut actions = parent.ledger.0.lock().unwrap();
    let source = actions[0].clone();
    while actions.len() < MAX_ACTIONS {
        let mut terminal = source.clone();
        terminal.id = next_id();
        terminal.family = None;
        terminal.phase = Phase::Confirmed;
        actions.push(terminal);
    }
    family::prune(&mut actions, 3);
    assert_eq!(actions.len(), 3);
    assert_eq!(actions[0].phase.label(), "confirmed");
    assert_eq!(actions[1].phase.label(), "unknown");
    while actions.len() < MAX_ACTIONS {
        let mut unresolved = source.clone();
        unresolved.id = next_id();
        unresolved.family = None;
        unresolved.phase = Phase::Unknown;
        actions.push(unresolved);
    }
    family::prune(&mut actions, 3);
    assert_eq!(actions.len(), MAX_ACTIONS);
}
#[test]
fn issued_429_is_rejected_with_cooldown_while_lost_and_server_errors_stay_unknown() {
    let mut s = family_setup();
    let (parent, children) = start_family(&mut s);
    ack(&mut s, &children[0]);
    settle(
        &children[1],
        Err(ListCommentReactionFailure::Unconfirmed(
            Error::RateLimited {
                retry_after_secs: Some(2),
            },
        )),
    );
    children[1].update(|a| a.retry_at = now() + 2);
    family::stop(&parent);
    finish(&mut s, &children[1]);
    let a = children[1].attempt().unwrap();
    assert_eq!(a.phase.label(), "rejected");
    assert_eq!(a.failure, Some("rate-limited"));
    assert_eq!(a.issued, Some(true));
    assert!(a.raw.is_none());
    assert!(!mine(&s, &parent, BASE));
    assert!(!available(&s, generation(&s), TS, SELECTED));
    for error in [
        Error::HttpStatus {
            status: 500,
            retry_after_secs: None,
        },
        unavailable(),
    ] {
        settle(
            &children[1],
            Err(ListCommentReactionFailure::Unconfirmed(error)),
        );
        assert_eq!(children[1].attempt().unwrap().phase.label(), "unknown");
    }
}
#[test]
fn custom_art_shadow_and_unowned_tones_do_not_create_new_removal_names() {
    let mut s = family_setup();
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .custom_emoji
        .insert(
            BASE.into(),
            super_platinum_core::slack::models::Emoji {
                name: BASE.into(),
                value: "https://example.test/custom.png".into(),
                ..Default::default()
            },
        );
    let (parent, _) = start_family(&mut s);
    assert_eq!(parent.family.unwrap().names, vec![BASE, TONE, THIRD]);
    let mut s = setup();
    let mut row = raw(vec![]);
    row["reactions"] = json!([{"name":SELECTED,"count":1,"users":["U_OTHER"]}]);
    load(&mut s, row);
    // No removal plan is invented; this now admits observed pill addition.
    assert!(available(&s, generation(&s), TS, SELECTED));
    let row = message(s.slack_lists.comments.entry.as_ref().unwrap(), TS).unwrap();
    assert!(family::selection(row, "U0", SELECTED).is_err());
    assert!(family::needs_addition(row, "U0", SELECTED));
}
