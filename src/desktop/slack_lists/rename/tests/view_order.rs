use super::*;
use super_platinum_core::slack_lists::{ViewReorder, ViewReorderDirection as Direction};
fn seeded() -> ShellState {
    ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-view-order",
    )
}
fn begin(s: &mut ShellState) -> (Guard, Intent) {
    let serial = entry(s).unwrap().serial;
    prepare(s, serial).unwrap()
}
fn desired(s: &ShellState, intent: &Intent) -> network::Metadata {
    let Intent::Reorder(i) = intent else { panic!() };
    let mut m = metadata(s, None);
    let raw = &mut m.document.raw;
    raw["list_metadata"]["views"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["id"] == i.view_id())
        .unwrap()["position"] = json!(i.desired_position());
    m.document = Document::parse(raw.clone()).unwrap();
    m
}
#[test]
fn fixed_review_cancel_and_boundaries_never_mutate_canonical_order() {
    for name in ["slack-list-view-order", "slack-list-view-order-narrow"] {
        let mut s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
        let e = entry(&s).unwrap();
        assert!(matches!(e.target, Target::Reorder(_, Direction::Up)));
        assert_eq!(e.input, Input::Reorder);
        let p = presentation(&s).unwrap().reorder.unwrap();
        assert_eq!(
            p.before.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(),
            ["V_ORDER_FIRST", "V_ORDER_SELECTED", "V_ORDER_LAST"]
        );
        assert_eq!(
            p.after.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(),
            ["V_ORDER_SELECTED", "V_ORDER_FIRST", "V_ORDER_LAST"]
        );
        assert_eq!(metadata(&s, None).document, e.original);
        assert!(close_entry(&s, e.serial));
        assert!(entry(&s).is_none());
        let doc = metadata(&s, None).document;
        assert_eq!(
            ViewReorder::choices(&doc, "V_ORDER_FIRST"),
            [Direction::Down]
        );
        assert_eq!(ViewReorder::choices(&doc, "V_ORDER_LAST"), [Direction::Up]);
        let visit = generation(&s).unwrap();
        let selection = selection(&s).unwrap();
        assert!(!super::super::view_order::open_shell(
            &mut s,
            visit,
            selection,
            "V_ORDER_FIRST",
            Direction::Down
        ));
        assert!(!super::super::view_order::open_shell(
            &mut s,
            visit,
            selection + 1,
            "V_ORDER_SELECTED",
            Direction::Down
        ));
    }
}
#[test]
fn one_permit_ack_uncertain_and_exact_readback_preserve_selection_records_and_settings() {
    let mut s = seeded();
    let e = entry(&s).unwrap();
    let selected = selected_view(&s).unwrap().to_owned();
    let records = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .clone();
    let (g, i) = begin(&mut s);
    let expected = desired(&s, &i);
    let permit = admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    let request = permit.request(&s.core.client, &fixture::session());
    let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
        panic!()
    };
    let url = url::Url::parse(&request.url).unwrap();
    let origin = url::Url::parse(&fixture::session().url).unwrap();
    assert_eq!(request.method, "POST");
    assert_eq!(url.origin(), origin.origin());
    assert_eq!(url.path(), "/api/lists.views.update");
    assert!(url.query().is_some()); // SlackClient appends the captured REST xparams.
    assert!(url.fragment().is_none());
    assert_eq!(fields.len(), 5);
    assert!(
        fields
            .iter()
            .any(|(k, v)| k == "id" && v == "V_ORDER_SELECTED")
    );
    assert!(fields.iter().any(|(k, _)| k == "position"));
    assert!(
        fields
            .iter()
            .any(|(k, v)| k == "_x_reason" && v == "update-view")
    );
    assert!(admitted(&s, &g, &i, &metadata(&s, None)).is_none());
    completed(&mut s, &g, Ok(json!({"ok":true})));
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert_eq!(metadata(&s, None).document, e.original);
    assert!(prepare(&mut s, e.serial).is_none());
    let check = checking(&s);
    assert!(checked(&mut s, &check, Ok(expected)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    assert_eq!(selected_view(&s), Some(selected.as_str()));
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        records
    );
    let now = metadata(&s, None).document;
    let mut restored = now.raw.clone();
    restored["list_metadata"]["views"][1]["position"] =
        e.original.raw["list_metadata"]["views"][1]["position"].clone();
    assert_eq!(restored, e.original.raw);
    assert!(close_entry(&s, e.serial));
}
#[test]
fn stale_selection_record_cursor_revision_account_and_preflight_refuse_admission() {
    for change in 0..6 {
        let mut s = seeded();
        let (g, i) = begin(&mut s);
        let mut fresh = metadata(&s, None);
        match change {
            0 => {
                s.slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .layout
                    .selection_generation = next_id()
            }
            1 => {
                let loaded = s
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap();
                let mut raw = loaded.records.raw.clone();
                raw["response_metadata"]["next_cursor"] = json!("changed");
                loaded.records = RecordsPage::parse(raw).unwrap();
            }
            2 => s.slack_lists.view.as_mut().unwrap().revision = next_id(),
            3 => s.core.account_epoch += 1,
            4 => {
                fresh.document.raw["list_metadata"]["views"][0]["opaque"] = json!({"changed":true});
                fresh.document = Document::parse(fresh.document.raw).unwrap();
            }
            _ => fresh.allow = Some("read_only".into()),
        }
        assert!(admitted(&s, &g, &i, &fresh).is_none(), "change {change}");
    }
}
#[test]
fn conflict_and_retired_read_cannot_authorize_replay_or_done_after_aba() {
    let mut s = seeded();
    let (g, i) = begin(&mut s);
    assert!(admitted(&s, &g, &i, &metadata(&s, None)).is_some());
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    let expected = desired(&s, &i);
    let before = metadata(&s, None).document;
    let check = checking(&s);
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .selection_generation = next_id();
    assert!(!checked(&mut s, &check, Ok(expected)));
    assert_eq!(metadata(&s, None).document, before);
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    let check = checking(&s);
    let unchanged = metadata(&s, None);
    assert!(checked(&mut s, &check, Ok(unchanged)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    let serial = entry(&s).unwrap().serial;
    assert!(prepare(&mut s, serial).is_none());
    let check = checking(&s);
    let expected = desired(&s, &i);
    assert!(checked(&mut s, &check, Ok(expected)));
    let serial = entry(&s).unwrap().serial;
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .selection_generation = next_id();
    assert!(!close_entry(&s, serial));
    let p = presentation(&s).unwrap();
    assert!(p.checkable);
    assert!(!p.current_review);
    assert!(p.message.contains("The move request will not be repeated."));
}
