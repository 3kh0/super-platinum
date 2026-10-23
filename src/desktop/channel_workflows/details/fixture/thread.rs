//! Raw, offline history only. The checker opens Reply and edits the real field.
use super::*;
use super_platinum_core::{drafts::DraftKey, slack::models::Message, state::ChannelMessages};

pub(crate) const ROOT: &str = "1781524900.000001";
pub(crate) const REPLY: &str = "1781524901.000002";
pub(crate) const CHANNEL: &str = "CADDDEST01";
pub(crate) const DRAFT: &str = "Unsent workflow review \u{4e2d}\nKeep caf\u{e9} details \u{1f642}.";

pub(crate) fn requested() -> bool {
    std::env::var("SUPER_PLATINUM_WORKFLOW_DETAILS_THREAD_CHECK").as_deref() == Ok("1")
        && super::requested()
        && matches!(
            endpoint::case().as_str(),
            "first-details-success" | "first-details-errors" | "first-details-close-held"
        )
}
pub(crate) fn raw() -> [Value; 2] {
    [
        json!({"type":"message","ts":ROOT,"channel":CHANNEL,"user":"UWORKFLOW",
        "text":"Original workflow discussion.","thread_ts":ROOT,"reply_count":1,"latest_reply":REPLY}),
        json!({"type":"message","ts":REPLY,"channel":CHANNEL,"user":"UWORKFLOW",
        "text":"Keep this discussion while inspecting workflow details.","thread_ts":ROOT}),
    ]
}
pub(crate) fn install(s: &mut ShellState) -> Result<(), String> {
    if std::env::var_os("SUPER_PLATINUM_WORKFLOW_DETAILS_THREAD_CHECK").is_none() {
        return Ok(());
    }
    if !requested() || endpoint::capture(s).is_err() {
        return Err("Thread custody requires exact owned details fixture and flag=1".into());
    }
    let [parent, reply] = raw();
    let parent: Message =
        serde_json::from_value(parent).map_err(|_| "Original thread parent decode")?;
    let reply: Message =
        serde_json::from_value(reply).map_err(|_| "Original thread reply decode")?;
    let mut history = ChannelMessages::default();
    history.loaded = true;
    history.upsert(parent.clone());
    let mut thread = ChannelMessages::default();
    thread.loaded = true;
    thread.upsert(parent);
    thread.upsert(reply);
    let w = s
        .core
        .workspaces
        .get_mut("TWORKFLOW")
        .ok_or("Original thread workspace missing")?;
    if !w.channels.contains_key(CHANNEL) {
        return Err("Original thread destination missing".into());
    }
    w.messages.insert(CHANNEL.into(), history);
    s.core
        .threads
        .insert(("TWORKFLOW".into(), CHANNEL.into(), ROOT.into()), thread);
    Ok(())
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let key = DraftKey {
        team: "TWORKFLOW".into(),
        channel: CHANNEL.into(),
        thread: Some(ROOT.into()),
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
    json!({"root":ROOT,"activeRoot":s.thread_root,"threadGeneration":s.thread_generation,
        "scope":s.thread_scope(),"loading":s.thread_loading,
        "threadComposer":s.core.thread_composer,"draftKey":key,"activeDraftKey":s.core.drafts.thread,
        "storedDraft":s.core.drafts.items.get(&key),"attachments":s.core.thread_composer_attachments.len(),
        "historyLoaded":history.map(|b|b.loaded),"threadLoaded":thread.map(|b|b.loaded),
        "history":history.map(|b|&b.messages),"thread":thread.map(|b|&b.messages),
        "historyPending":history.map(|b|&b.pending),"threadPending":thread.map(|b|&b.pending),
        "renderedRows":s.thread_messages.iter().map(|m|json!({"id":m.id,"ts":m.ts,"pending":m.pending})).collect::<Vec<_>>()})
}
