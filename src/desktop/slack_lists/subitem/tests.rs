use super::*;
use serde_json::json;
struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with(|r| {
            r.entries.remove(&self.0);
        });
    }
}
fn shell() -> (ShellState, Cleanup) {
    let mut s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    super::super::fixture::subitem::install(&mut s, "slack-list-subitem-create");
    s.core.active_account = Some(format!("subitem-test-{}", next_id()));
    s.slack_lists.scope = Scope::capture(&s);
    open_root(&mut s);
    let c = Cleanup(Key::current(&s).unwrap());
    (s, c)
}
fn open_root(s: &mut ShellState) {
    let v = s.slack_lists.view.as_ref().unwrap();
    let row = v.rows.iter().find(|r| r.id == "R_HYDRATE_PARENT").unwrap();
    let (instance, visit, layout) = (row.instance_id.clone(), v.generation, v.layout.generation);
    assert!(record_detail::open_shell(s, &instance, visit, layout));
}
fn detail(s: &ShellState) -> u64 {
    super::super::presentation(s)
        .unwrap()
        .detail
        .unwrap()
        .generation
}
fn child() -> Value {
    json!({"id":"NEW","parent_record_id":"R_HYDRATE_PARENT","date_created":99,"created_by":"U1","fields":[{"key":"NAME","rich_text":[]}],"new_unknown":true})
}
fn loaded(s: &ShellState, after: bool) -> network::Loaded {
    let l = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    let children = if after {
        vec![child()]
    } else {
        l.records
            .records
            .iter()
            .filter(|r| r.parent_id() == Some("R_HYDRATE_PARENT"))
            .map(|r| r.raw.clone())
            .collect()
    };
    let raw = json!({"ok":true,"list":l.document.raw,"record":l.records.record("R_HYDRATE_PARENT").unwrap().raw,"subtasks":children});
    network::Loaded {
        document: l.document.clone(),
        allow: l.allow_lists.clone(),
        personal: l.personal_task_lists.clone().unwrap(),
        experiments: l.list_experiments.unwrap(),
        parent: SubitemReadback::from_response(&l.document.id, "R_HYDRATE_PARENT", &raw).unwrap(),
        raw: Arc::new(raw),
    }
}
fn sending(s: &mut ShellState) -> Guard {
    let d = detail(s);
    let g = begin(s, d).unwrap();
    assert!(admit(s, &g, &loaded(s, false)).is_some());
    g
}
fn ack() -> Value {
    json!({"ok":true,"record":child(),"opaque_ack":{"retained":true}})
}
#[test]
fn positive_ack_then_exact_sparse_parent_read_preserves_tree_and_explicit_navigation() {
    let (mut s, _c) = shell();
    let before = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .clone();
    let selected = s.slack_lists.view.as_ref().unwrap().layout.selected.clone();
    let g = sending(&mut s);
    completed(&mut s, &g, Ok(ack()));
    assert_eq!(entry(&s).unwrap().stage, Stage::AwaitingRead);
    assert!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records
            .record("NEW")
            .is_none()
    );
    let check = checking(&mut s, g.serial).unwrap();
    let data = loaded(&s, true);
    assert!(checked(&mut s, &check, Ok(data)));
    let after = &s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records;
    for id in ["R_CHILD_A", "R_CHILD_B", "R_GRANDCHILD", "R_HYDRATE_OTHER"] {
        assert_eq!(after.record(id), before.record(id));
    }
    assert_eq!(after.next_cursor, before.next_cursor);
    assert_eq!(
        after.raw["response_metadata"],
        before.raw["response_metadata"]
    );
    assert_eq!(entry(&s).unwrap().stage, Stage::Available);
    assert!(open_shell(&mut s, g.serial));
    let p = super::super::presentation(&s).unwrap();
    assert_eq!(p.detail.unwrap().record_id, "NEW");
    assert_eq!(p.selected_view, selected);
    assert!(!blocks(&s));
}
#[test]
fn lost_ack_never_selects_new_blank_from_population_diff() {
    let (mut s, _c) = shell();
    let g = sending(&mut s);
    completed(
        &mut s,
        &g,
        Err(ListSubitemFailure::Unconfirmed(Error::Transport(
            "synthetic lost".into(),
        ))),
    );
    let check = checking(&mut s, g.serial).unwrap();
    let data = loaded(&s, true);
    assert!(checked(&mut s, &check, Ok(data)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Reviewed);
    assert!(entry(&s).unwrap().child.is_none());
    assert!(!open_shell(&mut s, g.serial));
    assert!(dismiss_shell(&mut s, g.serial));
    assert!(!blocks(&s));
}
#[test]
fn same_scope_replacement_selection_and_parent_changes_revoke_unsent_admission() {
    for mode in 0..4 {
        let (mut s, _c) = shell();
        let d = detail(&s);
        let g = begin(&mut s, d).unwrap();
        let fresh = loaded(&s, false);
        match mode {
            0 => s.slack_lists.scope = Scope::capture(&s),
            1 => {
                s.slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .layout
                    .selection_generation = next_id()
            }
            2 => s.core.account_epoch += 1,
            _ => {
                let v = s.slack_lists.view.as_mut().unwrap();
                v.revision = next_id();
            }
        }
        assert!(admit(&s, &g, &fresh).is_none());
        assert_eq!(entry_for(&g).stage, Stage::Preparing);
    }
}
fn entry_for(g: &Guard) -> Entry {
    with(|r| r.entries[&g.key].clone())
}
#[test]
fn retained_receipt_survives_actual_shell_replacement_and_stale_check_cannot_project() {
    let (mut s, _c) = shell();
    let g = sending(&mut s);
    let owner = s.core.active_account.clone();
    completed(&mut s, &g, Ok(ack()));
    let check = checking(&mut s, g.serial).unwrap();
    let data = loaded(&s, true);
    let (mut replacement, _other) = shell();
    replacement.core.active_account = owner;
    replacement.slack_lists.scope = Scope::capture(&replacement);
    let _previous = std::mem::replace(&mut s, replacement);
    assert!(blocks(&s));
    let before = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .clone();
    assert!(!checked(&mut s, &check, Ok(data)));
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        before
    );
    assert_eq!(entry(&s).unwrap().stage, Stage::AwaitingRead);
    assert!(entry(&s).unwrap().reply.is_some());
}
#[test]
fn late_known_limit_blocks_all_creation_routes_after_owner_return() {
    let (mut s, _c) = shell();
    let g = sending(&mut s);
    s.core.account_epoch += 1;
    completed(
        &mut s,
        &g,
        Err(ListSubitemFailure::Unconfirmed(Error::HttpStatus {
            status: 503,
            retry_after_secs: Some(8),
        })),
    );
    assert!(rename::creation_cooldown(&g.scope));
    s.slack_lists.scope = Scope::capture(&s);
    assert!(blocked(&s));
    assert!(entry(&s).unwrap().failure.is_some());
    assert!(!create::presentation(&s).enabled);
}
#[test]
fn exact_review_invalidated_by_later_current_selection_cannot_open_or_retire() {
    let (mut s, _c) = shell();
    let g = sending(&mut s);
    completed(&mut s, &g, Ok(ack()));
    let check = checking(&mut s, g.serial).unwrap();
    let data = loaded(&s, true);
    assert!(checked(&mut s, &check, Ok(data)));
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .selection_generation = next_id();
    assert!(!open_shell(&mut s, g.serial));
    assert!(!dismiss_shell(&mut s, g.serial));
    assert!(blocks(&s));
}
#[test]
fn complete_payload_reservation_refuses_without_eviction_or_serial_change() {
    assert_eq!(registry::charge(0), Some(24 * 1024 * 1024));
    assert_eq!(
        registry::charge(super_platinum_core::slack_lists::SUBITEM_INPUT_LIMIT),
        Some(32 * 1024 * 1024)
    );
    assert_eq!(registry::charge(usize::MAX), None);
    let (mut s, _c) = shell();
    let d = detail(&s);
    let g = begin(&mut s, d).unwrap();
    let template = entry_for(&g);
    let mut r = registry::Registry::default();
    let mut e = template.clone();
    e.charge = registry::TOTAL_BYTES;
    r.entries.insert(g.key.clone(), e);
    let other = Key {
        owner: (Some("OTHER".into()), "T".into(), "U".into()),
        list: g.key.list.clone(),
    };
    assert!(!r.room(&other, 1));
    assert_eq!(r.entries[&g.key].serial, g.serial);
    assert_eq!(r.entries.len(), 1);
    let before = entry_for(&g).serial;
    let v = s.slack_lists.view.as_mut().unwrap();
    v.loaded.as_mut().unwrap().records.raw["oversize"] =
        json!("x".repeat(super_platinum_core::slack_lists::SUBITEM_INPUT_LIMIT));
    assert!(begin(&mut s, d).is_err());
    assert_eq!(entry_for(&g).serial, before);
}
#[test]
fn header_duplicate_cell_and_schema_admission_share_subitem_lock() {
    let (mut s, _c) = shell();
    let d = detail(&s);
    let g = begin(&mut s, d).unwrap();
    assert!(create::blocks_mutation(&s));
    assert!(!create::presentation(&s).enabled);
    assert!(!rename::view_create::can_open(&s));
    let current = detail(&s);
    assert!(record_detail::begin_editor_shell(&mut s, current, "C_NAME").is_err());
    assert!(dismiss_shell(&mut s, g.serial));
    assert!(blocks(&s));
    unsent(&mut s, &g, "Canceled");
    assert!(dismiss_shell(&mut s, g.serial));
    assert!(!blocks(&s));
}

