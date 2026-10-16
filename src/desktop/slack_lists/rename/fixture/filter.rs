//! Three explicit full-vector filter writes through the real builder and reducers.
use super::*;
pub(crate) fn filter_check_enabled() -> bool {
    let legacy = std::env::var("SUPER_PLATINUM_LIST_VIEW_FILTER_CHECK").ok();
    let number = std::env::var("SUPER_PLATINUM_LIST_NUMBER_FILTER_CHECK").ok();
    let alias = std::env::var("SUPER_PLATINUM_FIXTURE").ok();
    mode_gate(legacy.as_deref(), number.as_deref(), alias.as_deref()).is_some()
}
pub(crate) fn number_filter_check_enabled() -> bool {
    let legacy = std::env::var("SUPER_PLATINUM_LIST_VIEW_FILTER_CHECK").ok();
    let number = std::env::var("SUPER_PLATINUM_LIST_NUMBER_FILTER_CHECK").ok();
    let alias = std::env::var("SUPER_PLATINUM_FIXTURE").ok();
    mode_gate(legacy.as_deref(), number.as_deref(), alias.as_deref()) == Some(true)
}
fn mode_gate(legacy: Option<&str>, number: Option<&str>, alias: Option<&str>) -> Option<bool> {
    if number != Some("1") && gate(legacy, alias) {
        return Some(false);
    }
    if legacy != Some("1")
        && number == Some("1")
        && matches!(
            alias,
            Some("slack-list-number-filter" | "slack-list-number-filter-narrow")
        )
    {
        return Some(true);
    }
    None
}
fn gate(flag: Option<&str>, alias: Option<&str>) -> bool {
    flag == Some("1")
        && matches!(
            alias,
            Some("slack-list-view-filter" | "slack-list-view-filter-narrow")
        )
}
pub(crate) fn filter_registry_empty(shell: &ShellState) -> Result<bool, String> {
    if !filter_check_enabled() || !enabled(shell) {
        return Err("Owned Filter fixture required".into());
    }
    Ok(entry(shell).is_none() && super::super::presentation(shell).is_none())
}
pub(crate) fn seed_filter(shell: &ShellState, server: Value) {
    assert!(filter_check_enabled() && enabled(shell));
    *ledger().lock().unwrap() = Ledger {
        server: Some(server),
        ..Default::default()
    };
}
pub(crate) fn stale_filter_file(shell: &ShellState) -> Result<(), String> {
    if !filter_check_enabled() || !enabled(shell) {
        return Err("Exact disconnected Filter checker required".into());
    }
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Uncertain
                && e.target.is_filter()
                && matches!(e.intent, Some(Intent::Filter(_)))
        })
        .ok_or("No uncertain Filter intent")?;
    ledger()
        .lock()
        .unwrap()
        .queue_precision_file(e.original.raw)
}
/// Setup a replacement visit from the captured canonical snapshot while the
/// old check remains held. This does not claim ordinary reopen preserves Find.
pub(crate) fn seed_filter_reopen(
    shell: &mut ShellState,
    visit: u64,
    loaded: crate::slack_lists::Loaded,
    find: crate::slack_lists::find::State,
) -> Result<(), String> {
    if !filter_check_enabled() || !enabled(shell) || ledger().lock().unwrap().held_read.is_none() {
        return Err("Held disconnected Filter check required".into());
    }
    seed_reopened(shell, visit, loaded, find)
}
fn seed_reopened(
    shell: &mut ShellState,
    visit: u64,
    loaded: crate::slack_lists::Loaded,
    find: crate::slack_lists::find::State,
) -> Result<(), String> {
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Checking
                && e.target == Target::Filter("V_FILTER".into())
                && e.intent.is_some()
                && e.job.is_some()
        })
        .ok_or("Missing retained Filter check")?;
    if generation(shell) != Some(visit)
        || e.original != loaded.document
        || e.view_selection == selection(shell)
        || find.input != "Launch item 0"
        || find.applied != find.input
        || find.revision == 0
    {
        return Err("Wrong captured snapshot/Find or replacement visit".into());
    }
    let v = shell
        .slack_lists
        .view
        .as_mut()
        .ok_or("Missing replacement List")?;
    if v.id != loaded.document.id
        || v.loaded.is_some()
        || v.loading
        || v.unavailable
        || v.read != 0
        || v.revision != 0
        || !v.layout.find.input.is_empty()
        || !v.layout.find.applied.is_empty()
    {
        return Err("Replacement List is not an unpopulated fixture visit".into());
    }
    v.title = loaded.document.title.clone();
    v.loaded = Some(loaded);
    v.stale = false;
    v.revision = next_id();
    v.layout.find = find;
    crate::slack_lists::rebuild_rows(shell);
    Ok(())
}
fn vector(index: usize) -> Option<Value> {
    vector_for(index, number_filter_check_enabled())
}
fn vector_for(index: usize, number: bool) -> Option<Value> {
    if number {
        return match index {
            0 => Some(json!([{"key":"estimate","operator":"less_than","values":["12.50"]}])),
            1 => Some(json!([{"key":"estimate","operator":"is_empty","values":[]}])),
            2 => Some(json!([])),
            _ => None,
        };
    }
    match index {
        0 => Some(json!([{"key":"done","operator":"is_not_empty","values":[]}])),
        1 => Some(json!([{"key":"done","operator":"is_empty","values":[]}])),
        2 => Some(json!([])),
        _ => None,
    }
}
fn set_filter(server: &mut Value, filters: Value) -> Result<(), Error> {
    let v = server["list_metadata"]["views"]
        .as_array_mut()
        .and_then(|v| v.iter_mut().find(|v| v["id"] == "V_FILTER"))
        .ok_or_else(|| Error::Api("fixture_filter_view".into()))?;
    v["filters"] = filters;
    v["info_column_filters"] = json!([]);
    Ok(())
}
fn reply(server: &Value) -> Value {
    json!({"ok":true,"list":{"id":server["id"],"list_metadata":server["list_metadata"]},"opaque_reply":{"filter":true}})
}
pub(super) async fn write(
    state: Signal<ShellState>,
    guard: &Guard,
    permit: Permit,
) -> Result<Value, ListColumnRenameFailure> {
    if !filter_check_enabled()
        || !enabled(&state.peek())
        || !guard.current(&state.peek(), Stage::Saving)
        || registry::blocked(&guard.key.owner)
        || !permit.is_filter()
    {
        return Err(ListColumnRenameFailure::NotRequested(
            Error::TransportNotConfigured,
        ));
    }
    let desired = with_registry(|r| {
        r.entries.get(&guard.key).and_then(|e| match &e.intent {
            Some(Intent::Filter(i)) if e.target == Target::Filter("V_FILTER".into()) => {
                (i.info_column_filters() == json!([])).then(|| i.filters().clone())
            }
            _ => None,
        })
    })
    .ok_or(ListColumnRenameFailure::NotRequested(
        Error::TransportNotConfigured,
    ))?;
    let index = {
        let l = ledger().lock().unwrap();
        if l.pending.is_some() || l.held_read.is_some() {
            return Err(ListColumnRenameFailure::NotRequested(Error::Api(
                "fixture_filter_pending".into(),
            )));
        }
        l.requests
            .iter()
            .filter(|r| r["path"] == "/api/lists.views.update")
            .count()
    };
    if vector(index).as_ref() != Some(&desired) {
        return Err(ListColumnRenameFailure::NotRequested(Error::Api(
            "fixture_filter_order".into(),
        )));
    }
    let request = permit.request(&state.peek().core.client, &session());
    let expected = vec![
        ("token".into(), "fixture-not-a-token".into()),
        ("list_id".into(), "F_LIST_FIXTURE".into()),
        ("id".into(), "V_FILTER".into()),
        ("filters".into(), desired.to_string()),
        ("info_column_filters".into(), "[]".into()),
        ("_x_reason".into(), "update-view".into()),
    ];
    if !matches!(&request.body,RequestBody::Form(fields) if fields==&expected) {
        return Err(ListColumnRenameFailure::NotRequested(Error::Api(
            "fixture_filter_form".into(),
        )));
    }
    if record(request).map_err(ListColumnRenameFailure::NotRequested)? != "/api/lists.views.update"
    {
        return Err(ListColumnRenameFailure::NotRequested(Error::Api(
            "fixture_filter_path".into(),
        )));
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut l = ledger().lock().unwrap();
        let response = if index == 0 {
            json!({"syntheticFailure":"response_lost"})
        } else {
            let mut server = l.server.clone().unwrap();
            set_filter(&mut server, desired.clone())
                .map_err(ListColumnRenameFailure::NotRequested)?;
            reply(&server)
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
    set_filter(l.server.as_mut().unwrap(), desired)
        .map_err(ListColumnRenameFailure::Unconfirmed)?;
    if index == 0 {
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "fixture_response_lost".into(),
        )))
    } else {
        Ok(l.responses.last().unwrap().clone())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn number_mode_is_closed_and_vectors_keep_lexical_values() {
        for alias in [
            "slack-list-number-filter",
            "slack-list-number-filter-narrow",
        ] {
            assert_eq!(mode_gate(None, Some("1"), Some(alias)), Some(true));
            assert_eq!(mode_gate(Some("1"), Some("1"), Some(alias)), None);
            assert_eq!(mode_gate(Some("1"), None, Some(alias)), None);
        }
        assert_eq!(
            mode_gate(None, Some("1"), Some("slack-list-view-filter")),
            None
        );
        assert_eq!(
            mode_gate(None, Some("1"), Some("slack-list-number-filter-extra")),
            None
        );
        assert_eq!(vector_for(0, true).unwrap()[0]["values"], json!(["12.50"]));
        assert_eq!(vector_for(1, true).unwrap()[0]["values"], json!([]));
        assert_eq!(vector_for(2, true), Some(json!([])));
        assert!(vector_for(3, true).is_none());
        assert_eq!(vector_for(0, false).unwrap()[0]["key"], "done");
    }
    #[test]
    fn only_exact_opted_in_aliases_enable_filter_executor() {
        for alias in ["slack-list-view-filter", "slack-list-view-filter-narrow"] {
            assert!(gate(Some("1"), Some(alias)));
            for flag in [None, Some("true"), Some("0"), Some("")] {
                assert!(!gate(flag, Some(alias)));
            }
        }
        for alias in [
            None,
            Some("slack-list-field-visibility"),
            Some("slack-list-view-filter-extra"),
        ] {
            assert!(!gate(Some("1"), alias));
        }
    }
    #[test]
    fn exact_full_vectors_change_only_target_filter_and_clear_restores_original() {
        let before = json!({"id":"F_LIST_FIXTURE","opaque":7,"list_metadata":{"schema":[{"id":"C_CHECK","key":"done","type":"checkbox"},{"id":"C_NUMBER","key":"estimate","type":"number"}],"views":[{"id":"V_FILTER","filters":[],"info_column_filters":[],"sorts":[{"key":"estimate","ascending":true}],"columns":[{"id":"C_CHECK","visible":true}],"opaque":false},{"id":"other","filters":[{"key":"other","operator":"is_empty","values":[]}]}]}});
        let mut server = before.clone();
        for index in 0..3 {
            let desired = vector(index).unwrap();
            set_filter(&mut server, desired.clone()).unwrap();
            assert_eq!(server["list_metadata"]["views"][0]["filters"], desired);
            let mut restored = server.clone();
            restored["list_metadata"]["views"][0]["filters"] = json!([]);
            assert_eq!(restored, before);
        }
        assert_eq!(server, before);
        assert!(vector(3).is_none());
    }
    #[test]
    fn replacement_seed_keeps_full_canonical_population_and_active_find_before_retired_read() {
        use super_platinum_core::slack_lists::ViewFilterChoice;
        let mut shell = ShellState::fixture_variant(
            crate::media::MediaRegistry::default(),
            "slack-list-view-filter",
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
        assert!(crate::slack_lists::rename::filter::change(
            &shell,
            e.serial,
            generation(&shell).unwrap(),
            e.filter_choice,
            &ViewFilterChoice::None,
            ViewFilterChoice::Checkbox {
                column_id: "C_CHECK".into(),
                checked: true
            }
        ));
        let v = shell.slack_lists.view.as_mut().unwrap();
        v.layout.find.input = "Launch item 0".into();
        v.layout.find.applied = "Launch item 0".into();
        v.layout.find.revision = 37;
        crate::slack_lists::rebuild_rows(&mut shell);
        let saved = shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .clone()
            .unwrap();
        let find = shell.slack_lists.view.as_ref().unwrap().layout.find.clone();
        let (write, intent) = prepare(&mut shell, e.serial).unwrap();
        admitted(
            &shell,
            &write,
            &intent,
            &network::Metadata {
                document: saved.document.clone(),
                allow: saved.allow_lists.clone(),
                personal: saved.personal_task_lists.clone().unwrap(),
            },
        )
        .unwrap();
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
        set_filter(&mut changed, vector(0).unwrap()).unwrap();
        crate::slack_lists::close_shell(&mut shell);
        shell.core.account_epoch += 1;
        assert!(crate::slack_lists::open_shell(
            &mut shell,
            serde_json::from_value(changed.clone()).unwrap()
        ));
        let visit = generation(&shell).unwrap();
        assert_ne!(visit, old_visit);
        assert_ne!(selection(&shell), Some(old_selection));
        crate::slack_lists::fixture::refresh(&mut shell);
        assert!(shell.slack_lists.view.as_ref().unwrap().loaded.is_none());
        assert!(blocks_read(&shell));
        let mut wrong = saved.clone();
        wrong.document = Document::parse(changed.clone()).unwrap();
        assert!(seed_reopened(&mut shell, visit, wrong, find.clone()).is_err());
        assert!(seed_reopened(&mut shell, old_visit, saved.clone(), find.clone()).is_err());
        assert!(seed_reopened(&mut shell, visit, saved.clone(), Default::default()).is_err());
        seed_reopened(&mut shell, visit, saved.clone(), find.clone()).unwrap();
        let same = |s: &ShellState| {
            let v = s.slack_lists.view.as_ref().unwrap();
            assert_eq!(v.loaded.as_ref().unwrap().document, saved.document);
            assert_eq!(v.loaded.as_ref().unwrap().records, saved.records);
            assert_eq!(
                (
                    &v.layout.find.input,
                    &v.layout.find.applied,
                    v.layout.find.revision
                ),
                (&find.input, &find.applied, find.revision)
            );
            assert_eq!(v.layout.selected.as_deref(), Some("V_FILTER"));
            assert_eq!(
                v.layout
                    .entries
                    .iter()
                    .filter_map(|e| match e {
                        crate::slack_lists::layout::TableEntry::Row(r) => Some(r.id.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                [2, 3, 1, 4, 7, 8, 0, 5, 6]
                    .into_iter()
                    .map(|i| format!("R_LIST_{i:03}"))
                    .collect::<Vec<_>>()
            );
        };
        same(&shell);
        let retained = entry(&shell).unwrap();
        assert_eq!(retained.stage, Stage::Checking);
        assert_eq!(retained.job, Some(guard.job));
        assert_eq!(retained.serial, e.serial);
        assert!(!guard.current(&shell, Stage::Checking));
        assert!(!crate::slack_lists::rename::checked(
            &mut shell,
            &guard,
            Ok(network::Metadata {
                document: Document::parse(changed).unwrap(),
                allow: saved.allow_lists.clone(),
                personal: saved.personal_task_lists.clone().unwrap()
            })
        ));
        same(&shell);
        assert_eq!(entry(&shell).unwrap().stage, Stage::Uncertain);
        assert!(seed_reopened(&mut shell, visit, saved.clone(), find.clone()).is_err());
    }
}
