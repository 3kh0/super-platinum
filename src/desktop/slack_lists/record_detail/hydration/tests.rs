use super::*;
use serde_json::json;

fn shell(incomplete: bool) -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    // Production cooldown storage outlives ShellState. Synthetic owners isolate
    // parallel tests without weakening that production lifetime.
    shell.core.active_account = Some(format!("hydration-test-{}", next_id()));
    shell.slack_lists.scope = Scope::capture(&shell);
    if incomplete {
        let view = shell.slack_lists.view.as_mut().unwrap();
        let loaded = view.loaded.as_mut().unwrap();
        let mut raw = loaded.records.raw.clone();
        raw["response_metadata"] = json!({"next_cursor":"opaque"});
        loaded.records = RecordsPage::parse(raw).unwrap();
    }
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (instance, visit, layout) = (
        view.rows[0].instance_id.clone(),
        view.generation,
        view.layout.generation,
    );
    assert!(super::super::open_shell(
        &mut shell, &instance, visit, layout
    ));
    shell
}
fn open(shell: &ShellState) -> u64 {
    shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .detail
        .as_ref()
        .unwrap()
        .open_generation
}
fn loaded(shell: &ShellState) -> Loaded {
    shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap()
}
fn response(attempt: &Attempt) -> ListRecordHydration {
    ListRecordHydration::from_response(&attempt.list,&attempt.parent,&json!({
        "ok":true,"list":attempt.document.raw,"record":attempt.page.record(&attempt.parent).unwrap().raw,
        "subtasks":[{"id":"R_HYDRATED","list_id":attempt.list,"parent_record_id":attempt.parent,
            "fields":[],"date_created":99,"created_by":"U_FIXTURE","position":"a2","view_positions":{}}]
    })).unwrap()
}

#[test]
fn only_incomplete_known_parent_claims_once_per_open() {
    let mut complete = shell(false);
    let id = open(&complete);
    assert!(claim(&mut complete, id).is_none());
    let mut shell = shell(true);
    let id = open(&shell);
    let attempt = claim(&mut shell, id).unwrap();
    assert!(attempt.current(&shell));
    assert!(claim(&mut shell, id).is_none());
    assert!(finish(&mut shell, &attempt, Ok(response(&attempt))));
    assert!(claim(&mut shell, id).is_none());
    assert!(!finish(&mut shell, &attempt, Ok(response(&attempt))));
}

#[test]
fn positive_merge_retains_global_incompleteness_and_invalidates_old_detail_actions() {
    let mut shell = shell(true);
    let id = open(&shell);
    let before = loaded(&shell);
    let attempt = claim(&mut shell, id).unwrap();
    assert!(finish(&mut shell, &attempt, Ok(response(&attempt))));
    let after = loaded(&shell);
    assert_eq!(
        after.records.records.len(),
        before.records.records.len() + 1
    );
    assert_eq!(after.records.next_cursor, before.records.next_cursor);
    for row in before.records.records {
        assert_eq!(after.records.record(&row.id), Some(&row));
    }
    let detail = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .detail
        .as_ref()
        .unwrap();
    assert_eq!(detail.open_generation, id);
    assert_ne!(detail.generation, attempt.detail);
    assert!(
        super::super::compute_duplicate_reason(
            shell.slack_lists.view.as_ref().unwrap(),
            &attempt.parent
        )
        .is_some()
    );
    assert!(shell.slack_lists.operations.is_empty());
}

#[test]
fn changed_owner_visit_read_revision_selection_or_overlay_cannot_apply() {
    for change in [
        "account",
        "workspace",
        "viewer",
        "visit",
        "read",
        "revision",
        "selection",
        "overlay",
        "deleted",
        "stale",
        "loading",
    ] {
        let mut shell = shell(true);
        let id = open(&shell);
        let attempt = claim(&mut shell, id).unwrap();
        match change {
            "account" => shell.core.account_epoch += 1,
            "workspace" => shell.core.active_team = Some("OTHER".into()),
            "viewer" => shell.self_account.user_id = "OTHER".into(),
            "overlay" => shell.overlay = None,
            "deleted" => {
                shell
                    .core
                    .workspaces
                    .get_mut(&attempt.scope.team)
                    .unwrap()
                    .deleted_files
                    .insert(attempt.list.clone());
            }
            _ => {
                let v = shell.slack_lists.view.as_mut().unwrap();
                match change {
                    "visit" => v.generation = next_id(),
                    "read" => v.read = next_id(),
                    "revision" => v.revision = next_id(),
                    "selection" => v.layout.selection_generation = next_id(),
                    "stale" => v.stale = true,
                    "loading" => v.loading = true,
                    _ => unreachable!(),
                }
            }
        }
        let before = loaded(&shell);
        assert!(
            !finish(&mut shell, &attempt, Ok(response(&attempt))),
            "{change}"
        );
        assert_eq!(loaded(&shell).records, before.records, "{change}");
    }
}

