//! The editor belongs to the acquired List item, independently of the timeline.
use super::*;
use crate::slack_lists::comments::{draft, reply};
#[path = "completion.rs"]
mod completion;

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
    let editing = crate::slack_lists::comments::own_edit::presentation(&state.read()).is_some()
        || crate::slack_lists::comments::own_delete::hides_reply(&state.read());
    let disabled = view.busy;
    let retry = view.capture_failed || view.error.is_some();
    let reaction_status = {
        let shell = state.read();
        crate::slack_lists::comments::presentation(&shell).and_then(|comments| {
            crate::slack_lists::comments::reaction_actions::status(&shell, comments.generation)
        })
    };
    let labels = {
        let shell = state.read();
        let blocks =
            serde_json::from_str::<Vec<serde_json::Value>>(&view.blocks).unwrap_or_default();
        let team = binding.owner_team();
        let mut labels = shell
            .core
            .workspaces
            .get(team)
            .and_then(|workspace| {
                serde_json::from_str::<std::collections::BTreeMap<String, String>>(
                    &crate::view::rich_editor::mention_labels(workspace, &blocks),
                )
                .ok()
            })
            .unwrap_or_default();
        labels.extend(
            crate::profile_edit::sections::rich::search::draft_file_labels(&shell, team, &blocks),
        );
        labels.extend(crate::slack_lists::comments::title_users::labels(
            &shell, &binding,
        ));
        serde_json::to_string(&labels).unwrap_or_else(|_| "{}".into())
    };
    rsx! {
        section {id:"slack-list-comment-compose",class:"slack-list-comment-compose","aria-label":"Write a comment",
            hidden:editing,style:if editing {"display:none"} else {"display:flex"},
            "data-list-comments-compose-scope":"{scope}","data-editor-generation":generation,
            if view.loading {p {class:"slack-list-comment-compose-status",role:"status","Loading saved draft…"}}
            else if view.ready {
                div {key:"list-comment-editor-{generation}",class:"slack-list-comment-compose-editor",
                    onmounted:{let binding=binding.clone();move |_|bridge::mount(state,binding.clone())},
                    crate::view::rich_editor::RichEditor {id:"slack-list-comment-draft",label:"Comment".to_owned(),
                        blocks:view.blocks,labels,disabled,autofocus:true,expected_generation:Some(generation)}
                }
                completion::Suggestions {state,generation,scope:scope.clone()}
            }
            if view.title_loading {p {id:"slack-list-comment-title-status",class:"slack-list-comment-compose-status",role:"status","Loading title details..."}}
            if let Some(error)=view.title_error {p {id:"slack-list-comment-title-status",class:"slack-list-comment-compose-status slack-list-error",role:"alert","{error}"}
                button {id:"slack-list-comment-title-retry",r#type:"button",class:"slack-list-action",disabled:view.title_loading,
                    onclick:{let binding=binding.clone();move |_|crate::slack_lists::comments::title_users::retry_draft(state,binding.clone())},"Retry title details"}
            }
            if let Some(status)=view.status {p {class:"slack-list-comment-compose-status",role:"status","{status}"}}
            if let Some(error)=view.error {p {class:"slack-list-comment-compose-status slack-list-error",role:"alert","{error}"}}
            div {class:"slack-list-comment-compose-actions",
                if let Some(status)=reaction_status {
                    p {id:"slack-list-comments-reaction-status",role:"status","{status}"}
                }
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