#[test]
fn unavailable_parent_can_be_explicitly_reviewed_without_replay_or_identity_claim() {
    for mode in 0..3 {
        let (mut s, _c) = shell();
        let g = sending(&mut s);
        completed(
            &mut s,
            &g,
            Err(ListSubitemFailure::Unconfirmed(Error::Transport(
                "lost".into(),
            ))),
        );
        let v = s.slack_lists.view.as_mut().unwrap();
        let l = v.loaded.as_mut().unwrap();
        let mut raw = l.records.raw.clone();
        let rows = raw["records"].as_array_mut().unwrap();
        if mode == 0 {
            rows.retain(|r| r["id"] != "R_HYDRATE_PARENT");
        } else {
            let p = rows
                .iter_mut()
                .find(|r| r["id"] == "R_HYDRATE_PARENT")
                .unwrap();
            if mode == 1 {
                p["archived"] = json!(true);
            } else {
                p["parent_record_id"] = json!("R_HYDRATE_OTHER");
            }
        }
        l.records = RecordsPage::parse(raw).unwrap();
        v.revision = next_id();
        rebuild_rows(&mut s);
        assert!(!dismiss_shell(&mut s, g.serial));
        assert!(review_retained_shell(&mut s, g.serial));
        assert!(!open_shell(&mut s, g.serial));
        assert!(entry(&s).unwrap().child.is_none());
        s.slack_lists.view.as_mut().unwrap().revision = next_id();
        assert!(!dismiss_shell(&mut s, g.serial));
        assert!(review_retained_shell(&mut s, g.serial));
        assert!(dismiss_shell(&mut s, g.serial));
        assert!(!blocks(&s));
    }
}
#[test]
fn positive_read_adopts_title_unknown_metadata_and_all_experiment_flags() {
    let (mut s, _c) = shell();
    let g = sending(&mut s);
    completed(&mut s, &g, Ok(ack()));
    let check = checking(&mut s, g.serial).unwrap();
    let mut data = loaded(&s, true);
    let mut raw = (*data.raw).clone();
    raw["list"]["title"] = json!("Fresh List title");
    raw["list"]["new_opaque"] = json!({"nested":7});
    data.document = Document::parse(raw["list"].clone()).unwrap();
    data.parent =
        SubitemReadback::from_response(&data.document.id, g.proof.parent.as_str(), &raw).unwrap();
    data.raw = Arc::new(raw);
    data.experiments.todos = true;
    data.experiments.grid_todos = true;
    assert!(checked(&mut s, &check, Ok(data)));
    let v = s.slack_lists.view.as_ref().unwrap();
    let l = v.loaded.as_ref().unwrap();
    assert_eq!(v.title, "Fresh List title");
    assert_eq!(l.document.raw["new_opaque"], json!({"nested":7}));
    assert!(l.list_experiments.unwrap().todos && l.list_experiments.unwrap().grid_todos);
}

