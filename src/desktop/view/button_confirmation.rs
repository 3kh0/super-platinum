//! Message-owned app confirmation; all actions are revalidated by the controller.
use dioxus::prelude::*;
use super_platinum_core::block_actions::{BlockText, BlockTextFormat};

use crate::{
    icons::{Icon, icon},
    model::RichNode,
    state::ShellState,
};

pub(crate) fn button_confirmation(state: Signal<ShellState>) -> Element {
    let Some(revision) = crate::message_buttons::confirmation(&state.read()).map(|c| c.revision)
    else {
        return rsx! {};
    };
    rsx! {ConfirmationDialog {key:"button-confirmation-{revision}",state}}
}

#[component]
fn ConfirmationDialog(state: Signal<ShellState>) -> Element {
    let shell = state.read();
    let Some(confirmation) = crate::message_buttons::confirmation(&shell) else {
        return rsx! {};
    };
    let title = plain_label(&confirmation.title);
    let confirm_label = plain_label(&confirmation.confirm_label);
    let cancel_label = plain_label(&confirmation.cancel_label);
    let body = match &confirmation.text.format {
        BlockTextFormat::Mrkdwn => shell
            .core
            .active_team
            .as_ref()
            .and_then(|team| shell.core.workspaces.get(team))
            .map(|workspace| {
                crate::blocks::mrkdwn_blocks(
                    crate::blocks::BlockCtx::new(workspace, &shell.media),
                    &confirmation.text.text,
                )
            })
            .unwrap_or_else(|| vec![RichNode::Text(plain_label(&confirmation.text))]),
        _ => vec![RichNode::Text(plain_label(&confirmation.text))],
    };
    let confirm_class = if confirmation.style.as_deref() == Some("danger") {
        "button-confirmation-submit danger"
    } else {
        "button-confirmation-submit"
    };
    let revision = confirmation.revision;
    let busy = confirmation.busy;
    let media_epoch = shell.media_epoch;
    drop(shell);
    rsx! {
        div {class:"scrim",onclick:move |_|crate::message_buttons::cancel(state)}
        section {id:"shell-overlay-dialog",class:"modal button-confirmation",role:"dialog",
            "aria-modal":"true","aria-labelledby":"button-confirmation-title",
            "aria-describedby":"button-confirmation-body",tabindex:"-1",
            "data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":revision,
            onkeydown:move |event|if event.key()==Key::Escape {
                event.prevent_default();event.stop_propagation();crate::message_buttons::cancel(state);
            },
            onmounted:move |_|{dioxus::document::eval("requestAnimationFrame(()=>document.getElementById('button-confirmation-cancel')?.focus({preventScroll:true}));");},
            header {
                h2 {id:"button-confirmation-title","{title}"}
                button {r#type:"button",class:"icon-btn","aria-label":"Close confirmation",
                    onclick:move |_|crate::message_buttons::cancel(state),{icon(Icon::Close,"icon")}}
            }
            div {id:"button-confirmation-body",class:"button-confirmation-body",
                for node in &body {{super::rich::rich_node(node,media_epoch,state)}}
            }
            form {id:"button-confirmation-form",onsubmit:move |event|{
                event.prevent_default();crate::message_buttons::confirm(state);
            },
                footer {
                    button {id:"button-confirmation-cancel",r#type:"button",
                        onclick:move |_|crate::message_buttons::cancel(state),"{cancel_label}"}
                    button {r#type:"submit",class:"{confirm_class}",disabled:busy,"aria-busy":busy.to_string(),
                        "{confirm_label}"}
                }
            }
        }
    }
}

// Composition labels stay literal. Decode Slack's three escapes exactly once;
// neither plain text nor unknown formatting is interpreted as links or HTML.
fn plain_label(text: &BlockText) -> String {
    let decoded = text
        .text
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    if text.emoji == Some(false) {
        decoded
    } else {
        super_platinum_core::state::emoji_text_to_display(&decoded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_confirmation_text_decodes_once_and_keeps_markup_literal() {
        let text = BlockText {
            format: BlockTextFormat::PlainText,
            text: "R&amp;D &lt;https://example.com|review&gt; &amp;lt;script&amp;gt; *literal*"
                .into(),
            emoji: None,
            verbatim: None,
        };
        assert_eq!(
            plain_label(&text),
            "R&D <https://example.com|review> &lt;script&gt; *literal*"
        );
    }
}