#[test]
fn exact_baseline_and_detail_replacement_reject_same_revision_late_response() {
    for change in ["parent", "unrelated", "document", "close_reopen"] {
        let mut shell = shell(true);
        let id = open(&shell);
        let attempt = claim(&mut shell, id).unwrap();
        if change == "close_reopen" {
            assert!(super::super::close_shell(&mut shell, attempt.detail));
            assert!(super::super::open_created(&mut shell, &attempt.parent));
        } else {
            let loaded = shell
                .slack_lists
                .view
                .as_mut()
                .unwrap()
                .loaded
                .as_mut()
                .unwrap();
            if change == "document" {
                loaded.document.raw["access"] = json!("read");
            } else {
                let mut raw = loaded.records.raw.clone();
                let index = if change == "parent" {
                    0
                } else {
                    raw["records"].as_array().unwrap().len() - 1
                };
                raw["records"][index]["changed"] = json!(true);
                loaded.records = RecordsPage::parse(raw).unwrap();
            }
        }
        let before = loaded(&shell);
        assert!(
            !finish(&mut shell, &attempt, Ok(response(&attempt))),
            "{change}"
        );
        assert_eq!(loaded(&shell).records, before.records);
    }
}

#[test]
fn errors_preserve_population_consume_claim_and_honor_cooldown() {
    for error in [
        Error::Offline("offline".into()),
        Error::Transport("lost".into()),
        Error::Api("record_deleted".into()),
        Error::RateLimited {
            retry_after_secs: Some(60),
        },
    ] {
        let mut shell = shell(true);
        let id = open(&shell);
        let attempt = claim(&mut shell, id).unwrap();
        let before = loaded(&shell);
        let cooldown = retry_at(&error);
        assert!(!finish(&mut shell, &attempt, Err(error)));
        assert_eq!(loaded(&shell).records, before.records);
        assert!(claim(&mut shell, id).is_none());
        assert!(shell.slack_lists.view.as_ref().unwrap().retry_at >= cooldown);
        assert!(
            shell
                .slack_lists
                .view
                .as_ref()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .contains("Previously loaded")
        );
        assert!(super::super::close_shell(&mut shell, attempt.detail));
        assert!(super::super::open_created(&mut shell, &attempt.parent));
        let new_open = open(&shell);
        assert_eq!(claim(&mut shell, new_open).is_some(), cooldown <= now());
    }
}

#[test]
fn active_edit_prevents_hydration_from_replacing_editor_baseline() {
    let mut shell = shell(true);
    let id = open(&shell);
    let attempt = claim(&mut shell, id).unwrap();
    let column = loaded(&shell)
        .document
        .columns
        .iter()
        .find(|c| c.kind == "checkbox")
        .unwrap()
        .id
        .clone();
    let edit = super::super::begin_checkbox_shell(&mut shell, attempt.detail, &column).unwrap();
    let before = loaded(&shell);
    assert!(!finish(&mut shell, &attempt, Ok(response(&attempt))));
    assert_eq!(loaded(&shell).records, before.records);
    assert!(
        shell
            .slack_lists
            .operations
            .contains_key(edit.edit.list_id())
    );
}

