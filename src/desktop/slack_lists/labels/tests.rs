use super::*;
use serde_json::json;
use super_platinum_core::slack::models::{User, UserGroup};

fn raw(value: Value) -> RtEvent {
    RtEvent::Unknown(serde_json::from_value(value).unwrap())
}
fn shell() -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    let workspace = shell.core.workspaces.get_mut("T1").unwrap();
    workspace.rt_generation = 7;
    workspace.users.insert(
        "LABEL_USER".into(),
        serde_json::from_value(json!({"id":"LABEL_USER","profile":{"display_name":"Before"}}))
            .unwrap(),
    );
    workspace.channels.insert(
        "LABEL_CHANNEL".into(),
        serde_json::from_value(json!({"id":"LABEL_CHANNEL","name":"before-channel"})).unwrap(),
    );
    workspace.usergroups.insert(
        "LABEL_GROUP".into(),
        serde_json::from_value(json!({"id":"LABEL_GROUP","handle":"before-group"})).unwrap(),
    );
    let mut loaded = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap();
    loaded.document.columns=Document::parse(json!({"id":"F_LIST_FIXTURE","mode":"list","list_metadata":{"schema":[{"id":"name","key":"name","name":"Item","type":"rich_text"},{"id":"users","key":"users","name":"Users","type":"user"}]}})).unwrap().columns;
    let rich = |id: &str, parent: Option<&str>, element: Value| json!({"id":id,"parent_record_id":parent,"fields":[{"key":"name","rich_text":[{"type":"rich_text_section","elements":[element]}]}]});
    loaded.records = RecordsPage::parse(json!({"ok":true,"records":[
        rich("parent",None,json!({"type":"text","text":"Unchanged parent"})),
        rich("user",Some("parent"),json!({"type":"user","user_id":"LABEL_USER"})),
        rich("channel",None,json!({"type":"channel","channel_id":"LABEL_CHANNEL"})),
        rich("group",None,json!({"type":"usergroup","usergroup_id":"LABEL_GROUP"})),
        {"id":"unknown","fields":[{"key":"users","user":["NEW_USER"]}]}
    ]}))
    .unwrap();
    shell.slack_lists.view.as_mut().unwrap().loaded = Some(loaded);
    super::super::rebuild_rows(&mut shell);
    let visit = super::super::generation(&shell).unwrap();
    assert!(super::super::toggle_expanded_shell(
        &mut shell, "parent", visit
    ));
    shell
}
fn row(shell: &ShellState, id: &str) -> Arc<presentation::RowView> {
    shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .all_rows
        .iter()
        .find(|row| row.id == id)
        .unwrap()
        .clone()
}

#[test]
fn status_only_and_stale_events_keep_all_projection_arcs() {
    let mut shell = shell();
    let before = shell.slack_lists.view.as_ref().unwrap().all_rows.clone();
    let user: User =
        serde_json::from_value(json!({"id":"LABEL_USER","profile":{"status_text":"Working"}}))
            .unwrap();
    let event = RtEvent::UserChanged(user.clone());
    assert!(prepare(&shell, "OTHER", 7, &event).is_none());
    assert!(prepare(&shell, "T1", 6, &event).is_none());
    let change = prepare(&shell, "T1", 7, &event).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .apply_user_update(user);
    assert!(!change.apply(&mut shell));
    assert!(Arc::ptr_eq(
        &before,
        &shell.slack_lists.view.as_ref().unwrap().all_rows
    ));
    let change = prepare(&shell, "T1", 7, &event).unwrap();
    super::super::close_shell(&mut shell);
    assert!(!change.apply(&mut shell));
}
#[test]
fn user_name_change_updates_hidden_children_and_preserves_other_rows_and_expansion() {
    let mut shell = shell();
    let parent = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .find(|row| row.id == "parent")
        .unwrap()
        .clone();
    let channel = row(&shell, "channel");
    let before = row(&shell, "user");
    let user: User =
        serde_json::from_value(json!({"id":"LABEL_USER","profile":{"display_name":"After"}}))
            .unwrap();
    let change = prepare(&shell, "T1", 7, &RtEvent::UserChanged(user.clone())).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .apply_user_update(user);
    assert!(change.apply(&mut shell));
    assert_eq!(row(&shell, "user").label, "@After");
    assert!(!Arc::ptr_eq(&before, &row(&shell, "user")));
    assert!(Arc::ptr_eq(&channel, &row(&shell, "channel")));
    let visible_parent = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .find(|row| row.id == "parent")
        .unwrap();
    assert!(visible_parent.expanded);
    assert!(Arc::ptr_eq(&parent, visible_parent));
    let visit = super::super::generation(&shell).unwrap();
    super::super::toggle_expanded_shell(&mut shell, "parent", visit);
    let user: User = serde_json::from_value(
        json!({"id":"LABEL_USER","profile":{"display_name":"Hidden rename"}}),
    )
    .unwrap();
    let change = prepare(&shell, "T1", 7, &RtEvent::UserChanged(user.clone())).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .apply_user_update(user);
    assert!(change.apply(&mut shell));
    assert_eq!(row(&shell, "user").label, "@Hidden rename");
    assert!(
        !shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .any(|row| row.id == "user")
    );
}
#[test]
fn channel_group_and_new_user_events_reconcile_names_without_list_reload() {
    let mut shell = shell();
    let event = raw(
        json!({"type":"channel_rename","channel":{"id":"LABEL_CHANNEL","name":"after-channel"},"event_ts":"1000.0"}),
    );
    let change = prepare(&shell, "T1", 7, &event).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .apply_channel_event(&event);
    assert!(change.apply(&mut shell));
    assert_eq!(row(&shell, "channel").label, "#after-channel");
    let group: UserGroup =
        serde_json::from_value(json!({"id":"LABEL_GROUP","handle":"after-group"})).unwrap();
    let event = raw(json!({"type":"subteam_updated","subteam":group}));
    let change = prepare(&shell, "T1", 7, &event).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .usergroups
        .insert("LABEL_GROUP".into(), group);
    assert!(change.apply(&mut shell));
    assert_eq!(row(&shell, "group").label, "@after-group");
    let user: User =
        serde_json::from_value(json!({"id":"NEW_USER","profile":{"display_name":"New name"}}))
            .unwrap();
    let change = prepare(&shell, "T1", 7, &RtEvent::UserChanged(user.clone())).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .apply_user_update(user);
    assert!(change.apply(&mut shell));
    assert_eq!(row(&shell, "unknown").cells[1].text, "@New name");
    assert!(
        matches!(&row(&shell,"unknown").cells[1].content,CellContent::Rich(nodes) if matches!(nodes.first(),Some(RichNode::UserMention{..})))
    );
}

