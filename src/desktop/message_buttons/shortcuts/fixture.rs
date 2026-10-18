//! Static review surfaces only. They cannot dispatch a shortcut or read Slack.
use super::*;
use serde_json::json;
pub(crate) fn install(s: &mut ShellState) {
    let team = s.core.active_team.clone().unwrap();
    let user = s.core.workspaces[&team].self_user_id.clone();
    let session = super_platinum_core::config::WorkspaceSession {
        team_id: team.clone(),
        user_id: user,
        enterprise_id: None,
        name: "Fixture workspace".into(),
        url: "https://example.test".into(),
        token: "synthetic-shortcut-token".into(),
    };
    s.core.session = Some(super_platinum_core::config::Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "synthetic".into(),
        workspaces: [(team.clone(), session)].into(),
    });
    s.core.transport = Some(Arc::new(
        super_platinum_core::slack::Transport::new("synthetic").unwrap(),
    ));
    s.overlay = None;
    s.app_modal = None;
    s.channel_tab = ChannelTab::Messages;
    let ctx = Arc::new(Context::capture(s).unwrap());
    let channel = s.core.active_channel.clone().unwrap();
    let channel_scope = channel_scope(s, &ctx, &channel).unwrap();
    let owner = Owner {
        channel_scope,
        ctx,
        channel,
        generation: s.channel_generation,
        thread: None,
        main: s.main_view,
        tab: s.channel_tab,
        rt: s.core.workspaces[&team].rt_generation,
        source: None,
        related: vec![team.clone()],
    };
    let catalog=domain::Catalog::decode(json!({"ok":true,"app_actions":[{"app_id":"A_REVIEW","app_name":"Release reviews","teams":[team],"actions":[
        {"action_id":"review","type":"global_action","name":"Review release","description":"Open the app’s release form"},
        {"action_id":"followup","type":"global_action","name":"Follow up with Équipe 東京","description":"Keep your message draft while the app opens"}]}]})).unwrap();
    s.message_buttons.shortcuts.dialog = Some(Dialog {
        fixture: true,
        calendar: None,
        id: super::super::next_id(),
        owner,
        kind: domain::Kind::Global,
        viewed: crate::app_modal::capture_shortcut_viewed(s),
        catalog: Some(catalog),
        query: String::new(),
        page: 0,
        selected: None,
        team: None,
        phase: Phase::Choosing,
        notice: Some("Offline preview — launching is disabled.".into()),
        token: None,
    });
    s.overlay = Some(Overlay::AppShortcuts);
}

pub(crate) fn calendar(s: &mut ShellState, variant: &str) {
    super::calendar::install_fixture(s, variant);
}
