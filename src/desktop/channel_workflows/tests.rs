use super::*;

#[test]
fn configured_tab_is_distinct_from_explicit_destination_and_never_persisted() {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    s.channel_tab = ChannelTab::Messages;
    let team = s.core.active_team.clone().unwrap();
    let channel = s.core.active_channel.clone().unwrap();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .get_mut(&channel)
        .unwrap()
        .extra
        .insert("properties".into(), json!({"tabz":[]}));
    assert!(!available(&s));
    let raw = owner::channel_raw(&s).unwrap();
    s.channel_tab = ChannelTab::Workflows;
    assert!(available(&s));
    assert_eq!(owner::channel_raw(&s).unwrap(), raw);
    s.channel_tab = ChannelTab::Messages;
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .get_mut(&channel)
        .unwrap()
        .extra
        .insert(
            "properties".into(),
            json!({"tabz":[{"id":"workflows","type":"workflows"}]}),
        );
    assert!(available(&s));
}

#[test]
fn original_panel_callbacks_retire_on_tab_channel_generation_and_account() {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    s.channel_tab = ChannelTab::Workflows;
    let m = Mount::capture(&s).unwrap();
    assert!(m.current(&s));
    s.channel_tab = ChannelTab::Messages;
    assert!(!m.current(&s));
    s.channel_tab = ChannelTab::Workflows;
    s.channel_generation += 1;
    assert!(!m.current(&s));
    s.channel_generation -= 1;
    s.core.account_epoch += 1;
    assert!(!m.current(&s));
}

#[test]
fn private_names_and_actual_preview_metadata_never_use_bookmark_title() {
    let collections=Collections::capture(&json!({"id":"C1","properties":{"channel_workflows":[
        {"workflow_trigger_id":"FtPrivate","title":"Featured private label"}]}}),
        &json!({"ok":true,"bookmarks":[{"id":"Bk1","type":"shortcut","shortcut_id":"FtPrivate","title":"DO NOT DISPLAY"},
            {"id":"Bk2","type":"shortcut","shortcut_id":"FtReady","title":"OLD TITLE"}]})).unwrap();
    let previews=Previews::decode(collections.preview_ids(),json!({"ok":true,"triggers":[
        {"id":"FtPrivate","error":"access_denied"},
        {"id":"FtReady","name":"","workflow":{"title":"Actual title","description":"Actual description"}}]})).unwrap();
    let rows = previews.project(&collections).unwrap();
    assert_eq!(view::title(&rows.featured()[0]), "Featured private label");
    assert_eq!(view::title(&rows.bookmarked()[0]), "Private workflow");
    assert_eq!(
        view::description(&rows.bookmarked()[0]),
        "Only certain people can use this workflow"
    );
    assert_eq!(view::title(&rows.bookmarked()[1]), "Actual title");
    assert_eq!(
        view::description(&rows.bookmarked()[1]),
        "Actual description"
    );
}

#[test]
fn preview_icon_uses_actual_smallest_fitting_size_or_largest_and_no_bookmark() {
    let app = json!({"icons":{"image_24":"small","image_48":"fit","image_96":"large"}});
    assert_eq!(view::image_url(&app), Some("fit"));
    assert_eq!(
        view::image_url(&json!({"icons":{"image24":"small","image32":"largest"}})),
        Some("largest")
    );
    assert_eq!(
        view::image_url(&json!({"icon_url":"not-an-app-icon"})),
        None
    );
}

#[test]
fn workflow_root_preserves_async_startup_send_boundary() {
    fn assert_send<T: Send>() {}
    assert_send::<super::Root>();
}
