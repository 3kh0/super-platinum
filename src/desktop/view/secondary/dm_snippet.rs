//! Passive inline decoration only when it preserves the original DM snippet.
#[cfg(test)]
use super::snippet::inline;
use super::snippet::{Preview, first as inline_first};
use crate::model::RichNode;
use crate::state::ShellState;
use super_platinum_core::{slack::models::DmEntry, state::Workspace};

pub(super) fn project(
    snapshot: &ShellState,
    workspace: Option<&Workspace>,
    entry: &DmEntry,
) -> Preview {
    let Some(message) = entry.message.as_ref() else {
        return Preview::plain(String::new());
    };
    let Some(ws) = workspace else {
        return Preview::plain(super_platinum_core::state::message_text(message));
    };
    let has_content = message
        .text
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
        || !message.blocks.is_empty()
        || !message.files.is_empty()
        || !message.attachments.is_empty();
    if has_content && snapshot.core.dm_preview_was_deleted(entry) {
        return Preview::plain("This message was deleted.".into());
    }
    // Exactly the VM work the old string preview performed, once. The inert
    // copy below adds no parsing, registration, fetch, or action ownership.
    let vm = crate::message_vm::message_vm(ws, message, &snapshot.media);
    let plain = super::super::rich::message_plain_text(&vm);
    let body = plain
        .lines()
        .find(|s| !s.trim().is_empty())
        .unwrap_or("")
        .trim();
    let is_mpim = ws
        .channels
        .get(&entry.id)
        .or(entry.channel.as_ref())
        .map(|c| c.is_mpim)
        .unwrap_or(false);
    let prefix = match message.user.as_deref() {
        Some(user) if user == ws.self_user_id => "You: ".into(),
        Some(user) if is_mpim => format!("{}: ", ws.display_name(user)),
        _ => String::new(),
    };
    let text = format!("{prefix}{body}");
    let mut rich = inline_first(crate::message_vm::preview::nodes(vm.body), body);
    if let Some(nodes) = rich.as_mut() {
        if !prefix.is_empty() {
            nodes.insert(0, RichNode::Text(prefix));
        }
    }
    Preview { text, rich }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn inline_styles_keep_exact_first_line_prefix_and_unsupported_bounded_fallbacks() {
        let shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let ws = &shell.core.workspaces["T1"];
        let mut entry: DmEntry = serde_json::from_value(json!({"id":"D_STYLE","channel":{"id":"D_STYLE","is_im":true,"user":"U2"},"message":{"user":ws.self_user_id,"ts":"1719801000.000400","text":"First *bold* part\nSecond paragraph stays out"}})).unwrap();
        // The original text-only fallback preserves literal delimiters. It
        // must not gain parsing just to manufacture formatted preview coverage.
        let literal = project(&shell, Some(ws), &entry);
        assert_eq!(literal.text, "You: First *bold* part");
        assert!(literal.rich.is_none());
        entry.message.as_mut().unwrap().blocks = vec![json!({"type":"rich_text","elements":[
            {"type":"rich_text_section","elements":[
                {"type":"text","text":"First "},
                {"type":"text","text":"bold","style":{"bold":true}},
                {"type":"text","text":" part\nSecond paragraph stays out"}
            ]}
        ]})];
        let result = project(&shell, Some(ws), &entry);
        assert_eq!(result.text, "You: First bold part");
        let nodes = result.rich.unwrap();
        assert_eq!(RichNode::Paragraph(nodes.clone()).plain_text(), result.text);
        assert!(format!("{nodes:?}").contains("bold: true"));
        assert!(!format!("{nodes:?}").contains("Second paragraph"));
        entry.message.as_mut().unwrap().user = Some("U1".into());
        entry.channel.as_mut().unwrap().is_mpim = true;
        let result = project(&shell, Some(ws), &entry);
        assert_eq!(
            result.text,
            format!("{}: First bold part", ws.display_name("U1"))
        );
        assert_eq!(
            RichNode::Paragraph(result.rich.unwrap()).plain_text(),
            result.text
        );
        assert!(project(&shell, None, &entry).rich.is_none());
        let text = RichNode::StyledText {
            text: "same".into(),
            bold: true,
            italic: false,
            strike: false,
            code: false,
        };
        assert!(inline_first(vec![RichNode::Paragraph(vec![text.clone()])], "different").is_none());
        assert!(inline_first(vec![RichNode::Quote(vec![text.clone()])], "same").is_none());
        assert!(
            inline_first(
                vec![RichNode::Paragraph(vec![text; 65])],
                &"same".repeat(65)
            )
            .is_none()
        );
        assert!(inline_first(vec![RichNode::Text("plain".into())], "plain").is_none());
        let mut code = false;
        let node = inline(
            RichNode::StyledText {
                text: "code".into(),
                bold: false,
                italic: false,
                strike: false,
                code: true,
            },
            &mut 2,
            &mut code,
        )
        .unwrap();
        assert!(matches!(node, RichNode::StyledText { code: false, .. }));
        assert!(!code);
    }
}
