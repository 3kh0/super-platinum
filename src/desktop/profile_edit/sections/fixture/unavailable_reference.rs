//! Finite files.info error carriers through the production reference reader.
use super::*;
use serde_json::json;
use std::sync::{Mutex, OnceLock};
pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_CANVAS_UNAVAILABLE_CHECK";
pub(crate) const CASE: &str = "SUPER_PLATINUM_PROFILE_CANVAS_UNAVAILABLE_CASE";
pub(crate) const PRIVATE: &str = "Private canvas";
pub(crate) const RECOVERED: &str = "Recovered canvas";
#[derive(Clone, Copy, Debug)]
pub(crate) enum Reply {
    Private,
    Normal,
    Malformed,
}
fn selected() -> &'static Mutex<Reply> {
    static REPLY: OnceLock<Mutex<Reply>> = OnceLock::new();
    REPLY.get_or_init(|| Mutex::new(Reply::Private))
}
pub(crate) fn case() -> String {
    std::env::var(CASE).unwrap_or_default()
}
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && mentions::named()
        && [FLAG, mentions::FLAG, files::FLAG, files::BLOCKS_FLAG]
            .iter()
            .all(|f| std::env::var(f).as_deref() == Ok("1"))
        && std::env::var("SUPER_PLATINUM_PROFILE_FILE_TITLES_KIND").as_deref() != Ok("list")
        && matches!(
            case().as_str(),
            "template" | "sticky" | "failure" | "cancel" | "retirement"
        )
        && [
            deleted_reference::FLAG,
            group_title::FLAG,
            missing_group::FLAG,
            channel_title::FLAG,
            missing_channel::FLAG,
            user_title::FLAG,
            emoji_title::FLAG,
            link_title::FLAG,
            empty_canvas::FLAG,
            completion::FLAG,
            group_recovery::FLAG,
        ]
        .iter()
        .all(|f| std::env::var(f).as_deref() != Ok("1"))
}
pub(crate) fn reply(next: Reply) {
    *selected().lock().unwrap() = next;
}
pub(crate) fn response() -> Value {
    match *selected().lock().unwrap() {
        Reply::Private if case() == "sticky" => {
            json!({"ok":false,"error":"not_visible","filetype":"quip","title":"SECRET ERROR TITLE"})
        }
        Reply::Private => json!({"ok":false,"error":"template_not_visible"}),
        Reply::Normal => {
            json!({"ok":true,"file":{"id":files::ID,"mode":"quip","filetype":"quip","title":RECOVERED,"updated":102}})
        }
        Reply::Malformed => json!({"ok":false,"error":7}),
    }
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let team = shell.core.active_team.as_deref().unwrap_or_default();
    json!({"case":case(),"selectedResponse":response(),"files":files::status(),
        "account":shell.core.active_account,"epoch":shell.core.account_epoch,
        "socketGeneration":shell.core.workspaces.get(team).map(|w|w.rt_generation),
        "networkCalls":0,"presentation":{"kind":"private-canvas","icon":"lock","disabled":false}})
}

pub(crate) fn retire_socket(mut state: Signal<ShellState>) -> Result<Value, String> {
    if !named() {
        return Err("Closed reference fixture required".into());
    }
    let mut shell = state.write();
    let team = shell.core.active_team.clone().ok_or("Missing team")?;
    let workspace = shell
        .core
        .workspaces
        .get_mut(&team)
        .ok_or("Missing workspace")?;
    let before = workspace.rt_generation;
    workspace.rt_generation += 1;
    Ok(json!({"before":before,"after":workspace.rt_generation}))
}
