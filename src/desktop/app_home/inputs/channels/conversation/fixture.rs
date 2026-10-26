//! Closed Home conversation fixture; actual shared aggregate requests only.
use super::super::fixture as shared;
use super::*;
use shared::capture;
use shared::{Reply, configure, key, pending_closed, release, snapshot};
pub(crate) mod check;
pub(crate) mod multiple;
pub(crate) const ALPHA: &str = shared::ALPHA;
pub(crate) const BETA: &str = shared::BETA;
pub(crate) const PRIVATE: &str = "GCONVPRV1";
pub(crate) const MPIM: &str = "GCONVMP01";
pub(crate) const REMOTE: &str = "GCONVRMT1";
pub(crate) const ADA: &str = "UCONVADA1";
pub(crate) const BLAIR: &str = "UCONVBLR1";
pub(crate) const REMOTE_USER: &str = "UCONVRMT1";
pub(crate) fn requested() -> bool {
    if crate::app_home::inputs::users::dispatch::fixture::conversation_requested() {
        return true;
    }
    if multiple::requested() {
        return true;
    }
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-conversation-select" | "app-home-conversation-select-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && !multiple::requested()
        && !crate::app_home::inputs::users::dispatch::fixture::conversation_requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_CONVERSATION_SELECT_CHECK").as_deref() == Ok("1")
        && matches!(
            case().as_str(),
            "selection" | "props" | "filter" | "recovery"
        )
}
pub(crate) fn case() -> String {
    if crate::app_home::inputs::users::dispatch::fixture::conversation_requested() {
        return crate::app_home::inputs::users::dispatch::fixture::case();
    }
    if multiple::requested() {
        return multiple::case();
    }
    std::env::var("SUPER_PLATINUM_APP_HOME_CONVERSATION_SELECT_CASE").unwrap_or_default()
}
pub(crate) fn field(block: &mut Value) {
    if multiple::requested() {
        multiple::field(block);
        return;
    }
    block["optional"] = json!(true);
    block["label"]["text"] = json!("Release conversation");
    block["element"] = json!({"type":"conversations_select","action_id":"value","default_to_current_conversation":true,"response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Find a conversation"}});
    if case() != "selection" {
        block["element"]["initial_conversation"] = json!(ALPHA);
    }
    if case() == "filter" {
        block["element"]["filter"] =
            json!({"only_from_team_id":"TWORK0002","exclude_bot_users":true});
    }
}
pub(crate) fn members() -> Vec<Value> {
    vec![
        json!({"id":ADA,"name":"ada","team_id":"T1","profile":{"display_name":"Ada"}}),
        json!({"id":BLAIR,"name":"blair","team_id":"TWORK0002","is_external":true,"profile":{"display_name":"Blair"}}),
    ]
}
pub(crate) fn channels() -> Vec<Value> {
    vec![
        json!({"id":ALPHA,"name":"alpha","is_channel":true,"is_member":true,"connected_team_ids":["TWORK0002"]}),
        json!({"id":BETA,"name":"beta","is_channel":true,"is_member":true,"connected_team_ids":["TWORK0003"]}),
        json!({"id":PRIVATE,"name":"private-release","is_group":true,"is_private":true,"is_member":true,"connected_team_ids":["TWORK0002"]}),
        json!({"id":"CCONVARC1","name":"archived-release","is_channel":true,"is_archived":true,"connected_team_ids":["TWORK0002"]}),
        json!({"id":"CCONVEXT1","name":"external-release","is_channel":true,"is_ext_shared":true,"connected_team_ids":["TWORK0002"]}),
        json!({"id":MPIM,"name":"mpdm-ada-blair","is_mpim":true,"is_group":true,"is_private":true,"is_member":true,"members":["UHOMESELF",ADA,BLAIR]}),
    ]
}
pub(crate) fn response() -> Value {
    json!({"users":{"ok":true,"results":[{"id":REMOTE_USER,"name":"remote-reviewer","team_id":"TWORK0002","is_external":true,"profile":{"display_name":"Remote reviewer"},"opaque":{"remote":true}}]},"channels":{"ok":true,"results":channels()},"mpims":{"ok":true,"mpims":[{"id":REMOTE,"name":"mpdm-remote-review","members":["UHOMESELF",ADA,BLAIR],"details":{"is_mpim":true,"opaque":{"remote":true}}}]}})
}
pub(super) fn request(
    s: &mut ShellState,
    t: &Ticket,
    r: &directory::aggregate::Requests,
) -> Option<tokio::sync::oneshot::Receiver<Result<Value, Error>>> {
    shared::request_conversations(s, t, r)
}

pub(crate) fn refresh(
    s: &mut ShellState,
    optional: bool,
    initial: Option<Option<&str>>,
    saved: Option<Option<&str>>,
) -> Result<Value, String> {
    let h = s.app_home.surface.as_ref().ok_or("Missing Home")?;
    let (team, rt, before) = (h.owner.ctx.session.team_id.clone(), h.owner.rt, h.revision);
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["hash"] = json!(format!("conversation-refresh-{before}"));
    let block = raw["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["block_id"] == "home-choice")
        .unwrap();
    block["optional"] = json!(optional);
    if let Some(initial) = initial {
        if let Some(id) = initial {
            block["element"]["initial_conversation"] = json!(id)
        } else {
            block["element"]
                .as_object_mut()
                .unwrap()
                .remove("initial_conversation");
        }
    }
    if let Some(saved) = saved {
        raw["state"] = json!({"values":{"home-choice":{"value":{"type":"conversations_select","selected_conversation":saved,"response_url_enabled":true}}}});
    }
    publish_view(s, team, rt, before, raw)
}
fn publish_view(
    s: &mut ShellState,
    team: String,
    rt: u64,
    before: u64,
    raw: Value,
) -> Result<Value, String> {
    let event = RtEvent::Unknown(
        serde_json::from_value(
            json!({"type":"view_updated","app_id":"A_HOME","view_id":"V_HOME","view":raw.clone()}),
        )
        .unwrap(),
    );
    crate::app_home::realtime(s, &team, rt, &event);
    if !s.app_home.surface.as_ref().is_some_and(|h| {
        h.owner.current(s)
            && h.revision != before
            && h.view.as_ref().is_some_and(|v| v.raw() == &raw)
    }) {
        return Err("Owned conversation refresh refused".into());
    }
    Ok(
        json!({"beforeRevision":before,"afterRevision":s.app_home.surface.as_ref().unwrap().revision,"raw":raw}),
    )
}
pub(crate) fn filter_refresh(s: &mut ShellState) -> Result<Value, String> {
    let h = s.app_home.surface.as_ref().ok_or("Missing Home")?;
    let (team, rt, before) = (h.owner.ctx.session.team_id.clone(), h.owner.rt, h.revision);
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["hash"] = json!(format!("conversation-filter-{before}"));
    let block = raw["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["block_id"] == "home-choice")
        .unwrap();
    block["element"]["filter"] =
        json!({"include":["private","mpim"],"exclude_external_shared_channels":true});
    publish_view(s, team, rt, before, raw)
}
