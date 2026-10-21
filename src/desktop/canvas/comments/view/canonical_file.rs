//! One original-row link. The href remains exact display evidence; its native
//! activation always goes through captured source custody, never the OS handler.
use super::*;

pub(super) fn content(
    state: Signal<ShellState>,
    model: Signal<State>,
    message: &Message,
    body: &crate::model::MessageVm,
    epoch: u64,
) -> Element {
    let token = {
        let shell = state.read();
        let comments = model.read();
        super::super::file_source::token(&shell, &comments, message)
    };
    if let Some(token) = token {
        let name = token.name().to_owned();
        let url = token.url().to_owned();
        let id = token.id().to_owned();
        let ts = token.message_ts().to_owned();
        let block_id = token.block_id().to_owned();
        return rsx! {
            div {class:"message-body","data-canvas-file-block":block_id,
                a {href:url.clone(),"data-canvas-file-id":id,
                    "data-canvas-file-root":ts,"data-canvas-file-name":name.clone(),
                    "data-canvas-file-url":url,
                    onclick:move |event| {
                        event.prevent_default();
                        event.stop_propagation();
                        super::super::file_navigation::open(state,model,token.clone());
                    },
                    "{name}"
                }
            }
        };
    }
    // A cold/stale/ineligible first file does not select a later file. Defer
    // fallback projection until this point, so an eligible override never
    // traverses original superseded blocks or registers their media.
    if !message.files.is_empty() {
        let fallback = {
            let shell = state.read();
            let comments = model.read();
            comments
                .owner
                .as_ref()
                .and_then(|owner| shell.core.workspaces.get(&owner.scope.team))
                .map(|workspace| {
                    super::super::projection::message_vm(workspace, message, &shell.media)
                })
        };
        if let Some(fallback) = fallback {
            return rsx! {
                {super::content(&fallback, state, epoch)}
                {acquisition(state, model, message)}
            };
        }
        return rsx! {div {class:"message-body"}};
    }
    super::content(body, state, epoch)
}

fn acquisition(state: Signal<ShellState>, model: Signal<State>, message: &Message) -> Element {
    let presentation = {
        let shell = state.read();
        let comments = model.read();
        super::super::file_acquisition::presentation(&shell, &comments, message)
    };
    let Some(presentation) = presentation else {
        return rsx! {};
    };
    let ts = message.ts.clone().unwrap_or_default();
    rsx! {
        div {"data-canvas-file-acquisition":ts.clone(),"data-phase":presentation.phase,
            "aria-busy":matches!(presentation.phase,"queued"|"loading"),
            p {class:"canvas-comments-note",role:"status","{presentation.notice}"}
            if let Some(retry)=presentation.retry {
                button {class:"canvas-file-retry","data-canvas-file-retry":ts.clone(),disabled:presentation.disabled,
                    onclick:move |_|super::super::file_acquisition::retry(state,model,retry.clone()),
                    "Retry file details"
                }
            }
        }
    }
}