#[test]
fn cookie_and_enterprise_rotation_revoke_exact_request_context() {
    use super_platinum_core::config::{Session, WorkspaceSession};
    for field in ["cookie", "enterprise"] {
        let mut shell = shell(true);
        let team = shell.core.active_team.clone().unwrap();
        shell.core.session = Some(Session {
            unified_user_client_enabled: Default::default(),
            d_cookie: "synthetic-cookie".into(),
            workspaces: BTreeMap::from([(
                team.clone(),
                WorkspaceSession {
                    team_id: team.clone(),
                    user_id: shell.self_account.user_id.clone(),
                    enterprise_id: None,
                    name: "Fixture".into(),
                    url: "https://fixture.slack.com".into(),
                    token: "synthetic-token".into(),
                },
            )]),
        });
        shell.slack_lists.scope = Scope::capture(&shell);
        let id = open(&shell);
        let attempt = claim(&mut shell, id).unwrap();
        assert!(attempt.scope.current(&shell));
        let session = shell.core.session.as_mut().unwrap();
        if field == "cookie" {
            session.d_cookie = "rotated".into();
        } else {
            session.workspaces.get_mut(&team).unwrap().enterprise_id = Some("E_OTHER".into());
        }
        assert!(
            !attempt.scope.current(&shell),
            "the shared List scope must reject cookie and enterprise rotation"
        );
        let before = loaded(&shell);
        assert!(!finish(&mut shell, &attempt, Ok(response(&attempt))));
        assert_eq!(loaded(&shell).records, before.records);
    }
}

#[test]
fn complete_mount_can_claim_when_a_canonical_refresh_first_reveals_incompleteness() {
    let mut shell = shell(false);
    let id = open(&shell);
    assert!(claim(&mut shell, id).is_none());
    let mut data = loaded(&shell);
    let mut raw = data.records.raw.clone();
    raw["response_metadata"] = json!({"next_cursor":"opaque"});
    data.records = RecordsPage::parse(raw).unwrap();
    let read = begin_read(&mut shell).unwrap();
    assert!(!finish_read(&mut shell, &read, Ok(data)));
    assert_eq!(open(&shell), id);
    assert!(claim(&mut shell, id).is_some());
}

#[test]
fn valid_hydration_for_another_known_parent_cannot_be_misassociated() {
    let mut shell = shell(true);
    let id = open(&shell);
    let attempt = claim(&mut shell, id).unwrap();
    let other = attempt
        .page
        .records
        .iter()
        .find(|row| row.id != attempt.parent)
        .unwrap();
    let foreign = ListRecordHydration::from_response(
        &attempt.list,
        &other.id,
        &json!({
            "ok":true,"list":attempt.document.raw,"record":other.raw,"subtasks":[]
        }),
    )
    .unwrap();
    let before = loaded(&shell);
    assert!(!finish(&mut shell, &attempt, Ok(foreign)));
    assert_eq!(loaded(&shell).records, before.records);
}

#[test]
fn late_429_after_detail_reopen_blocks_new_claim_and_already_claimed_dispatch() {
    let mut shell = shell(true);
    let id = open(&shell);
    let old = claim(&mut shell, id).unwrap();
    assert!(super::super::close_shell(&mut shell, old.detail));
    assert!(super::super::open_created(&mut shell, &old.parent));
    let id = open(&shell);
    let queued = claim(&mut shell, id).unwrap();
    assert!(queued.dispatch_ready(&shell));
    let before = loaded(&shell);
    assert!(!finish(
        &mut shell,
        &old,
        Err(Error::RateLimited {
            retry_after_secs: Some(60)
        })
    ));
    assert_eq!(loaded(&shell).records, before.records);
    assert!(queued.current(&shell));
    assert!(!queued.dispatch_ready(&shell));
    assert!(super::super::close_shell(&mut shell, queued.detail));
    assert!(super::super::open_created(&mut shell, &old.parent));
    let id = open(&shell);
    assert!(claim(&mut shell, id).is_none());
}

#[test]
fn late_cooldown_survives_list_close_and_fresh_list_read() {
    let mut shell = shell(true);
    let id = open(&shell);
    let old = claim(&mut shell, id).unwrap();
    let data = loaded(&shell);
    let file: File = serde_json::from_value(data.document.raw.clone()).unwrap();
    crate::slack_lists::close_shell(&mut shell);
    assert!(!finish(
        &mut shell,
        &old,
        Err(Error::HttpStatus {
            status: 503,
            retry_after_secs: Some(75)
        })
    ));
    assert!(crate::slack_lists::open_shell(&mut shell, file));
    let read = begin_read(&mut shell).unwrap();
    assert!(!finish_read(&mut shell, &read, Ok(data)));
    assert!(super::super::open_created(&mut shell, &old.parent));
    assert_eq!(
        shell.slack_lists.view.as_ref().unwrap().retry_at,
        0,
        "fresh view cannot erase retained cooldown"
    );
    let id = open(&shell);
    assert!(claim(&mut shell, id).is_none());
}

