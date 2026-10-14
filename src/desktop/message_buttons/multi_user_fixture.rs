//! Multi-user checking reuses the disconnected ordinary-user request recorder.
use crate::state::ShellState;
use serde_json::json;

pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_MESSAGE_MULTI_USER_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("message-multi-users-select" | "message-multi-users-select-narrow")
        )
}

pub(crate) fn install(shell: &mut ShellState) {
    super::user_fixture::install(shell);
    let message = shell
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
    message.blocks[0]["text"]["text"] = json!(
        "Choose reviewers. Selection stays private until Apply; Cancel restores the saved reviewers."
    );
    message.blocks[1]["accessory"] = json!({"type":"multi_users_select","action_id":"pick","initial_users":["U00000001"],"max_selected_items":2,"required":true,"placeholder":{"type":"plain_text","text":"Choose reviewers"}});
    message.blocks[2]["elements"][0] = json!({"type":"multi_users_select","action_id":"other","optional":true,"placeholder":{"type":"plain_text","text":"Choose follow-up reviewers"}});
    super::user_fixture::reproject(shell);
}

pub(crate) fn change_initial(mut state: dioxus::prelude::Signal<ShellState>) {
    use dioxus::prelude::*;
    if !enabled() {
        return;
    }
    let mut shell = state.write();
    if shell.core.transport.is_some() {
        return;
    }
    let message = shell
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
    message.blocks[1]["accessory"]["initial_users"] = json!(["U00000004"]);
    super::user_fixture::reproject(&mut shell);
    super::poll(&mut shell);
}
