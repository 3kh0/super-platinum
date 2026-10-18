use super::*;
use super_platinum_core::slack_lists::{BoardLanePlacement, ViewRefinementSavePhase};
fn shell() -> ShellState {
    let mut shell = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-board-local-order",
    );
    // These tests exercise Save-as-new with a positively observed capacity.
    let loaded = shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    let mut raw = loaded.document.raw.clone();
    raw["list_limits"] = json!({"over_view_maximum":false});
    loaded.document = Document::parse(raw).unwrap();
    rebuild_rows(&mut shell);
    shell
}
fn dirty(s: &mut ShellState) {
    let a = refinement::capture(s).unwrap();
    let d = a
        .draft(s)
        .unwrap()
        .with_completed(
            &s.slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .document,
            true,
        )
        .unwrap();
    assert!(refinement::apply(s, &a, d));
}
fn provider(s: &mut ShellState) {
    let l = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    let d = BoardLaneOrderDraft::moved(
        &l.document,
        "V_LANE",
        None,
        "OPT_DONE",
        "OPT_PROGRESS",
        BoardLanePlacement::Before,
        l.allow_lists.as_deref(),
        l.personal_task_lists.as_ref(),
    )
    .unwrap();
    s.slack_lists.view.as_mut().unwrap().layout.board_order = Some(d);
    rebuild_rows(s);
}
fn opened(s: &mut ShellState, new: bool) -> u64 {
    let a = capture(s, new).unwrap();
    let visit = generation(s).unwrap();
    let selected = selection(s).unwrap();
    let serial = open_shell(s, a, visit, selected).unwrap();
    if new {
        with_registry(|r| {
            r.entries.get_mut(&Key::current(s).unwrap()).unwrap().input =
                Input::Name("Created".into())
        });
    }
    serial
}
fn admitted(
    s: &mut ShellState,
    new: bool,
) -> (
    Guard,
    ViewRefinementSave,
    ViewRefinementSaveAttempt,
    ViewRefinementSavePermit,
) {
    let serial = opened(s, new);
    let (guard, Intent::Refinement(intent)) = prepare(s, serial).unwrap() else {
        panic!()
    };
    let l = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    let fresh = super::super::super::network::Metadata {
        document: l.document.clone(),
        allow: l.allow_lists.clone(),
        personal: l.personal_task_lists.clone().unwrap(),
        experiments: None,
    };
    let (a, p) = admit(s, &guard, &intent, &fresh).unwrap();
    (guard, intent, a, p)
}
fn ack(i: &ViewRefinementSave) -> Value {
    let mut raw = i.original().raw.clone();
    raw["list_metadata"]["views"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"V_ACK_CREATED","name":"Created","type":"board"}));
    json!({"ok":true,"list":raw})
}
#[test]
fn new_ack_selects_pending_immediately_and_child_does_not_require_metadata_observation() {
    let mut s = shell();
    provider(&mut s);
    dirty(&mut s);
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let (guard, intent, mut a, _permit) = admitted(&mut s, true);
    let response = ack(&intent);
    assert!(
        intent
            .observed_view_id(&Document::parse(response["list"].clone()).unwrap())
            .is_none()
    );
    let handoff = a.acknowledge(&response).unwrap();
    let (child, _child_permit) = preference::admit(
        &json!({"ok":true,"prefs":{"lists_default_view":"{\"F_OTHER\":\"V_OTHER\"}"}}),
        handoff,
        intent.original(),
    )
    .unwrap();
    assert_eq!(child.phase(), ViewRefinementSavePhase::InFlight);
    assert_eq!(child.intent().expected()["F_OTHER"], "V_OTHER");
    let pending = select_pending(&mut s, &guard, "V_ACK_CREATED").unwrap();
    let selected = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(selected.layout.selected.as_deref(), Some("V_ACK_CREATED"));
    assert!(selected.layout.board_order.is_some());
    assert!(!selected.layout.refinements.is_empty());
    assert!(
        !entry(&s)
            .unwrap()
            .refinement_receipt
            .unwrap()
            .dismissed_on_ack
    );
    assert!(pending.current(&s, &guard));
    assert!(!guard.current(&s, Stage::Saving));
    assert!(settle(
        &mut s,
        &guard,
        &intent,
        Ok(response),
        Some("V_ACK_CREATED"),
        Some(&pending)
    ));
    let v = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(v.layout.selected.as_deref(), Some("V_ACK_CREATED"));
    assert_eq!(
        v.layout.pending_refinement.as_deref(),
        Some("V_ACK_CREATED")
    );
    assert!(v.layout.refinements.is_empty());
    assert!(v.layout.board_order.is_none());
    assert!(v.rows.is_empty() && v.layout.lanes.is_empty());
    assert_eq!(v.loaded.as_ref().unwrap().document, before.document);
    assert_eq!(v.loaded.as_ref().unwrap().records, before.records);
    assert_eq!(before.records.records.len(), 73);
    assert!(
        entry(&s)
            .unwrap()
            .refinement_receipt
            .unwrap()
            .custody
            .unwrap()
            .current(&s)
    );
    assert!(a.acknowledge(&ack(&intent)).is_none());
}
#[test]
fn lost_reply_never_selects_created_id_or_clears_new_local_provider() {
    let mut s = shell();
    provider(&mut s);
    dirty(&mut s);
    let before = s.slack_lists.view.as_ref().unwrap().clone();
    let (guard, intent, mut a, _permit) = admitted(&mut s, true);
    assert!(a.finish(false));
    assert!(a.acknowledge(&ack(&intent)).is_none());
    assert!(settle(
        &mut s,
        &guard,
        &intent,
        Err(Failure::Unconfirmed(Error::Api("lost".into()))),
        None,
        None
    ));
    let v = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(v.layout.selected, before.layout.selected);
    assert_eq!(v.layout.refinements, before.layout.refinements);
    assert_eq!(v.layout.board_order, before.layout.board_order);
    assert!(v.layout.pending_refinement.is_none());
    assert_eq!(
        entry(&s).unwrap().refinement_receipt.unwrap().preference,
        Preference::NotRequested
    );
}
#[test]
fn retired_ack_retains_receipt_without_reset_selection_or_child_custody() {
    let mut s = shell();
    provider(&mut s);
    dirty(&mut s);
    let (guard, intent, mut a, _permit) = admitted(&mut s, true);
    let response = ack(&intent);
    let _handoff = a.acknowledge(&response).unwrap();
    s.core.account_epoch = s.core.account_epoch.wrapping_add(1);
    let before = s.slack_lists.view.as_ref().unwrap().clone();
    assert!(!settle(
        &mut s,
        &guard,
        &intent,
        Ok(response),
        Some("V_ACK_CREATED"),
        None
    ));
    let v = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(v.layout.selected, before.layout.selected);
    assert_eq!(v.layout.refinements, before.layout.refinements);
    assert_eq!(v.layout.board_order, before.layout.board_order);
    with_registry(|r| {
        let e = r.entries.get(&guard.key).unwrap();
        assert!(e.raw_reply.is_some());
        assert!(
            !e.refinement_receipt
                .as_ref()
                .unwrap()
                .custody
                .as_ref()
                .unwrap()
                .current(&s)
        );
    });
}
#[test]
fn old_save_action_cannot_open_after_local_change_and_preflight_cannot_admit_twice() {
    let mut s = shell();
    dirty(&mut s);
    let old = capture(&s, false).unwrap();
    let a = refinement::capture(&s).unwrap();
    let d = a
        .draft(&s)
        .unwrap()
        .with_visibility(
            &s.slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .document,
            "C_NUMBER",
            false,
        )
        .unwrap();
    assert!(refinement::apply(&mut s, &a, d));
    let visit = generation(&s).unwrap();
    let selected = selection(&s).unwrap();
    assert!(open_shell(&mut s, old, visit, selected).is_none());
    assert!(entry(&s).is_none());
    let (guard, intent, _attempt, _permit) = admitted(&mut s, false);
    let l = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    let fresh = super::super::super::network::Metadata {
        document: l.document.clone(),
        allow: l.allow_lists.clone(),
        personal: l.personal_task_lists.clone().unwrap(),
        experiments: None,
    };
    assert!(admit(&s, &guard, &intent, &fresh).is_none());
}
#[test]
fn existing_save_clears_provider_after_failure_but_keeps_local_maps_and_raw_page() {
    let mut s = shell();
    provider(&mut s);
    dirty(&mut s);
    let before = s.slack_lists.view.as_ref().unwrap().clone();
    let (guard, intent, _a, _permit) = admitted(&mut s, false);
    assert!(settle(
        &mut s,
        &guard,
        &intent,
        Err(Failure::Unconfirmed(Error::Api("lost".into()))),
        None,
        None
    ));
    let v = s.slack_lists.view.as_ref().unwrap();
    assert!(v.layout.board_order.is_none());
    assert_eq!(v.layout.refinements, before.layout.refinements);
    assert_eq!(
        v.loaded.as_ref().unwrap().records,
        before.loaded.as_ref().unwrap().records
    );
    assert_eq!(
        v.loaded.as_ref().unwrap().document,
        before.loaded.as_ref().unwrap().document
    );
}

#[test]
fn pending_selection_proof_cannot_complete_after_account_or_local_custody_changes() {
    for retired in 0..2 {
        let mut s = shell();
        provider(&mut s);
        dirty(&mut s);
        let (guard, intent, mut attempt, _permit) = admitted(&mut s, true);
        let reply = ack(&intent);
        let handoff = attempt.acknowledge(&reply).unwrap();
        let pending = select_pending(&mut s, &guard, handoff.view_id()).unwrap();
        if retired == 0 {
            s.core.account_epoch += 1;
        } else {
            s.slack_lists.view.as_mut().unwrap().layout.generation += 1;
        }
        assert!(!pending.current(&s, &guard));
        assert!(!settle(
            &mut s,
            &guard,
            &intent,
            Ok(reply),
            Some("V_ACK_CREATED"),
            Some(&pending)
        ));
        let view = s.slack_lists.view.as_ref().unwrap();
        assert!(view.layout.board_order.is_some());
        assert!(!view.layout.refinements.is_empty());
        assert!(with_registry(|r| !r.entries[&guard.key]
            .refinement_receipt
            .as_ref()
            .unwrap()
            .dismissed_on_ack));
    }
}
