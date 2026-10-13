use super::*;
fn linear(
    workspace: &Workspace,
    bag: &ChannelMessages,
    history: &ThreadHistory,
    root: &str,
    media: &MediaRegistry,
    window: &crate::thread_window::ThreadWindow,
) -> Vec<MessageVm> {
    let selected = window
        .rows
        .iter()
        .enumerate()
        .filter(|(i, _)| window.selected(*i, root))
        .map(|(_, r)| r.ts.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut previous: Option<&super_platinum_core::slack::models::Message> = None;
    bag.messages
        .iter()
        .filter_map(|message| {
            let ts = message.ts.as_deref()?;
            if !selected.contains(ts) {
                previous = None;
                return None;
            }
            let mut vm = crate::message_vm::message_vm_with_pending(
                workspace,
                message,
                media,
                bag.is_pending(ts),
            );
            vm.compact = previous.is_some_and(|p| {
                p.ts.as_deref() != Some(root)
                    && p.ts.as_deref().is_some_and(|ts| {
                        history.contains(root, ts) == history.contains(root, &vm.ts)
                    })
                    && super_platinum_core::state::same_message_group(p, message)
            });
            previous = Some(message);
            Some(vm)
        })
        .collect()
}
#[test]
fn indexed_rich_projection_equals_reference_across_gaps_pending_and_missing_timestamp() {
    let core = crate::fixture::fixture_core();
    let workspace = &core.workspaces["T1"];
    let media = MediaRegistry::default();
    let mut bag = super::fixture_bag(300);
    let root = "1719800400.000000";
    bag.messages.insert(20, Default::default());
    bag.pending.push("1719800415.000000".into());
    let mut history = ThreadHistory::default();
    history.watch_live(1719800640);
    history.start(&ThreadHistory::initial(Some("1719800450.000000"),1719800640),root,&serde_json::from_value(serde_json::json!({"messages":[{"ts":root},{"ts":"1719800450.000000"},{"ts":"1719800460.000000"}],"has_more":true})).unwrap());
    for history in [&ThreadHistory::default(), &history] {
        for target in [None, Some("1719800455.000000")] {
            let mut window = crate::thread_window::ThreadWindow::default();
            window.sync("one", &bag, history, root, target, target.is_none());
            assert_eq!(
                project_window(workspace, &bag, history, root, &media, &window),
                linear(workspace, &bag, history, root, &media, &window)
            );
            window.measure(3, 9, &[], false);
            window.align_viewport(None, false);
            assert_eq!(
                project_window(workspace, &bag, history, root, &media, &window),
                linear(workspace, &bag, history, root, &media, &window)
            );
        }
    }
}
#[test]
fn shifted_or_replaced_source_rejects_complete_window_until_resynchronized() {
    let core = crate::fixture::fixture_core();
    let media = MediaRegistry::default();
    let mut bag = super::fixture_bag(300);
    let history = ThreadHistory::default();
    let root = "1719800400.000000";
    let mut window = crate::thread_window::ThreadWindow::default();
    window.sync("one", &bag, &history, root, None, true);
    bag.messages.remove(1);
    assert!(
        project_window(
            &core.workspaces["T1"],
            &bag,
            &history,
            root,
            &media,
            &window
        )
        .is_empty()
    );
    window.sync("one", &bag, &history, root, None, true);
    let a = window.rows[window.selected_indices()[1]].source_index;
    let b = window.rows[window.selected_indices()[2]].source_index;
    bag.messages.swap(a, b);
    assert!(
        project_window(
            &core.workspaces["T1"],
            &bag,
            &history,
            root,
            &media,
            &window
        )
        .is_empty(),
        "same-length reorder must reject old source indices"
    );
    bag.messages.swap(a, b);
    bag.pending.push(bag.messages[a].ts.clone().unwrap());
    assert!(
        project_window(
            &core.workspaces["T1"],
            &bag,
            &history,
            root,
            &media,
            &window
        )
        .is_empty(),
        "pending state changed without changing message count"
    );
    window.sync("one", &bag, &history, root, None, true);
    bag.pending.clear();
    assert!(
        project_window(
            &core.workspaces["T1"],
            &bag,
            &history,
            root,
            &media,
            &window
        )
        .is_empty(),
        "cleared pending state must retire the cache too"
    );
    window.sync("one", &bag, &history, root, None, true);
    let source = window.rows[window.selected_indices()[1]].source_index;
    bag.messages[source].ts = Some("9999999999.000000".into());
    assert!(
        project_window(
            &core.workspaces["T1"],
            &bag,
            &history,
            root,
            &media,
            &window
        )
        .is_empty()
    );
    window.sync("one", &bag, &history, root, None, true);
    assert_eq!(
        project_window(
            &core.workspaces["T1"],
            &bag,
            &history,
            root,
            &media,
            &window
        ),
        linear(
            &core.workspaces["T1"],
            &bag,
            &history,
            root,
            &media,
            &window
        )
    );
}

#[test]
fn viewport_refresh_falls_back_to_canonical_sync_after_prepend() {
    let mut shell = crate::state::ShellState::fixture(MediaRegistry::default());
    let root = "1719800400.000000";
    let key = (
        "T1".into(),
        shell.core.active_channel.clone().unwrap(),
        root.into(),
    );
    shell.thread_root = Some(root.into());
    shell
        .core
        .threads
        .insert(key.clone(), super::fixture_bag(300));
    shell.refresh_thread_from_core();
    let before = shell.thread_window.rows.len();
    shell.core.threads.get_mut(&key).unwrap().messages.insert(
        0,
        super_platinum_core::slack::models::Message {
            ts: Some("1719800399.000000".into()),
            text: Some("Prepended reply".into()),
            ..Default::default()
        },
    );
    shell.reproject_thread_window();
    assert_eq!(shell.thread_window.rows.len(), before + 1);
    assert_eq!(
        shell.thread_messages.len(),
        shell.thread_window.selected_indices().len()
    );
    for (message, &index) in shell
        .thread_messages
        .iter()
        .zip(shell.thread_window.selected_indices())
    {
        let row = &shell.thread_window.rows[index];
        assert_eq!(message.ts, row.ts);
        assert_eq!(
            shell.core.threads[&key].messages[row.source_index]
                .ts
                .as_deref(),
            Some(row.ts.as_str())
        );
    }
}
