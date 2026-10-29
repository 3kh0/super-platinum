//! Synthetic cold-list states; registration is not native acceptance.
use super::*;
use serde_json::json;
use super_platinum_core::config::Session;
pub(crate) fn install(shell: &mut ShellState) {
    #[cfg(feature = "protected-native-fixture")]
    let transport = Arc::new(Transport::disconnected_fixture("synthetic"));
    #[cfg(not(feature = "protected-native-fixture"))]
    let transport = Arc::new(Transport::new("synthetic").unwrap());
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: shell.core.workspaces["T1"].self_user_id.clone(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
        enterprise_id: None,
    };
    shell.core.active_account = Some("dm-cold-row-fixture".into());
    shell.core.transport = Some(transport);
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: Default::default(),
        workspaces: [("T1".into(), session)].into(),
    });
    shell.main_view = crate::state::MainView::Dms;
    shell.overlay = None;
    shell.core.dms.entries=serde_json::from_value(json!([
        {"id":"D_COLD_CACHED","latest":"1719801000.000400","channel":{"id":"D_COLD_CACHED","is_im":true,"is_open":false,"user":"U2"},"message":{"text":"A cached conversation outside the channel projection","ts":"1719801000.000400"}},
        {"id":"D_COLD_UNKNOWN","latest":"1719800900.000300","channel":{"id":"D_COLD_UNKNOWN","is_im":true,"is_open":false,"user":"U3"},"message":{"text":"Read-only metadata is needed before opening","ts":"1719800900.000300"}}
    ])).unwrap();
    let channel = shell.core.dms.entries[0].channel.clone().unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .insert(channel.id.clone(), channel);
    shell.core.dms.loaded = true;
    shell.core.dms.loading = false;
    shell.core.dms.next_cursor = None;
    shell
        .channels
        .retain(|c| c.id != "D_COLD_CACHED" && c.id != "D_COLD_UNKNOWN");
}

pub(crate) fn install_paging(shell: &mut ShellState, name: &str) {
    install(shell);
    shell.core.dms.next_cursor = Some("fixture-older-page".into());
    shell.set_dm_query("Not among the loaded conversations".into());
    if name.contains("loading") {
        shell.core.dms.loading = true;
    }
    if name.contains("empty") {
        shell.core.dms.entries.clear();
        shell.core.dms.next_cursor = None;
        shell.set_dm_query(String::new());
    }
}

pub(crate) fn install_initial(shell: &mut ShellState, name: &str) {
    install(shell);
    shell.core.dms.entries.clear();
    shell.core.dms.loaded = false;
    shell.core.dms.loading = name.contains("loading");
    shell.core.dms.next_cursor = None;
    shell.set_dm_query(String::new());
}

pub(crate) fn install_facts(shell: &mut ShellState) {
    install(shell);
    shell.core.dms.loaded = true;
    shell.core.dms.loading = false;
    shell.core.dms.next_cursor = None;
    shell.set_dm_query(String::new());
    shell.set_dm_unread_only(true);
    shell.core.dms.entries=serde_json::from_value(json!([
        {"id":"D_UNREAD_FACTS","latest":"1719801000.000400","channel":{"id":"D_UNREAD_FACTS","is_im":true,"is_open":false,"user":"U2","unread_count":0},"message":{"text":"Cached unread state while metadata is cold","ts":"1719801000.000400"}},
        {"id":"G_NAME_FACTS","latest":"1719800900.000300","channel":{"id":"G_NAME_FACTS","is_mpim":true,"is_group":true,"name":"Before rename","unread_count":0},"message":{"text":"Current name comes from workspace metadata","ts":"1719800900.000300"}}
    ])).unwrap();
    let ws = shell.core.workspaces.get_mut("T1").unwrap();
    ws.channels.remove("D_UNREAD_FACTS");
    let bag = ws.messages.entry("D_UNREAD_FACTS".into()).or_default();
    bag.unread_count = 3;
    bag.mention_count = 1;
    bag.last_read = Some("8.0".into());
    ws.channels.insert("G_NAME_FACTS".into(),serde_json::from_value(json!({"id":"G_NAME_FACTS","is_mpim":true,"is_group":true,"name":"Release Planning","unread_count":0})).unwrap());
}
