//! Opt-in raw conversation history and closed fresh-About responses.
//! The driver edits both real composers; this module never seeds draft state.
use super::*;
use super_platinum_core::{
    block_actions::{app_eligibility, home, home::about::Info},
    drafts::DraftKey,
    slack::models::Message,
    state::ChannelMessages,
};

pub(crate) const CHECK_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_DRAFT_CHECK";
pub(crate) const CHANNEL: &str = "CAPPHOME1";
pub(crate) const ROOT: &str = "1781524900.000001";
pub(crate) const REPLY: &str = "1781524901.000002";
pub(crate) const MAIN_DRAFT: &str =
    "Unsent channel review \u{4e2d}\nKeep caf\u{e9} details \u{1f642}.";
pub(crate) const THREAD_DRAFT: &str =
    "Unsent thread review \u{4e2d}\nKeep caf\u{e9} replies \u{1f642}.";

#[derive(Default)]
pub(super) struct Data {
    seed: Option<Value>,
    about: Vec<Value>,
}
impl Data {
    pub(super) fn snapshot(&self) -> Value {
        json!({"seed":self.seed,"about":self.about,"networkCalls":0})
    }
}
pub(crate) fn requested() -> bool {
    std::env::var(CHECK_ENV).as_deref() == Ok("1") && super::requested()
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    requested()
        && super::capture(s).is_ok_and(|m| {
            m.0.lock()
                .unwrap()
                .drafts
                .as_ref()
                .is_some_and(|d| d.seed.is_some())
        })
}
pub(crate) fn raw() -> [Value; 2] {
    [
        json!({"type":"message","ts":ROOT,"channel":CHANNEL,"user":"UWORKFLOW",
            "text":"Original workflow Share discussion.","thread_ts":ROOT,
            "reply_count":1,"latest_reply":REPLY}),
        json!({"type":"message","ts":REPLY,"channel":CHANNEL,"user":"UWORKFLOW",
            "text":"Keep this discussion while sharing the workflow.","thread_ts":ROOT}),
    ]
}
pub(crate) fn seed(s: &mut ShellState) -> Result<(), String> {
    if !requested() {
        return Err("Share draft seed requires exact opt-in fixture".into());
    }
    let model = super::capture(s)?;
    let (_, session) = super::context(s).map_err(|_| "Original Share session absent")?;
    if s.core.active_channel.as_deref() != Some(CHANNEL) {
        return Err("Original Share app conversation absent".into());
    }
    let mut m = model.0.lock().unwrap();
    let d = m
        .drafts
        .as_mut()
        .ok_or("Original Share draft model absent")?;
    if d.seed.is_some() {
        return Err("Share draft history seed must be original".into());
    }
    let rows = raw();
    let parent: Message =
        serde_json::from_value(rows[0].clone()).map_err(|_| "Original Share parent decode")?;
    let reply: Message =
        serde_json::from_value(rows[1].clone()).map_err(|_| "Original Share reply decode")?;
    let w = s
        .core
        .workspaces
        .get_mut("TWORKFLOW")
        .ok_or("Original workspace absent")?;
    if !w.channels.contains_key(CHANNEL) {
        return Err("Original Share app channel absent".into());
    }
    let mut history = ChannelMessages::default();
    history.loaded = true;
    history.upsert(parent.clone());
    let mut thread = ChannelMessages::default();
    thread.loaded = true;
    thread.upsert(parent);
    thread.upsert(reply);
    w.messages.insert(CHANNEL.into(), history);
    s.core
        .threads
        .insert(("TWORKFLOW".into(), CHANNEL.into(), ROOT.into()), thread);
    d.seed = Some(json!({"owner":super::owner(s,&session),"channel":CHANNEL,
        "root":ROOT,"reply":REPLY,"raw":rows,"channelGeneration":s.channel_generation,
        "composerStateSeeded":false}));
    drop(m);
    s.refresh_from_core();
    Ok(())
}

