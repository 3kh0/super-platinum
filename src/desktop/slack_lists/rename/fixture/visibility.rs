//! Exact two-attempt native visibility executor; no HTTP or live fallback.
use super::*;
pub(crate) fn visibility_check_enabled() -> bool {
    gate(
        std::env::var("SUPER_PLATINUM_LIST_FIELD_VISIBILITY_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
fn gate(flag: Option<&str>, alias: Option<&str>) -> bool {
    flag == Some("1")
        && matches!(
            alias,
            Some("slack-list-field-visibility" | "slack-list-field-visibility-narrow")
        )
}
pub(crate) fn seed_visibility(shell: &ShellState, server: Value) {
    assert!(visibility_check_enabled() && enabled(shell));
    *ledger().lock().unwrap() = Ledger {
        server: Some(server),
        ..Default::default()
    };
}
pub(crate) fn stale_visibility_file(shell: &ShellState) -> Result<(), String> {
    if !visibility_check_enabled() || !enabled(shell) {
        return Err("Exact disconnected visibility checker required".into());
    }
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Uncertain
                && e.target.is_visibility()
                && matches!(e.intent, Some(Intent::Visibility(_)))
        })
        .ok_or("No uncertain visibility intent")?;
    ledger()
        .lock()
        .unwrap()
        .queue_precision_file(e.original.raw)
}
/// Fixture setup only: a retained Checking entry intentionally blocks begin_read.
/// Establish the previously captured canonical snapshot in the actual new visit
/// while the old completion remains held. Never edits the registry or response.
pub(crate) fn seed_visibility_reopen(
    shell: &mut ShellState,
    visit: u64,
    loaded: crate::slack_lists::Loaded,
) -> Result<(), String> {
    if !visibility_check_enabled()
        || !enabled(shell)
        || ledger().lock().unwrap().held_read.is_none()
    {
        return Err("Held disconnected visibility check required".into());
    }
    seed_reopened(shell, visit, loaded)
}
fn seed_reopened(
    shell: &mut ShellState,
    visit: u64,
    loaded: crate::slack_lists::Loaded,
) -> Result<(), String> {
    let entry = entry(shell)
        .filter(|e| {
            e.stage == Stage::Checking
                && e.target.is_visibility()
                && e.intent.is_some()
                && e.job.is_some()
        })
        .ok_or("Missing retained visibility check")?;
    if generation(shell) != Some(visit)
        || entry.original != loaded.document
        || entry.view_selection == selection(shell)
    {
        return Err("Wrong captured snapshot or replacement visit".into());
    }
    let view = shell
        .slack_lists
        .view
        .as_mut()
        .ok_or("Missing replacement List")?;
    if view.id != loaded.document.id
        || view.loaded.is_some()
        || view.loading
        || view.unavailable
        || view.read != 0
        || view.revision != 0
    {
        return Err("Replacement List is not an unpopulated fixture visit".into());
    }
    view.title = loaded.document.title.clone();
    view.loaded = Some(loaded);
    view.stale = false;
    view.revision = next_id();
    crate::slack_lists::rebuild_rows(shell);
    Ok(())
}
fn set_visible(server: &mut Value, visible: bool) -> Result<(), Error> {
    let columns = server["list_metadata"]["views"]
        .as_array_mut()
        .and_then(|views| views.iter_mut().find(|v| v["id"] == "V_PRECISION"))
        .and_then(|v| v["columns"].as_array_mut())
        .ok_or_else(|| Error::Api("fixture_visibility_view".into()))?;
    let column = columns
        .iter_mut()
        .find(|c| c["key"] == "estimate" && c["id"] == "C_NUMBER")
        .ok_or_else(|| Error::Api("fixture_visibility_column".into()))?;
    column["visible"] = json!(visible);
    Ok(())
}
fn reply(server: &Value) -> Value {
    json!({"ok":true,"list":{"id":server["id"],"list_metadata":server["list_metadata"]},"opaque_reply":{"visibility":true}})
}
pub(super) async fn write(
    state: Signal<ShellState>,
    guard: &Guard,
    permit: Permit,
) -> Result<Value, ListColumnRenameFailure> {
    if !visibility_check_enabled()
        || !enabled(&state.peek())
        || !guard.current(&state.peek(), Stage::Saving)
        || registry::blocked(&guard.key.owner)
        || !permit.is_visibility()
    {
        return Err(ListColumnRenameFailure::NotRequested(
            Error::TransportNotConfigured,
        ));
    }
    let desired = with_registry(|r| {
        r.entries.get(&guard.key).and_then(|e| match &e.input {
            Input::Visibility(v) => Some(*v),
            _ => None,
        })
    })
    .ok_or(ListColumnRenameFailure::NotRequested(
        Error::TransportNotConfigured,
    ))?;
    let target = with_registry(|r| r.entries[&guard.key].target.clone());
    if target
        != (Target::Visibility {
            view: "V_PRECISION".into(),
            column: "C_NUMBER".into(),
        })
    {
        return Err(ListColumnRenameFailure::NotRequested(Error::Api(
            "fixture_visibility_target".into(),
        )));
    }
    let request = permit.request(&state.peek().core.client, &session());
    let expected = vec![
        ("token".into(), "fixture-not-a-token".into()),
        ("list_id".into(), "F_LIST_FIXTURE".into()),
        ("id".into(), "V_PRECISION".into()),
        (
            "columns".into(),
            json!([{"key":"estimate","visible":desired,"id":"C_NUMBER"}]).to_string(),
        ),
        ("_x_reason".into(), "update-view".into()),
    ];
    if !matches!(&request.body,RequestBody::Form(fields) if fields==&expected) {
        return Err(ListColumnRenameFailure::NotRequested(Error::Api(
            "fixture_visibility_form".into(),
        )));
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let l = ledger().lock().unwrap();
        let writes = l
            .requests
            .iter()
            .filter(|r| r["path"] == "/api/lists.views.update")
            .count();
        if l.pending.is_some() || l.held_read.is_some() || writes >= 2 || desired != (writes == 1) {
            return Err(ListColumnRenameFailure::NotRequested(Error::Api(
                "fixture_visibility_order".into(),
            )));
        }
    }
    if record(request).map_err(ListColumnRenameFailure::NotRequested)? != "/api/lists.views.update"
    {
        return Err(ListColumnRenameFailure::NotRequested(Error::Api(
            "fixture_visibility_path".into(),
        )));
    }
    {
        let mut l = ledger().lock().unwrap();
        let response = if desired {
            let mut server = l.server.clone().unwrap();
            set_visible(&mut server, true).map_err(ListColumnRenameFailure::NotRequested)?;
            reply(&server)
        } else {
            json!({"syntheticFailure":"response_lost"})
        };
        l.responses.push(response);
        l.pending = Some(tx);
    }
    tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| {
            ListColumnRenameFailure::Unconfirmed(Error::Transport("fixture_timeout".into()))
        })?
        .map_err(|_| {
            ListColumnRenameFailure::Unconfirmed(Error::Transport("fixture_closed".into()))
        })?;
    let mut l = ledger().lock().unwrap();
    set_visible(l.server.as_mut().unwrap(), desired)
        .map_err(ListColumnRenameFailure::Unconfirmed)?;
    if desired {
        Ok(l.responses.last().unwrap().clone())
    } else {
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "fixture_response_lost".into(),
        )))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_flag_and_alias_required() {
        for alias in [
            "slack-list-field-visibility",
            "slack-list-field-visibility-narrow",
        ] {
            assert!(gate(Some("1"), Some(alias)));
            for f in [None, Some(""), Some("true"), Some("0")] {
                assert!(!gate(f, Some(alias)));
            }
        }
        for a in [
            None,
            Some("slack-list-number-rename"),
            Some("slack-list-field-visibility-extra"),
        ] {
            assert!(!gate(Some("1"), a));
        }
    }
    #[test]
    fn synthetic_server_changes_only_one_visibility_bit() {
        let original = json!({"id":"F_LIST_FIXTURE","opaque":17,"list_metadata":{"schema":[{"id":"C_NUMBER","type":"number","options":{"precision":2}}],"views":[{"id":"V_PRECISION","columns":[{"id":"C_NAME","key":"NAME","visible":true},{"id":"C_NUMBER","key":"estimate","visible":true,"opaque":false}],"filters":[],"sorts":[],"opaque":7}]}});
        let mut hidden = original.clone();
        set_visible(&mut hidden, false).unwrap();
        assert_eq!(
            hidden["list_metadata"]["views"][0]["columns"][1]["visible"],
            false
        );
        set_visible(&mut hidden, true).unwrap();
        assert_eq!(hidden, original);
    }
    #[test]
    fn held_check_reopen_requires_canonical_seed_before_obsolete_completion() {
        let mut shell = ShellState::fixture_variant(
            crate::media::MediaRegistry::default(),
            "slack-list-field-visibility",
        );
        let key = Key::current(&shell).unwrap();
        struct Cleanup(Key);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                with_registry(|r| {
                    r.entries.remove(&self.0);
                });
            }
        }
        let _cleanup = Cleanup(key.clone());
        let e = entry(&shell).unwrap();
        let Target::Visibility {
            column: rendered, ..
        } = &e.target
        else {
            panic!()
        };
        assert!(crate::slack_lists::rename::visibility::change(
            &shell,
            e.serial,
            generation(&shell).unwrap(),
            e.visibility_choice,
            rendered,
            Some("C_NUMBER"),
            None
        ));
        let e = entry(&shell).unwrap();
        assert!(crate::slack_lists::rename::visibility::change(
            &shell,
            e.serial,
            generation(&shell).unwrap(),
            e.visibility_choice,
            "C_NUMBER",
            None,
            Some(true)
        ));
        let saved = shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .clone()
            .unwrap();
        let (write, intent) = prepare(&mut shell, e.serial).unwrap();
        let metadata = network::Metadata {
            experiments: None,
            document: saved.document.clone(),
            allow: saved.allow_lists.clone(),
            personal: saved.personal_task_lists.clone().unwrap(),
        };
        assert!(admitted(&shell, &write, &intent, &metadata).is_some());
        crate::slack_lists::rename::completed(
            &mut shell,
            &write,
            Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
                "lost".into(),
            ))),
        );
        let guard = Guard::capture_read(&shell, key.clone(), e.serial).unwrap();
        with_registry(|r| {
            let e = r.entries.get_mut(&key).unwrap();
            e.job = Some(guard.job);
            e.stage = Stage::Checking;
        });
        let old_visit = generation(&shell).unwrap();
        let old_selection = selection(&shell).unwrap();
        let mut changed = saved.document.raw.clone();
        set_visible(&mut changed, true).unwrap();
        crate::slack_lists::close_shell(&mut shell);
        shell.core.account_epoch += 1;
        assert!(crate::slack_lists::open_shell(
            &mut shell,
            serde_json::from_value(changed.clone()).unwrap()
        ));
        let new_visit = generation(&shell).unwrap();
        assert_ne!(old_visit, new_visit);
        assert_ne!(Some(old_selection), selection(&shell));
        // This was the original checker setup. Checking correctly refuses the read.
        crate::slack_lists::fixture::refresh(&mut shell);
        assert!(shell.slack_lists.view.as_ref().unwrap().loaded.is_none());
        assert!(blocks_read(&shell));
        let mut wrong = saved.clone();
        wrong.document = Document::parse(changed.clone()).unwrap();
        assert!(seed_reopened(&mut shell, new_visit, wrong).is_err());
        assert!(seed_reopened(&mut shell, old_visit, saved.clone()).is_err());
        assert!(shell.slack_lists.view.as_ref().unwrap().loaded.is_none());
        seed_reopened(&mut shell, new_visit, saved.clone()).unwrap();
        let retained = entry(&shell).unwrap();
        assert_eq!(retained.stage, Stage::Checking);
        assert_eq!(retained.job, Some(guard.job));
        assert_eq!(retained.serial, e.serial);
        let before = shell.slack_lists.view.as_ref().unwrap();
        assert_eq!(before.loaded.as_ref().unwrap().document, saved.document);
        assert_eq!(before.loaded.as_ref().unwrap().records, saved.records);
        assert_eq!(before.layout.selected.as_deref(), Some("V_PRECISION"));
        assert_eq!(
            before.layout.kind,
            crate::slack_lists::ProjectionLayout::Table
        );
        assert_eq!(
            (
                &before.layout.find.input,
                &before.layout.find.applied,
                before.layout.find.revision
            ),
            (&String::new(), &String::new(), 0)
        );
        assert!(!guard.current(&shell, Stage::Checking));
        assert!(!crate::slack_lists::rename::checked(
            &mut shell,
            &guard,
            Ok(network::Metadata {
                experiments: None,
                document: Document::parse(changed).unwrap(),
                allow: saved.allow_lists.clone(),
                personal: saved.personal_task_lists.clone().unwrap()
            })
        ));
        let after = shell.slack_lists.view.as_ref().unwrap();
        assert_eq!(after.loaded.as_ref().unwrap().document, saved.document);
        assert_eq!(after.loaded.as_ref().unwrap().records, saved.records);
        assert_eq!(after.layout.selected.as_deref(), Some("V_PRECISION"));
        assert_eq!(
            (
                &after.layout.find.input,
                &after.layout.find.applied,
                after.layout.find.revision
            ),
            (&String::new(), &String::new(), 0)
        );
        assert_eq!(entry(&shell).unwrap().stage, Stage::Uncertain);
        assert!(seed_reopened(&mut shell, new_visit, saved).is_err());
    }
}
