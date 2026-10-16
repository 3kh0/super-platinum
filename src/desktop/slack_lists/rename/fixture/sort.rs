//! Three explicit full-vector sort writes through the real builder and reducers.
use super::*;
pub(crate) fn sort_check_enabled() -> bool {
    gate(
        std::env::var("SUPER_PLATINUM_LIST_VIEW_SORT_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
fn gate(flag: Option<&str>, alias: Option<&str>) -> bool {
    flag == Some("1")
        && matches!(
            alias,
            Some("slack-list-view-sort" | "slack-list-view-sort-narrow")
        )
}
pub(crate) fn seed_sort(shell: &ShellState, server: Value) {
    assert!(sort_check_enabled() && enabled(shell));
    *ledger().lock().unwrap() = Ledger {
        server: Some(server),
        ..Default::default()
    };
}
pub(crate) fn stale_sort_file(shell: &ShellState) -> Result<(), String> {
    if !sort_check_enabled() || !enabled(shell) {
        return Err("Exact disconnected Sort checker required".into());
    }
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Uncertain
                && e.target.is_sort()
                && matches!(e.intent, Some(Intent::Sort(_)))
        })
        .ok_or("No uncertain Sort intent")?;
    ledger()
        .lock()
        .unwrap()
        .queue_precision_file(e.original.raw)
}
/// Setup a replacement visit from the captured canonical snapshot while the
/// old check remains held. This does not claim ordinary reopen preserves Find.
pub(crate) fn seed_sort_reopen(
    shell: &mut ShellState,
    visit: u64,
    loaded: crate::slack_lists::Loaded,
    find: crate::slack_lists::find::State,
) -> Result<(), String> {
    if !sort_check_enabled() || !enabled(shell) || ledger().lock().unwrap().held_read.is_none() {
        return Err("Held disconnected Sort check required".into());
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
                && e.target == Target::Sort("V_SORT".into())
                && e.intent.is_some()
                && e.job.is_some()
        })
        .ok_or("Missing retained Sort check")?;
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
    match index {
        0 => Some(json!([{"key":"estimate","ascending":true}])),
        1 => Some(json!([{"key":"estimate","ascending":false}])),
        2 => Some(json!([])),
        _ => None,
    }
}
fn set_sort(server: &mut Value, sorts: Value) -> Result<(), Error> {
    let v = server["list_metadata"]["views"]
        .as_array_mut()
        .and_then(|v| v.iter_mut().find(|v| v["id"] == "V_SORT"))
        .ok_or_else(|| Error::Api("fixture_sort_view".into()))?;
    v["sorts"] = sorts;
    Ok(())
}
fn reply(server: &Value) -> Value {
    json!({"ok":true,"list":{"id":server["id"],"list_metadata":server["list_metadata"]},"opaque_reply":{"sort":true}})
}
pub(super) async fn write(
    state: Signal<ShellState>,
    guard: &Guard,
    permit: Permit,
) -> Result<Value, ListColumnRenameFailure> {
    if !sort_check_enabled()
        || !enabled(&state.peek())
        || !guard.current(&state.peek(), Stage::Saving)
        || registry::blocked(&guard.key.owner)
        || !permit.is_sort()
    {
        return Err(ListColumnRenameFailure::NotRequested(
            Error::TransportNotConfigured,
        ));
    }
    let desired = with_registry(|r| {
        r.entries.get(&guard.key).and_then(|e| match &e.intent {
            Some(Intent::Sort(i)) if e.target == Target::Sort("V_SORT".into()) => {
                Some(i.sorts().clone())
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
                "fixture_sort_pending".into(),
            )));
        }
        l.requests
            .iter()
            .filter(|r| r["path"] == "/api/lists.views.update")
            .count()
    };
    if vector(index).as_ref() != Some(&desired) {
        return Err(ListColumnRenameFailure::NotRequested(Error::Api(
            "fixture_sort_order".into(),
        )));
    }
    let request = permit.request(&state.peek().core.client, &session());
    let expected = vec![
        ("token".into(), "fixture-not-a-token".into()),
        ("list_id".into(), "F_LIST_FIXTURE".into()),
        ("id".into(), "V_SORT".into()),
        ("sorts".into(), desired.to_string()),
        ("_x_reason".into(), "update-view".into()),
    ];
    if !matches!(&request.body,RequestBody::Form(fields) if fields==&expected) {
        return Err(ListColumnRenameFailure::NotRequested(Error::Api(
            "fixture_sort_form".into(),
        )));
    }
    if record(request).map_err(ListColumnRenameFailure::NotRequested)? != "/api/lists.views.update"
    {
        return Err(ListColumnRenameFailure::NotRequested(Error::Api(
            "fixture_sort_path".into(),
        )));
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut l = ledger().lock().unwrap();
        let response = if index == 0 {
            json!({"syntheticFailure":"response_lost"})
        } else {
            let mut server = l.server.clone().unwrap();
            set_sort(&mut server, desired.clone())
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
    set_sort(l.server.as_mut().unwrap(), desired).map_err(ListColumnRenameFailure::Unconfirmed)?;
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
    fn only_exact_opted_in_aliases_enable_sort_executor() {
        for alias in ["slack-list-view-sort", "slack-list-view-sort-narrow"] {
            assert!(gate(Some("1"), Some(alias)));
            for flag in [None, Some("true"), Some("0"), Some("")] {
                assert!(!gate(flag, Some(alias)));
            }
        }
        for alias in [
            None,
            Some("slack-list-field-visibility"),
            Some("slack-list-view-sort-extra"),
        ] {
            assert!(!gate(Some("1"), alias));
        }
    }
    #[test]
    fn exact_full_vectors_change_only_target_sort_and_clear_restores_original() {
        let before = json!({"id":"F_LIST_FIXTURE","opaque":7,"list_metadata":{"schema":[{"id":"C_NUMBER","key":"estimate","type":"number"}],"views":[{"id":"V_SORT","sorts":[],"filters":[],"columns":[{"id":"C_NUMBER","visible":true}],"opaque":false},{"id":"other","sorts":[{"key":"other","ascending":true}]}]}});
        let mut server = before.clone();
        for index in 0..3 {
            let desired = vector(index).unwrap();
            set_sort(&mut server, desired.clone()).unwrap();
            assert_eq!(server["list_metadata"]["views"][0]["sorts"], desired);
            let mut restored = server.clone();
            restored["list_metadata"]["views"][0]["sorts"] = json!([]);
            assert_eq!(restored, before);
        }
        assert_eq!(server, before);
        assert!(vector(3).is_none());
    }
    #[test]
    fn replacement_seed_keeps_full_canonical_population_and_active_find_before_retired_read() {
        use super_platinum_core::slack_lists::ViewSortChoice;
        let mut shell = ShellState::fixture_variant(
            crate::media::MediaRegistry::default(),
            "slack-list-view-sort",
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
        assert!(crate::slack_lists::rename::sort::change(
            &shell,
            e.serial,
            generation(&shell).unwrap(),
            e.sort_choice,
            &ViewSortChoice::None,
            ViewSortChoice::Number {
                column_id: "C_NUMBER".into(),
                ascending: true
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
        set_sort(&mut changed, vector(0).unwrap()).unwrap();
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
            assert_eq!(v.layout.selected.as_deref(), Some("V_SORT"));
            assert_eq!(
                v.layout
                    .entries
                    .iter()
                    .filter_map(|e| match e {
                        crate::slack_lists::layout::TableEntry::Row(r) => Some(r.id.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                (0..9).map(|i| format!("R_LIST_{i:03}")).collect::<Vec<_>>()
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