pub(crate) fn about_response(s: &ShellState, request: &PreparedRequest) -> Result<Value, Error> {
    if !enabled(s)
        || s.core.active_channel.as_deref() != Some(CHANNEL)
        || s.channel_tab != ChannelTab::AppAbout
    {
        return Err(closed());
    }
    let model = super::capture(s).map_err(|_| closed())?;
    let (client, session) = super::context(s)?;
    let channel = s
        .core
        .workspaces
        .get("TWORKFLOW")
        .and_then(|w| w.channels.get(CHANNEL))
        .ok_or_else(closed)?;
    let (method, _) = crate::app_home::about::management::fixture::decode(request)?;
    let (expected, raw) = match method {
        "conversations.info" => (
            home::fetch(&client, &session, CHANNEL)?,
            json!({"ok":true,"channel":channel,
                "app_home":{"app_id":"A_HOME","app_installed_team_id":"TWORKFLOW",
                "home_tab_enabled":true,"messages_tab_enabled":true,"home_view_id":"V_HOME"}}),
        ),
        "apps.about.info" => (
            Info::request(&client, &session, "A_HOME")?,
            json!({"ok":true,"app_id":"A_HOME","name":"Review requests","owner_ids":["UWORKFLOW"]}),
        ),
        "apps.profile.get" => (
            app_eligibility::profile_request(&client, &session, "A_HOME")?,
            json!({"ok":true,"app_profile":{"id":"A_HOME","team_id":"TWORKFLOW",
                "is_installed":true,"is_workflow_app":true}}),
        ),
        "experiments.getByUser" => (
            api::list_experiments(&client, &session),
            json!({"ok":true,"assignments":{"agent_about_page":{"group":"on"},
                "flexpane_content_parity":{"group":"on"}}}),
        ),
        _ => return Err(closed()),
    };
    super::super::super::picker::same_request(request, &expected)?;
    let mut m = model.0.lock().unwrap();
    let d = m.drafts.as_mut().ok_or_else(closed)?;
    if d.about.len() >= 12 {
        return Err(closed());
    }
    d.about.push(json!({"serial":d.about.len()+1,"method":method,
        "body":super::safe_body(&request.body),"raw":raw,
        "endpoint":request.url.split('?').next(),"retrySafe":request.retry_safe(),
        "owner":super::owner(s,&session),"channel":CHANNEL,"channelGeneration":s.channel_generation}));
    Ok(raw)
}

pub(crate) fn snapshot(s: &ShellState) -> Value {
    let main = DraftKey {
        team: "TWORKFLOW".into(),
        channel: CHANNEL.into(),
        thread: None,
    };
    let thread_key = DraftKey {
        thread: Some(ROOT.into()),
        ..main.clone()
    };
    let history = s
        .core
        .workspaces
        .get("TWORKFLOW")
        .and_then(|w| w.messages.get(CHANNEL));
    let thread = s
        .core
        .threads
        .get(&("TWORKFLOW".into(), CHANNEL.into(), ROOT.into()));
    json!({"enabled":enabled(s),"channel":CHANNEL,"root":ROOT,"reply":REPLY,
        "activeRoot":s.thread_root,"threadGeneration":s.thread_generation,
        "scope":s.thread_scope(),"loading":s.thread_loading,
        "composer":s.core.composer,"threadComposer":s.core.thread_composer,
        "mainDraftKey":main,"threadDraftKey":thread_key,
        "storedMainDraft":s.core.drafts.items.get(&main),"storedThreadDraft":s.core.drafts.items.get(&thread_key),
        "mainAttachments":s.core.composer_attachments.len(),"threadAttachments":s.core.thread_composer_attachments.len(),
        "historyLoaded":history.map(|b|b.loaded),"threadLoaded":thread.map(|b|b.loaded),
        "history":history.map(|b|&b.messages),"thread":thread.map(|b|&b.messages),
        "historyPending":history.map(|b|&b.pending),"threadPending":thread.map(|b|&b.pending)})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_history_has_one_parent_and_correlated_reply() {
        let [parent, reply] = raw();
        let p: Message = serde_json::from_value(parent.clone()).unwrap();
        let r: Message = serde_json::from_value(reply.clone()).unwrap();
        let mut history = ChannelMessages::default();
        history.upsert(p.clone());
        let mut thread = ChannelMessages::default();
        thread.upsert(p);
        thread.upsert(r);
        assert_eq!(history.messages.len(), 1);
        assert_eq!(thread.messages.len(), 2);
        assert_eq!(parent["latest_reply"], reply["ts"]);
        assert_eq!(parent["ts"], reply["thread_ts"]);
        assert_eq!(parent["channel"], CHANNEL);
        assert_eq!(reply["channel"], CHANNEL);
    }
}
