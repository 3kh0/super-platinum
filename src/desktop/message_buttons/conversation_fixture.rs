//! Disconnected ordinary conversation controls; no discovery or action transport.
use crate::state::ShellState;
use serde_json::json;
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_MESSAGE_CONVERSATION_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("message-conversations-select" | "message-conversations-select-narrow")
        )
}
pub(crate) fn install(shell: &mut ShellState) {
    super::user_fixture::install(shell);
    let w = shell.core.workspaces.get_mut("T1").unwrap();
    for raw in [
        json!({"id":"C00000001","name":"general","is_channel":true,"is_member":false}),
        json!({"id":"G00000002","name":"private-project","is_private":true,"is_member":true}),
        json!({"id":"G00000003","name":"Review group","is_mpim":true,"members":["U00000001","U00000002","U00000003"]}),
        json!({"id":"C00000004","name":"external-shared","is_channel":true,"is_ext_shared":true}),
        json!({"id":"C00000005","name":"archived","is_channel":true,"is_archived":true}),
        json!({"id":"D00000006","is_im":true,"user":"U00000002"}),
    ] {
        let c: super_platinum_core::slack::models::Channel = serde_json::from_value(raw).unwrap();
        w.channels.insert(c.id.clone(), c);
    }
    let m = w
        .messages
        .get_mut("C2")
        .unwrap()
        .messages
        .iter_mut()
        .find(|m| m.ts.as_deref() == Some(super::user_fixture::MAIN_TS))
        .unwrap();
    m.blocks[0]["text"]["text"] =
        json!("Choose a conversation. Searching and cancelling do not submit or open a DM.");
    m.blocks[1]["text"]["text"] = json!("Destination");
    let a = &mut m.blocks[1]["accessory"];
    a["type"] = json!("conversations_select");
    a.as_object_mut().unwrap().remove("initial_user");
    a["initial_conversation"] = json!("C00000001");
    a["placeholder"]["text"] = json!("Choose conversation");
    a["confirm"]["title"]["text"] = json!("Change conversation?");
    a["confirm"]["text"]["text"] = json!("Apply this *conversation*?");
    a["confirm"]["confirm"]["text"] = json!("Change conversation");
    let b = &mut m.blocks[2]["elements"][0];
    b["type"] = json!("conversations_select");
    b["placeholder"]["text"] = json!("Choose follow-up conversation");
    b["default_to_current_conversation"] = json!(true);
    b["filter"] = json!({"include":["public","private","mpim","im"],"exclude_bot_users":true,"exclude_external_shared_channels":true});
    super::user_fixture::reproject(shell);
}
pub(crate) use super::user_fixture::{complete, navigate_away_back};
pub(crate) fn change_initial(mut state: dioxus::prelude::Signal<ShellState>) {
    use dioxus::prelude::*;
    if !enabled() {
        return;
    }
    let mut shell = state.write();
    if shell.core.transport.is_some() {
        return;
    }
    let m = shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages
        .iter_mut()
        .find(|m| m.ts.as_deref() == Some(super::user_fixture::MAIN_TS))
        .unwrap();
    m.blocks[1]["accessory"]["initial_conversation"] = json!("G00000003");
    super::user_fixture::reproject(&mut shell);
    super::poll(&mut shell);
}