#[test]
fn late_429_is_retained_for_original_account_across_shell_replacement_and_return() {
    let mut original = shell(true);
    let id = open(&original);
    let old = claim(&mut original, id).unwrap();
    let account = original.core.active_account.clone();
    let mut other = shell(true);
    let before = loaded(&other);
    assert!(!finish(
        &mut other,
        &old,
        Err(Error::RateLimited {
            retry_after_secs: None
        })
    ));
    assert_eq!(loaded(&other).records, before.records);
    let id = open(&other);
    assert!(
        claim(&mut other, id).is_some(),
        "another owner is not throttled"
    );
    let mut returned = shell(true);
    returned.core.active_account = account;
    returned.core.account_epoch += 7;
    returned.slack_lists.scope = Scope::capture(&returned);
    let id = open(&returned);
    assert!(
        claim(&mut returned, id).is_none(),
        "same stable owner remains throttled after new ShellState/epoch"
    );
}

#[test]
fn current_default_429_and_explicit_503_set_ui_hint_and_retained_deadline() {
    for error in [
        Error::HttpStatus {
            status: 429,
            retry_after_secs: None,
        },
        Error::HttpStatus {
            status: 503,
            retry_after_secs: Some(75),
        },
    ] {
        let mut shell = shell(true);
        let id = open(&shell);
        let attempt = claim(&mut shell, id).unwrap();
        let before = now();
        assert!(!finish(&mut shell, &attempt, Err(error)));
        assert!(shell.slack_lists.view.as_ref().unwrap().retry_at >= before + 30);
        assert!(cooldown::blocked(&attempt.scope));
    }
}

#[test]
fn late_hydration_limit_blocks_subitem_and_schema_routes_after_list_reopen() {
    for error in [
        Error::RateLimited {
            retry_after_secs: Some(60),
        },
        Error::HttpStatus {
            status: 503,
            retry_after_secs: Some(60),
        },
    ] {
        let mut shell = shell(true);
        let id = open(&shell);
        let old = claim(&mut shell, id).unwrap();
        let data = loaded(&shell);
        let file: File = serde_json::from_value(data.document.raw.clone()).unwrap();
        crate::slack_lists::close_shell(&mut shell);
        assert!(!finish(&mut shell, &old, Err(error)));
        assert!(crate::slack_lists::open_shell(&mut shell, file));
        let read = begin_read(&mut shell).unwrap();
        assert!(!finish_read(&mut shell, &read, Ok(data)));
        assert!(super::super::open_created(&mut shell, &old.parent));
        assert!(crate::slack_lists::subitem::blocked(&shell));
        assert!(!crate::slack_lists::create::presentation(&shell).enabled);
        assert!(rename::creation_cooldown(
            shell.slack_lists.scope.as_ref().unwrap()
        ));
    }
}

#[test]
fn shared_create_limit_blocks_already_claimed_and_reopened_hydration() {
    for error in [
        Error::RateLimited {
            retry_after_secs: Some(60),
        },
        Error::HttpStatus {
            status: 503,
            retry_after_secs: Some(60),
        },
    ] {
        let mut shell = shell(true);
        let id = open(&shell);
        let queued = claim(&mut shell, id).unwrap();
        assert!(queued.dispatch_ready(&shell));
        assert!(!cooldown::blocked(&queued.scope));
        // Same stable-owner publication used by actual subitem completion.
        // Its completion/explicit-retirement path is independently exercised by
        // subitem::tests::explicit_retirement_does_not_reset_known_creation_limit.
        rename::record_creation_limit(&queued.scope, &error);
        assert!(queued.current(&shell));
        assert!(!queued.dispatch_ready(&shell));
        assert!(
            !cooldown::blocked(&queued.scope),
            "shared fence must suffice without hydration-local history"
        );
        assert!(super::super::close_shell(&mut shell, queued.detail));
        assert!(super::super::open_created(&mut shell, &queued.parent));
        let id = open(&shell);
        assert!(claim(&mut shell, id).is_none());
    }
}
