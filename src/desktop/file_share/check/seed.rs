//! Named disconnected startup seed. Capability filesystem checks run only here.
use super::*;
use crate::state::{ChannelTab, MainView, WorkspaceVm};
use super_platinum_core::{
    config::WorkspaceSession,
    state::{ChannelMessages, Workspace},
};
pub(super) fn prepare(shell: &mut ShellState) {
    shell.core.session = None;
    shell.core.transport = None;
    shell.core.active_account = Some(fixture::ACCOUNT.into());
    shell.core.active_team = Some(fixture::TEAM.into());
    shell.core.active_channel = Some(fixture::DESTINATION.into());
    let metadata = WorkspaceSession {
        team_id: fixture::TEAM.into(),
        user_id: fixture::USER.into(),
        enterprise_id: None,
        name: "Share fixture".into(),
        url: "https://file-share-fixture.invalid".into(),
        token: String::new(),
    };
    let mut workspace = Workspace::from_session(&metadata);
    let channel = serde_json::from_value::<super_platinum_core::slack::models::Channel>(
        fixture::destination(),
    )
    .unwrap();
    workspace.channels.insert(channel.id.clone(), channel);
    workspace.last_active_channel = Some(fixture::DESTINATION.into());
    for (id, name) in [(fixture::USER, "You"), ("U00000002", "File author")] {
        let user = serde_json::from_value(
            json!({"id":id,"name":name,"real_name":name,"profile":{"display_name":name}}),
        )
        .unwrap();
        workspace.users.insert(id.into(), user);
    }
    let mut bag = ChannelMessages::default();
    bag.upsert(serde_json::from_value(json!({"ts":"1719800400.000000","channel":fixture::DESTINATION,"user":"U00000002","text":"File for review","files":[fixture::source_file()]})).unwrap());
    workspace.messages.insert(fixture::DESTINATION.into(), bag);
    shell.core.workspaces.clear();
    shell
        .core
        .workspaces
        .insert(fixture::TEAM.into(), workspace);
    shell.workspaces = vec![WorkspaceVm {
        id: fixture::TEAM.into(),
        name: "Share fixture".into(),
        initials: "S".into(),
    }];
    shell.main_view = MainView::Home;
    shell.channel_tab = ChannelTab::Messages;
    shell.overlay = None;
    shell.viewer = None;
    shell.thread_root = None;
    shell.thread_messages.clear();
    shell.surfaces.clear();
    shell.file_share = Default::default();
    shell.file_share_fixture = None;
    shell.core.files = Default::default();
    shell
        .core
        .files
        .ensure_scope(shell.core.account_epoch, fixture::TEAM);
    let generation = shell.core.files.begin(Default::default(), 1);
    shell
        .core
        .files
        .apply(generation, super_platinum_core::files::FilesPage::default());
    shell.refresh_from_core();
    shell.sync_fixture_messages();
}
pub(crate) fn install(shell: &mut ShellState) {
    prepare(shell);
    match fixture::Capability::load(shell) {
        Ok(capability) => shell.file_share_fixture = Some(capability),
        Err(_) => eprintln!(
            "file-share-check: {{\"ok\":false,\"error\":\"Fixture startup ownership rejected\"}}"
        ),
    }
}
pub(super) fn mutate_source(shell: &mut ShellState, remove: bool) -> Result<(), String> {
    let workspace = shell
        .core
        .workspaces
        .get_mut(fixture::TEAM)
        .ok_or("Fixture workspace missing")?;
    let message = workspace
        .messages
        .get_mut(fixture::DESTINATION)
        .and_then(|b| b.messages.first_mut())
        .ok_or("Fixture source missing")?;
    let file = message
        .files
        .iter_mut()
        .find(|f| f.id.as_deref() == Some(fixture::FILE))
        .ok_or("Fixture file missing")?;
    if remove {
        message.files.clear();
    } else {
        file.title = Some("Changed fixture release notes".into());
    }
    shell.refresh_from_core();
    shell.sync_fixture_messages();
    Ok(())
}
