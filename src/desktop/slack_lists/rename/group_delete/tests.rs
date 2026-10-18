use super::super::network as shared_network;
use super::*;
use super_platinum_core::slack::{
    client::RequestBody, transport::ListColumnRenameFailure as Failure,
};
fn seed() -> ShellState {
    ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-board-group-delete",
    )
}
fn loaded(s: &ShellState) -> &Loaded {
    s.slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
}
fn metadata(s: &ShellState) -> shared_network::Metadata {
    let l = loaded(s);
    shared_network::Metadata {
        document: l.document.clone(),
        allow: l.allow_lists.clone(),
        personal: l.personal_task_lists.clone().unwrap(),
        experiments: l.list_experiments,
    }
}
fn preparing(s: &mut ShellState) -> (Guard, BoardGroupDelete) {
    let a = available(s, "OPT_PROGRESS").unwrap();
    assert!(open_shell(s, a));
    let serial = entry(s).unwrap().serial;
    let (g, i) = prepare(s, serial).unwrap();
    let Intent::GroupDelete(i) = i else { panic!() };
    (g, i)
}
fn admit(s: &mut ShellState) -> (Guard, BoardGroupDelete) {
    let (g, i) = preparing(s);
    assert!(network::admit(s, &g, &i, &metadata(s), &loaded(s).records).is_some());
    (g, i)
}
fn checking(s: &ShellState) -> Guard {
    let key = Key::current(s).unwrap();
    let e = entry(s).unwrap();
    let g = Guard::capture_read(s, key.clone(), e.serial).unwrap();
    with_registry(|r| {
        let e = r.entries.get_mut(&key).unwrap();
        e.stage = Stage::Checking;
        e.job = Some(g.job);
    });
    g
}
fn desired(i: &BoardGroupDelete) -> (Document, RecordsPage) {
    let mut d = i.original().raw.clone();
    let c = d["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["id"] == i.column_id())
        .unwrap();
    c["options"] = i.options().clone();
    let mut p = i.original_records().raw.clone();
    for cell in i.cells() {
        let row = p["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["id"] == cell["row_id"])
            .unwrap();
        let f = row["fields"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|f| f["key"] == i.column_key())
            .unwrap();
        f["select"] = json!([]);
    }
    (Document::parse(d).unwrap(), RecordsPage::parse(p).unwrap())
}
fn finish(s: &mut ShellState, g: &Guard) {
    network::completed(s, g, false, Ok(json!({"ok":true})));
    network::completed(s, g, true, Ok(json!({"ok":true})));
}
#[test]
fn aliases_preserve_population_and_cancel_mints_no_intent() {
    for name in [
        "slack-list-board-group-delete",
        "slack-list-board-group-delete-narrow",
    ] {
        let mut s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
        let before = loaded(&s).clone();
        assert_eq!(before.records.records.len(), 73);
        assert_eq!(
            before.records.raw["response_metadata"]["next_cursor"],
            "precision-opaque+/="
        );
        let a = available(&s, "OPT_PROGRESS").unwrap();
        assert!(a.matching > 0);
        assert!(open_shell(&mut s, a));
        let e = entry(&s).unwrap();
        assert!(e.intent.is_none());
        assert!(e.group_delete_outcomes.is_none());
        assert!(close_entry(&s, e.serial));
        assert!(entry(&s).is_none());
        assert_eq!(loaded(&s).document, before.document);
        assert_eq!(loaded(&s).records, before.records);
    }
}
#[test]
fn stale_action_and_confirmation_refuse_raw_scope_and_projection_drift() {
    for case in 0..5 {
        let mut s = seed();
        let a = available(&s, "OPT_PROGRESS").unwrap();
        match case {
            0 => s.slack_lists.scope = Scope::capture(&s),
            1 => s.core.account_epoch += 1,
            2 => s.slack_lists.view.as_mut().unwrap().layout.find.revision = next_id(),
            3 => {
                let l = s
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap();
                let mut p = l.records.raw.clone();
                p["records"][0]["opaque"] = json!(7);
                l.records = RecordsPage::parse(p).unwrap();
            }
            _ => {
                s.slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .layout
                    .selection_generation = next_id()
            }
        }
        assert!(!open_shell(&mut s, a));
        assert!(entry(&s).is_none());
    }
    let mut s = seed();
    let a = available(&s, "OPT_PROGRESS").unwrap();
    assert!(open_shell(&mut s, a));
    let serial = entry(&s).unwrap().serial;
    s.slack_lists.scope = Scope::capture(&s);
    assert!(prepare(&mut s, serial).is_none());
    assert!(entry(&s).unwrap().intent.is_none());
}
#[test]
fn fresh_full_page_and_policy_precede_exact_independent_one_use_forms() {
    let mut s = seed();
    let (g, i) = preparing(&mut s);
    let page = loaded(&s).records.clone();
    let mut m = metadata(&s);
    m.allow = Some("disabled".into());
    assert!(network::admit(&s, &g, &i, &m, &page).is_none());
    m = metadata(&s);
    let mut raw = page.raw.clone();
    raw["opaque"] = json!(true);
    let changed = RecordsPage::parse(raw).unwrap();
    assert!(network::admit(&s, &g, &i, &m, &changed).is_none());
    let (a, b) = network::admit(&s, &g, &i, &m, &page).unwrap();
    assert!(network::admit(&s, &g, &i, &m, &page).is_none());
    for (permit, reason, len) in [
        (a, "update-column", 6),
        (b.unwrap(), "bulk-update-cells", 4),
    ] {
        let req = permit.request(&s.core.client, &fixture::session());
        let RequestBody::Form(fields) = req.body else {
            panic!()
        };
        assert_eq!(fields.len(), len);
        assert_eq!(fields.last().unwrap(), &("_x_reason".into(), reason.into()));
    }
}
#[test]
fn effects_finish_independently_and_not_requested_is_never_observed() {
    let mut s = seed();
    let (g, i) = admit(&mut s);
    network::completed(&mut s, &g, false, Ok(json!({"ok":true})));
    assert_eq!(entry(&s).unwrap().stage, Stage::Saving);
    network::completed(
        &mut s,
        &g,
        true,
        Err(Failure::NotRequested(Error::TransportNotConfigured)),
    );
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    let (d, p) = desired(&i);
    let read = checking(&s);
    let mut m = metadata(&s);
    m.document = d;
    assert!(checked_with_records(&mut s, &read, Ok(m), Some(p)));
    let e = entry(&s).unwrap();
    assert_eq!(e.stage, Stage::Conflict);
    let o = e.group_delete_outcomes.unwrap();
    assert_eq!(o.schema, BoardGroupDeletePhase::NotRequested);
    assert_eq!(o.cells, BoardGroupDeletePhase::Observed);
}
#[test]
fn partial_then_exact_observation_preserves_all_raw_and_current_review() {
    let mut s = seed();
    let (g, i) = admit(&mut s);
    finish(&mut s, &g);
    let (d, p) = desired(&i);
    let mut m = metadata(&s);
    m.document = d.clone();
    let read = checking(&s);
    assert!(checked_with_records(
        &mut s,
        &read,
        Ok(m),
        Some(i.original_records().clone())
    ));
    let o = entry(&s).unwrap().group_delete_outcomes.unwrap();
    assert_eq!(o.schema, BoardGroupDeletePhase::Observed);
    assert_eq!(o.cells, BoardGroupDeletePhase::Unconfirmed);
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    let read = checking(&s);
    let m = metadata(&s);
    assert!(checked_with_records(&mut s, &read, Ok(m), Some(p.clone())));
    let e = entry(&s).unwrap();
    assert_eq!(e.stage, Stage::Observed);
    assert!(e.review.as_ref().unwrap().navigation_current(&s));
    assert_eq!(loaded(&s).document.raw, d.raw);
    assert_eq!(loaded(&s).records.raw, p.raw);
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap()
        .records = i.original_records().clone();
    assert!(!close_entry(&s, e.serial));
    assert!(checkable(&s, &entry(&s).unwrap()));
}
#[test]
fn normalized_and_retired_readbacks_do_not_claim_exact_application() {
    let mut s = seed();
    let (g, i) = admit(&mut s);
    finish(&mut s, &g);
    let (d, p) = desired(&i);
    let mut raw = p.raw;
    raw["records"][0]["date_updated"] = json!(123);
    let p = RecordsPage::parse(raw).unwrap();
    let mut m = metadata(&s);
    m.document = d;
    let read = checking(&s);
    assert!(checked_with_records(&mut s, &read, Ok(m), Some(p)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    let before = loaded(&s).clone();
    let read = checking(&s);
    let m = metadata(&s);
    s.slack_lists.view.as_mut().unwrap().generation = next_id();
    assert!(!checked_with_records(
        &mut s,
        &read,
        Ok(m),
        Some(before.records.clone())
    ));
    assert_eq!(loaded(&s).document, before.document);
    assert_eq!(loaded(&s).records, before.records);
}
#[test]
fn zero_matches_has_schema_only_and_no_empty_cells_effect() {
    let mut s = seed();
    let l = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    let mut raw = l.records.raw.clone();
    for row in raw["records"].as_array_mut().unwrap() {
        for field in row["fields"].as_array_mut().unwrap() {
            if field["key"] == "status" {
                field["select"] = json!([]);
            }
        }
    }
    l.records = RecordsPage::parse(raw).unwrap();
    rebuild_rows(&mut s);
    let (g, i) = preparing(&mut s);
    assert!(i.cells().is_empty());
    let (_, cells) = network::admit(&s, &g, &i, &metadata(&s), &loaded(&s).records).unwrap();
    assert!(cells.is_none());
    network::completed(&mut s, &g, true, Ok(json!({"ok":true})));
    let e = entry(&s).unwrap();
    assert_eq!(e.stage, Stage::Uncertain);
    assert_eq!(
        e.group_delete_outcomes.unwrap().cells,
        BoardGroupDeletePhase::NotRequired
    );
}
#[test]
fn malformed_label_stale_open_refuses_without_registry_or_panic() {
    let mut s = seed();
    let a = available(&s, "OPT_PROGRESS").unwrap();
    let l = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    let mut raw = l.document.raw.clone();
    let c = raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["key"] == "status")
        .unwrap();
    let opt = c["options"]["choices"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["value"] == "OPT_PROGRESS")
        .unwrap();
    opt.as_object_mut().unwrap().remove("label");
    l.document = Document::parse(raw).unwrap();
    assert!(!open_shell(&mut s, a));
    assert!(entry(&s).is_none());
}

#[test]
fn stale_display_membership_and_context_retire_both_sends() {
    for saving in [false, true] {
        let mut s = seed();
        let (g, i) = preparing(&mut s);
        let stage = if saving {
            assert!(network::admit(&s, &g, &i, &metadata(&s), &loaded(&s).records).is_some());
            Stage::Saving
        } else {
            Stage::Preparing
        };
        assert!(g.current(&s, stage));
        // Simulate stale renderer rows without changing the raw page or stamps.
        remove_displayed_row(&mut s);
        assert!(!g.current(&s, stage));
    }
    let mut s = seed();
    remove_displayed_row(&mut s);
    assert!(available(&s, "OPT_PROGRESS").is_none());
    assert!(entry(&s).is_none());
}

#[test]
fn failed_followup_read_revokes_current_observation_labels() {
    let mut s = seed();
    let (g, i) = admit(&mut s);
    finish(&mut s, &g);
    let (d, p) = desired(&i);
    let mut m = metadata(&s);
    m.document = d;
    let read = checking(&s);
    assert!(checked_with_records(&mut s, &read, Ok(m), Some(p)));
    let e = entry(&s).unwrap();
    assert_eq!(e.stage, Stage::Observed);
    s.slack_lists.view.as_mut().unwrap().layout.find.revision = next_id();
    assert_eq!(
        presentation(&s, &e).unwrap().schema,
        Some("Previously observed; check current data")
    );
    let read = checking(&s);
    assert!(!checked_with_records(
        &mut s,
        &read,
        Err(Error::TransportNotConfigured),
        None
    ));
    let e = entry(&s).unwrap();
    let o = e.group_delete_outcomes.unwrap();
    assert_eq!(e.stage, Stage::Uncertain);
    assert_eq!(o.schema, BoardGroupDeletePhase::Unconfirmed);
    assert_eq!(o.cells, BoardGroupDeletePhase::Unconfirmed);
}

fn remove_displayed_row(s: &mut ShellState) {
    let view = s.slack_lists.view.as_mut().unwrap();
    let lanes = std::sync::Arc::make_mut(&mut view.layout.lanes);
    let lane = lanes.iter_mut().find(|l| l.key == "OPT_PROGRESS").unwrap();
    std::sync::Arc::make_mut(&mut lane.rows).pop();
}
