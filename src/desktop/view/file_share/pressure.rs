//! Inline review of an earlier capture; current note component stays mounted.
use super::*;
use std::sync::Arc;
struct Shown {
    capture: file_share::pressure::Preview,
    text: String,
}
fn text_preview(shell: &ShellState, note: &[serde_json::Value]) -> String {
    let Some(workspace) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team))
    else {
        return String::new();
    };
    // Projection only: temporary registry is never connected to a fetcher or
    // installed into shell media. Exact typed input stays in the selected ticket.
    let media = crate::media::MediaRegistry::default();
    let ctx = crate::blocks::BlockCtx::new(workspace, &media);
    note.iter()
        .flat_map(|block| crate::blocks::block_nodes(ctx, block))
        .map(|node| node.plain_text())
        .collect::<Vec<_>>()
        .join("\n")
}
#[component]
pub(super) fn EarlierCapture(mut state: Signal<ShellState>, dialog_id: String) -> Element {
    let mut preview = use_signal(|| None::<Arc<Shown>>);
    let mut error = use_signal(|| None::<&'static str>);
    let snapshot = state.read();
    let available = file_share::has_capture(&snapshot, &dialog_id);
    let shown = preview
        .read()
        .as_ref()
        .filter(|p| file_share::pressure::current(&snapshot, &p.capture))
        .cloned();
    let open_id = dialog_id.clone();
    let discard_id = dialog_id.clone();
    let hide_id = dialog_id.clone();
    rsx! {
        if available {
            div {class:"file-share-earlier",id:"file-share-earlier-capture",
                button {id:"file-share-review-earlier",r#type:"button",onmousedown:move|event|event.prevent_default(),
                    onclick:move |_|match file_share::pressure::capture(&state.peek(),&open_id) {
                        Ok(value)=>{let text=text_preview(&state.peek(),&value.note);preview.set(Some(Arc::new(Shown{capture:value,text})));error.set(None);},Err(e)=>error.set(Some(e.message())),
                    },"Review earlier captured copy"}
                if let Some(shown)=shown {
                    div {class:"file-share-earlier-preview","data-capture-id":shown.capture.id(),
                        p {class:"file-share-hint","This is an earlier captured copy. Discarding it leaves your current message open."}
                        p {class:"file-share-hint",if let Some(destination)=&shown.capture.destination {"To {destination}"}else{"No conversation selected"}}
                        p {class:"file-share-hint","Earlier message (text preview)"}
                        if shown.text.is_empty() {p {class:"file-share-hint","No message text in this earlier copy"}}
                        else {div {id:"file-share-earlier-text",class:"file-share-earlier-text","{shown.text}"}}
                        div {class:"file-share-earlier-actions",
                            button {id:"file-share-discard-earlier",r#type:"button",onmousedown:move|event|event.prevent_default(),onclick:move |_|{
                                match file_share::pressure::discard(&mut state.write(),&shown.capture) {Ok(())=>{preview.set(None);error.set(None);frame("file-share-dialog",&discard_id,&["#file-share-note"],false);},Err(e)=>error.set(Some(e.message()))}
                            },"Discard this earlier copy"}
                            button {id:"file-share-hide-earlier",r#type:"button",onmousedown:move|event|event.prevent_default(),onclick:move |_|{preview.set(None);error.set(None);frame("file-share-dialog",&hide_id,&["#file-share-note"],false);},"Hide earlier copy"}
                        }
                    }
                }
                if let Some(message)=error() {p {class:"file-share-error",role:"alert","{message}"}}
            }
        }
    }
}
