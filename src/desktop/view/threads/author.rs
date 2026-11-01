//! Author navigation consumes its own original actor/member capability.
use super::*;
use super_platinum_core::slack::models::Message;

fn document_id<'a>(snapshot: &ShellState, message: &'a Message) -> Option<&'a str> {
    crate::thread_feed::document_author_id(snapshot.core.active_team.as_deref(), message)
}
/// Passive identity only. Names require a genuine current private human read;
/// a cached User cannot classify this editor. Profile/image authority is absent.
pub(super) fn document_label(
    snapshot: &ShellState,
    read: Option<&crate::thread_feed::Source>,
    item: &super_platinum_core::slack::models::ThreadViewItem,
    message: &Message,
) -> Option<String> {
    message.extra.get("document_comment")?;
    Some(match document_id(snapshot, message) {
        Some(id) => read
            .and_then(|r| {
                r.reading_document_name(snapshot, item, message)
                    .or_else(|| r.reading_member_name(snapshot, id))
            })
            .unwrap_or_else(|| id.into()),
        None => "Unknown author".into(),
    })
}
pub(super) fn render(
    state: Signal<ShellState>,
    snapshot: &ShellState,
    open: Option<crate::thread_feed::ProfileOpen>,
    fallback: &str,
) -> Element {
    let label = open
        .as_ref()
        .and_then(|s| s.user(snapshot))
        .map(|p| super_platinum_core::state::display_name(Some(p.user()), &p.user().id))
        .unwrap_or_else(|| fallback.into());
    rsx! {strong {if let Some(open)=open {
        button {r#type:"button",class:"thread-feed-author",id:open.control_id(),"data-thread-profile-author":"true",
            "aria-label":"View {label} profile",
            onclick:move |event:MouseEvent| {
                let m=event.modifiers();
                if !(m.ctrl()||m.meta()||m.alt()||m.shift()){open.start(state);}
            },"{label}"}
    }else{"{label}"}}}
}