#[test]
fn newly_resolved_dm_updates_dependencies_even_when_peer_names_match() {
    let mut shell = shell();
    let loaded = shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    loaded.document.columns[1].kind = "channel".into();
    loaded.records = RecordsPage::parse(
        json!({"ok":true,"records":[{"id":"dm","fields":[{"key":"users","channel":["NEW_DM"]}]}]}),
    )
    .unwrap();
    super::super::rebuild_rows(&mut shell);
    let event = raw(
        json!({"type":"channel_created","channel":{"id":"NEW_DM","is_im":true,"user":"LABEL_USER"}}),
    );
    let change = prepare(&shell, "T1", 7, &event).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .insert(
            "NEW_DM".into(),
            serde_json::from_value(json!({"id":"NEW_DM","is_im":true,"user":"LABEL_USER"}))
                .unwrap(),
        );
    assert!(change.apply(&mut shell));
    assert_eq!(row(&shell, "dm").cells[1].text, "#Before");
    let user: User = serde_json::from_value(
        json!({"id":"LABEL_USER","profile":{"display_name":"Renamed peer"}}),
    )
    .unwrap();
    let change = prepare(&shell, "T1", 7, &RtEvent::UserChanged(user.clone())).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .apply_user_update(user);
    assert!(change.apply(&mut shell));
    assert_eq!(row(&shell, "dm").cells[1].text, "#Renamed peer");
    let workspace = shell.core.workspaces.get_mut("T1").unwrap();
    workspace.users.insert(
        "OTHER_PEER".into(),
        serde_json::from_value(
            json!({"id":"OTHER_PEER","profile":{"display_name":"Renamed peer"}}),
        )
        .unwrap(),
    );
    let change = prepare(&shell, "T1", 7, &event).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .get_mut("NEW_DM")
        .unwrap()
        .user = Some("OTHER_PEER".into());
    assert!(change.apply(&mut shell));
    let old: User = serde_json::from_value(json!({"id":"LABEL_USER"})).unwrap();
    assert!(prepare(&shell, "T1", 7, &RtEvent::UserChanged(old)).is_none());
    let user: User =
        serde_json::from_value(json!({"id":"OTHER_PEER","profile":{"display_name":"Second peer"}}))
            .unwrap();
    let change = prepare(&shell, "T1", 7, &RtEvent::UserChanged(user.clone())).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .apply_user_update(user);
    assert!(change.apply(&mut shell));
    assert_eq!(row(&shell, "dm").cells[1].text, "#Second peer");
}

#[test]
fn grouped_reference_refresh_preserves_instances_and_selected_columns() {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    fixture::add_saved_views(&mut shell);
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
    fixture::edit_backend_for_test(&mut shell, |_, records| {
        let record = records["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|record| record["id"] == "R_LIST_002")
            .unwrap();
        record["fields"][0] = json!({"key":"NAME","rich_text":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"user","user_id":"GROUPED_NEW_USER"}]}]}]});
    });
    fixture::refresh(&mut shell);
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, generation) = (view.generation, view.layout.generation);
    assert!(layout::select_shell(
        &mut shell,
        Some("V_LIST_GROUPED".into()),
        visit,
        generation
    ));
    let before = presentation(&shell).unwrap();
    let instances = before
        .rows
        .iter()
        .filter(|row| row.id == "R_LIST_002")
        .map(|row| row.instance_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(instances.len(), 2);
    let user: User = serde_json::from_value(
        json!({"id":"GROUPED_NEW_USER","profile":{"display_name":"Resolved name"}}),
    )
    .unwrap();
    let change = prepare(&shell, "T1", 7, &RtEvent::UserChanged(user.clone())).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .apply_user_update(user);
    assert!(change.apply(&mut shell));
    let after = presentation(&shell).unwrap();
    assert_eq!(before.layout_generation, after.layout_generation);
    let duplicates = after
        .rows
        .iter()
        .filter(|row| row.id == "R_LIST_002")
        .collect::<Vec<_>>();
    assert_eq!(
        duplicates
            .iter()
            .map(|row| row.instance_id.clone())
            .collect::<Vec<_>>(),
        instances
    );
    for row in duplicates {
        assert_eq!(row.label, "@Resolved name");
        assert_eq!(
            row.cells
                .iter()
                .map(|cell| cell.column_id.as_str())
                .collect::<Vec<_>>(),
            vec!["C_NAME", "C_CHECK", "C_MULTI"]
        );
        assert!(after.entries.iter().any(
            |entry| matches!(entry, layout::TableEntry::Row(entry) if Arc::ptr_eq(entry,row))
        ));
    }
}

mod people;