#[test]
fn oversized_new_input_refuses_before_registry_or_serial_admission() {
    let (mut s, _c) = shell();
    let v = s.slack_lists.view.as_mut().unwrap();
    let l = v.loaded.as_mut().unwrap();
    let mut raw = l.records.raw.clone();
    raw["large_unknown"] = json!("x".repeat(super_platinum_core::slack_lists::SUBITEM_INPUT_LIMIT));
    l.records = RecordsPage::parse(raw).unwrap();
    v.revision = next_id();
    rebuild_rows(&mut s);
    let d = detail(&s);
    let before = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .raw
        .clone();
    assert!(begin(&mut s, d).is_err());
    assert!(entry(&s).is_none());
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records
            .raw,
        before
    );
}

#[test]
fn cancel_does_not_free_capacity_before_detached_read_settles() {
    let (mut s, _c) = shell();
    let d = detail(&s);
    let g = begin(&mut s, d).unwrap();
    let charge = entry_for(&g).charge;
    assert!(dismiss_shell(&mut s, g.serial));
    assert_eq!(entry_for(&g).stage, Stage::NotRequested);
    assert!(entry_for(&g).in_flight);
    assert!(!dismiss_shell(&mut s, g.serial));
    assert!(begin(&mut s, d).is_err());
    assert_eq!(entry_for(&g).charge, charge);
    assert!(admit(&s, &g, &loaded(&s, false)).is_none());
    unsent(&mut s, &g, "Canceled");
    assert!(!entry_for(&g).in_flight);
    assert!(dismiss_shell(&mut s, g.serial));
    assert!(!blocks(&s));
}

