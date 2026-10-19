//! The editor belongs to the acquired List item, independently of the timeline.
use super::*;
use crate::slack_lists::comments::{draft, reply};

pub(super) fn panel(state: Signal<ShellState>) -> Element {
    let Some(view) = draft::presentation(&state.read()) else {
        return rsx! {};
    };
    rsx! {Composer {key:"list-comment-draft-{view.binding.generation()}",state}}
}

#[component]
fn Composer(state: Signal<ShellState>) -> Element {
    let Some(view) = draft::presentation(&state.read()) else {
        return rsx! {};
    };
    let binding = view.binding.clone();
    let generation = binding.generation();
    let scope = binding.scope().to_owned();
    let disabled = view.busy;
    let retry = view.capture_failed || view.error.is_some();
    let labels = {
        let shell = state.read();
        let blocks =
            serde_json::from_str::<Vec<serde_json::Value>>(&view.blocks).unwrap_or_default();
        shell
            .core
            .active_team
            .as_ref()
            .and_then(|team| shell.core.workspaces.get(team))
            .map(|workspace| crate::view::rich_editor::mention_labels(workspace, &blocks))
            .unwrap_or_else(|| "{}".into())
    };
    rsx! {
        section {id:"slack-list-comment-compose",class:"slack-list-comment-compose","aria-label":"Write a comment",
            "data-list-comments-compose-scope":"{scope}","data-editor-generation":generation,
            if view.loading {p {class:"slack-list-comment-compose-status",role:"status","Loading saved draft…"}}
            else if view.ready {
                div {key:"list-comment-editor-{generation}",class:"slack-list-comment-compose-editor",
                    onmounted:{let binding=binding.clone();move |_|bridge::mount(state,binding.clone())},
                    crate::view::rich_editor::RichEditor {id:"slack-list-comment-draft",label:"Comment".to_owned(),
                        blocks:view.blocks,labels,disabled,autofocus:true,expected_generation:Some(generation)}
                }
            }
            if let Some(status)=view.status {p {class:"slack-list-comment-compose-status",role:"status","{status}"}}
            if let Some(error)=view.error {p {class:"slack-list-comment-compose-status slack-list-error",role:"alert","{error}"}}
            div {class:"slack-list-comment-compose-actions",
                if retry {button {id:"slack-list-comment-retry",r#type:"button",class:"slack-list-action",disabled:view.busy,
                    onclick:{let binding=binding.clone();move |_|draft::retry(state,&binding)},"Retry local save"}}
                button {id:"slack-list-comment-send",r#type:"button",class:"slack-list-action",disabled:!view.can_send||disabled||view.locked||view.loading,
                    onclick:{let binding=binding.clone();move |_|reply::send(state,binding.clone())},
                    "Send comment"
                }
            }
        }
    }
}
