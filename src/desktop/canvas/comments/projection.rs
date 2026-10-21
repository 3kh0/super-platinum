//! Canvas root fallback bodies use the original message's display fields.
//! The retained root remains untouched for navigation, selection and mutations.
use crate::blocks::{BlockCtx, block_nodes, canvas_root_mrkdwn, plain_inline_nodes};
use crate::media::MediaRegistry;
use crate::model::{MessageVm, RichNode};
use super_platinum_core::canvas::comments::display::{RootBody, root_body};
use super_platinum_core::{slack::models::Message, state::Workspace};
mod text;

/// Read-time metadata for a file-bearing root. Its body is selected reactively
/// from the original source only after current canonical override admission.
pub(super) fn metadata_vm(
    workspace: &Workspace,
    message: &Message,
    media: &MediaRegistry,
) -> MessageVm {
    let mut metadata = message.clone();
    metadata.blocks.clear();
    metadata.text = None;
    metadata.files.clear();
    metadata.attachments.clear();
    let mut value = crate::message_vm::message_vm_with_pending(workspace, &metadata, media, false);
    value.body.clear();
    value
}

pub(super) fn message_vm(
    workspace: &Workspace,
    message: &Message,
    media: &MediaRegistry,
) -> MessageVm {
    if let Some(body) = root_body(message) {
        // Project metadata without traversing superseded/hidden body blocks or
        // registering their media. The original response remains in Row.message.
        let mut metadata = message.clone();
        metadata.blocks.clear();
        metadata.text = None;
        if matches!(body, RootBody::Hidden) || !message.files.is_empty() {
            // Hidden content and file-root fallback do not append generic
            // file/attachment cards. Suppress their traversal before VM
            // construction; original blocks remain the selected body's input.
            // No canonical first-file link override is inferred from embeds.
            metadata.files.clear();
            metadata.attachments.clear();
        }
        let mut value =
            crate::message_vm::message_vm_with_pending(workspace, &metadata, media, false);
        let context = BlockCtx::new(workspace, media);
        value.body = match body {
            RootBody::Hidden => vec![RichNode::Text("This content was deleted.".into())],
            RootBody::Unavailable => Vec::new(),
            // A nonempty blocks array owns the snippet even when its contents
            // are unsupported. Never disclose a fallback the server superseded.
            RootBody::Blocks(blocks) => blocks
                .iter()
                .flat_map(|block| block_nodes(context, block))
                .collect(),
            RootBody::Text { text, mrkdwn } => {
                let mut nodes = if mrkdwn {
                    canvas_root_mrkdwn(context, text)
                } else {
                    text.split('\n')
                        .map(|line| RichNode::Paragraph(plain_inline_nodes(context, line)))
                        .collect()
                };
                text::decode(&mut nodes);
                nodes
            }
        };
        value
    } else {
        crate::message_vm::message_vm_with_pending(workspace, message, media, false)
    }
}

#[cfg(test)]
mod tests;