#[test]
fn cookie_token_enterprise_transport_and_newer_reads_revoke_original_admission() {
    use super_platinum_core::config::{Session, WorkspaceSession};
    for mode in 0..7 {
        let (mut s, _c) = shell();
        let team = s.core.active_team.clone().unwrap();
        s.core.session = Some(Session {
            d_cookie: "synthetic-cookie".into(),
            workspaces: BTreeMap::from([(
                team.clone(),
                WorkspaceSession {
                    team_id: team.clone(),
                    user_id: s.self_account.user_id.clone(),
                    enterprise_id: None,
                    name: "Fixture".into(),
                    url: "https://fixture.slack.com".into(),
                    token: "synthetic-token".into(),
                },
            )]),
        });
        s.slack_lists.scope = Scope::capture(&s);
        let d = detail(&s);
        let g = begin(&mut s, d).unwrap();
        let fresh = loaded(&s, false);
        match mode {
            0 => s.core.session.as_mut().unwrap().d_cookie = "rotated".into(),
            1 => {
                s.core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .token = "rotated".into()
            }
            2 => {
                s.core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .enterprise_id = Some("E_OTHER".into())
            }
            3 => {
                s.core.transport = Some(Arc::new(
                    super_platinum_core::slack::Transport::new("synthetic-cookie").unwrap(),
                ))
            }
            4 => s.slack_lists.view.as_mut().unwrap().read = next_id(),
            5 => s.overlay = None,
            _ => {
                s.slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap()
                    .document
                    .raw["last_read"] = json!(99)
            }
        }
        assert!(admit(&s, &g, &fresh).is_none());
        assert_eq!(entry_for(&g).stage, Stage::Preparing);
        assert!(entry_for(&g).reply.is_none());
    }
}

#[test]
fn repeated_old_completion_cannot_replace_newer_retained_receipt() {
    let (mut s, _c) = shell();
    let old = sending(&mut s);
    completed(
        &mut s,
        &old,
        Err(ListSubitemFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    assert!(review_retained_shell(&mut s, old.serial));
    assert!(dismiss_shell(&mut s, old.serial));
    let fresh = sending(&mut s);
    assert_ne!(fresh.serial, old.serial);
    completed(&mut s, &old, Ok(ack()));
    let retained = entry_for(&fresh);
    assert_eq!(retained.stage, Stage::Sending);
    assert!(retained.reply.is_none());
    assert!(retained.in_flight);
    completed(&mut s, &fresh, Ok(ack()));
    assert_eq!(entry_for(&fresh).stage, Stage::AwaitingRead);
}

#[test]
fn explicit_retirement_does_not_reset_known_creation_limit() {
    for error in [
        Error::RateLimited {
            retry_after_secs: Some(60),
        },
        Error::HttpStatus {
            status: 503,
            retry_after_secs: Some(60),
        },
    ] {
        let (mut s, _c) = shell();
        let g = sending(&mut s);
        s.core.account_epoch += 1;
        completed(&mut s, &g, Err(ListSubitemFailure::Unconfirmed(error)));
        s.slack_lists.scope = Scope::capture(&s);
        assert!(review_retained_shell(&mut s, g.serial));
        assert!(dismiss_shell(&mut s, g.serial));
        assert!(!blocks(&s));
        assert!(blocked(&s));
        assert!(rename::creation_cooldown(
            s.slack_lists.scope.as_ref().unwrap()
        ));
        let d = detail(&s);
        assert!(begin(&mut s, d).is_err());
    }
}
