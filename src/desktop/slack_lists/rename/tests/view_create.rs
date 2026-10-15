use super::super::view_create;
use super::*;
struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with_registry(|r| {
            r.entries.remove(&self.0);
        });
    }
}
fn create_shell() -> (ShellState, Cleanup) {
    let mut s = shell();
    let v = s.slack_lists.view.as_mut().unwrap();
    let l = v.loaded.as_mut().unwrap();
    let mut raw = l.document.raw.clone();
    raw["list_limits"] = json!({"over_view_maximum":false});
    raw["list_metadata"]["views"] = json!([{"id":"V_OLD","name":"Original","type":"table","columns":[{"id":"name","key":"name","visible":true}],"grouping":{},"opaque":1}]);
    raw["list_metadata"]["default_view"] = json!("V_OLD");
    l.document = Document::parse(raw).unwrap();
    v.layout = Default::default();
    rebuild_rows(&mut s);
    let cleanup = Cleanup(Key::current(&s).unwrap());
    (s, cleanup)
}
fn begin_create(s: &mut ShellState) -> (Guard, Intent) {
    let visit = generation(s).unwrap();
    let selection = super::super::selection(s).unwrap();
    assert!(view_create::open_shell(s, visit, selection));
    let key = Key::current(s).unwrap();
    with_registry(|r| r.entries.get_mut(&key).unwrap().name = "Fresh view".into());
    prepare(s, entry(s).unwrap().serial).unwrap()
}
fn added(s: &ShellState) -> network::Metadata {
    let mut m = metadata(s, None);
    m.document.raw["list_metadata"]["views"].as_array_mut().unwrap().push(json!({"id":"SERVER_NEW","name":"Fresh view","type":"table","columns":[{"id":"name","key":"name","visible":true}],"filters":[],"sorts":[],"opaque":{"preserved":true}}));
    m.document = Document::parse(m.document.raw).unwrap();
    m
}
fn available(s: &mut ShellState) -> (u64, Value) {
    let (g, i) = begin_create(s);
    assert!(
        admitted(s, &g, &i, &metadata(s, None))
            .unwrap()
            .is_creation()
    );
    let raw = json!({"ok":true,"list":{"id":"F_OTHER","views":[{"id":"DO_NOT_SELECT"}]}});
    completed(s, &g, Ok(raw.clone()));
    assert_eq!(entry(s).unwrap().stage, Stage::Uncertain);
    assert_eq!(selected_view(s), Some("V_OLD"));
    assert_eq!(entry(s).unwrap().raw_reply, Some(raw.clone()));
    let g = checking(s);
    let fresh = added(s);
    assert!(checked(s, &g, Ok(fresh)));
    assert_eq!(entry(s).unwrap().stage, Stage::Available);
    (g.serial, raw)
}
#[test]
fn creation_and_both_renames_share_exact_registry_slot() {
    for target in [Target::Column("name".into()), Target::View("V_OLD".into())] {
        for create_first in [true, false] {
            let (mut s, _c) = create_shell();
            let visit = generation(&s).unwrap();
            let sel = super::super::selection(&s).unwrap();
            if create_first {
                assert!(view_create::open_shell(&mut s, visit, sel));
                assert!(!open_target(&mut s, target.clone(), visit));
            } else {
                assert!(open_target(&mut s, target.clone(), visit));
                assert!(!view_create::open_shell(&mut s, visit, sel));
            }
        }
    }
}
#[test]
fn limits_crossing_and_selection_aba_never_dispatch() {
    let (mut s, _c) = create_shell();
    let (g, i) = begin_create(&mut s);
    let mut changed = metadata(&s, None);
    changed.document.raw["list_limits"]["over_view_maximum"] = json!(true);
    changed.document = Document::parse(changed.document.raw).unwrap();
    assert!(admitted(&s, &g, &i, &changed).is_none());
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .selection_generation = next_id();
    assert!(admitted(&s, &g, &i, &metadata(&s, None)).is_none());
    assert_eq!(entry(&s).unwrap().stage, Stage::Preparing);
}
#[test]
fn positive_reply_is_only_provenance_fresh_open_preserves_partial_population() {
    let (mut s, _c) = create_shell();
    let population = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .clone();
    let (serial, raw) = available(&mut s);
    assert_eq!(selected_view(&s), Some("V_OLD"));
    assert_eq!(entry(&s).unwrap().raw_reply, Some(raw));
    let proof = view_create::open_available_shell(&mut s, serial).unwrap();
    assert_eq!(proof.id, "SERVER_NEW");
    assert_eq!(selected_view(&s), Some("SERVER_NEW"));
    assert!(entry(&s).is_none());
    assert!(view_create::open_available_shell(&mut s, serial).is_none());
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        population
    );
}
#[test]
fn navigation_refuses_replaced_scope_selection_document_and_visit() {
    for change in [
        "scope",
        "selection",
        "document",
        "visit",
        "epoch",
        "cookie",
        "token",
        "enterprise",
        "transport",
    ] {
        let (mut s, _c) = create_shell();
        let (serial, _) = available(&mut s);
        match change {
            "scope" => s.slack_lists.scope = Scope::capture(&s),
            "selection" => {
                s.slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .layout
                    .selection_generation = next_id()
            }
            "visit" => s.slack_lists.view.as_mut().unwrap().generation = next_id(),
            "epoch" => s.core.account_epoch += 1,
            "cookie" => s.core.session.as_mut().unwrap().d_cookie.push('x'),
            "token" => {
                let team = s.core.active_team.clone().unwrap();
                s.core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .token
                    .push('x');
            }
            "enterprise" => {
                let team = s.core.active_team.clone().unwrap();
                s.core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .enterprise_id = Some("E_CHANGED".into());
            }
            "transport" => {
                s.core.transport = Some(std::sync::Arc::new(
                    super_platinum_core::slack::Transport::new("synthetic-other").unwrap(),
                ))
            }
            _ => {
                let v = s.slack_lists.view.as_mut().unwrap();
                v.loaded.as_mut().unwrap().document.raw["updated"] = json!(99);
                v.loaded.as_mut().unwrap().document =
                    Document::parse(v.loaded.as_ref().unwrap().document.raw.clone()).unwrap();
            }
        }
        assert!(
            view_create::open_available_shell(&mut s, serial).is_none(),
            "{change}"
        );
        assert_eq!(selected_view(&s), Some("V_OLD"));
    }
}
#[test]
fn ambiguous_or_unsupported_added_views_are_review_not_available_or_unsent() {
    for change in ["none", "sibling", "defaults", "unsupported"] {
        let (mut s, _c) = create_shell();
        let (g, i) = begin_create(&mut s);
        admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
        completed(&mut s, &g, Ok(json!({"ok":true})));
        let check = checking(&s);
        let mut fresh = added(&s);
        match change {
            "none" => fresh = metadata(&s, None),
            "sibling" => {
                fresh.document.raw["list_metadata"]["views"][0]["name"] = json!("Concurrent rename")
            }
            "defaults" => fresh.document.raw["list_metadata"]["default_view"] = json!("SERVER_NEW"),
            _ => {
                fresh.document.raw["list_metadata"]["views"][1]["filter_set"] =
                    json!({"unsupported":true})
            }
        };
        fresh.document = Document::parse(fresh.document.raw).unwrap();
        assert!(checked(&mut s, &check, Ok(fresh)));
        assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
        assert!(entry(&s).unwrap().available.is_none());
        assert!(prepare(&mut s, g.serial).is_none());
        assert_eq!(selected_view(&s), Some("V_OLD"));
    }
}
#[test]
fn current_return_uses_same_owner_cooldown_before_projection() {
    let (mut s, _c) = create_shell();
    let (g, i) = begin_create(&mut s);
    admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    s.core.account_epoch += 1;
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::HttpStatus {
            status: 503,
            retry_after_secs: Some(9),
        })),
    );
    s.slack_lists.scope = Scope::capture(&s);
    assert!(registry::blocked(&g.key.owner));
    assert!(!presentation(&s).unwrap().can_check);
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
}
#[test]
fn unavailable_read_does_not_relabel_original_dispatch_as_unsent() {
    let (mut s, _c) = create_shell();
    let (g, i) = begin_create(&mut s);
    admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    completed(&mut s, &g, Ok(json!({"ok":false,"error":"denied"})));
    let check = checking(&s);
    s.slack_lists.view.as_mut().unwrap().revision = next_id();
    let fresh = added(&s);
    assert!(!checked(&mut s, &check, Ok(fresh)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert!(entry(&s).unwrap().available.is_none());
    assert_eq!(selected_view(&s), Some("V_OLD"));
}
