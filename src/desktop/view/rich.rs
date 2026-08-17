use dioxus::prelude::*;

use crate::state::{Overlay, RichNode, ShellState};

pub(crate) fn reaction_label(name: &str) -> String {
    // Already a glyph (fixture) or a Slack shortcode.
    if name.chars().count() <= 4
        && name
            .chars()
            .any(|c| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
    {
        return name.to_owned();
    }
    super_platinum_core::state::emoji_glyph(name)
}

pub(crate) fn rich_node(
    node: &RichNode,
    media_epoch: u64,
    mut state: Signal<ShellState>,
) -> Element {
    match node {
        RichNode::Text(text) => rsx! { span { "{text}" } },
        RichNode::StyledText {
            text,
            bold,
            italic,
            strike,
            code,
        } => {
            let class = format!(
                "{}{}{}{}",
                if *bold { " bold" } else { "" },
                if *italic { " italic" } else { "" },
                if *strike { " strike" } else { "" },
                if *code { " inline-code" } else { "" }
            );
            rsx! { span { class: "{class}", "{text}" } }
        }
        RichNode::Link { label, url } => {
            let destination = url.clone();
            rsx! {
                a {
                    href: "{url}",
                    onclick: move |event| {
                        event.prevent_default();
                        if let Err(error) = crate::media::open_external(&destination) {
                            eprintln!("super-platinum: blocked external navigation: {error}");
                        }
                    },
                    "{label}"
                }
            }
        }
        RichNode::UserMention { user_id, label } => {
            let user = user_id.clone();
            rsx! {
                button {
                    class: "mention-chip",
                    onclick: move |_| {
                        spawn(crate::bootstrap::open_profile(state, user.clone()));
                    },
                    "{label}"
                }
            }
        }
        RichNode::ChannelMention { channel_id, label } => {
            let channel = channel_id.clone();
            rsx! {
                button {
                    class: "mention-chip channel-chip",
                    onclick: move |_| {
                        let index = {
                            state
                                .read()
                                .channels
                                .iter()
                                .position(|candidate| candidate.id == channel)
                        };
                        if let Some(index) = index {
                            state.write().select_channel(index);
                            spawn(crate::bootstrap::refresh_selected_channel(state));
                        }
                    },
                    "{label}"
                }
            }
        }
        RichNode::Code(code) => rsx! { code { "{code}" } },
        RichNode::Paragraph(nodes) => {
            rsx! { p { for node in nodes { {rich_node(node, media_epoch, state)} } } }
        }
        RichNode::Quote(nodes) => {
            rsx! { blockquote { for node in nodes { {rich_node(node, media_epoch, state)} } } }
        }
        RichNode::Emoji { name, glyph } => rsx! {
            span { class: "custom-emoji", title: ":{name}:", "{glyph}" }
        },
        RichNode::Media { id, name, mime } => {
            let uri = id.uri();
            if mime.starts_with("image/") {
                let viewer = crate::state::ViewerVm {
                    id: id.clone(),
                    name: name.clone(),
                    mime: mime.clone(),
                };
                rsx! { button { class: "media-button", onclick: move |_| { let mut shell = state.write(); shell.viewer = Some(viewer.clone()); shell.overlay = Some(Overlay::Viewer); }, img { key: "media-{media_epoch}-{uri}", class: "message-media", src: "{uri}", alt: "{name}" } } }
            } else if mime.starts_with("video/") {
                let viewer = crate::state::ViewerVm {
                    id: id.clone(),
                    name: name.clone(),
                    mime: mime.clone(),
                };
                rsx! { div { class: "media-preview", video { key: "media-{media_epoch}-{uri}", class: "message-video", src: "{uri}", controls: true, "{name}" } button { onclick: move |_| { let mut shell = state.write(); shell.viewer = Some(viewer.clone()); shell.overlay = Some(Overlay::Viewer); }, "Open viewer" } } }
            } else {
                rsx! { a { class: "file-link", href: "{uri}", download: "{name}", "📎 {name}" } }
            }
        }
    }
}

pub(crate) fn attachment_chip(
    mut state: Signal<ShellState>,
    attachment: &super_platinum_core::domain::ComposerAttachment,
    upload_epoch: u64,
) -> Element {
    let id = attachment.id;
    let name = crate::messaging::truncate_filename(&attachment.name);
    let kind = crate::messaging::attachment_kind_label(&attachment.path);
    let detail = if attachment.uploading {
        let ratio = crate::messaging::attachment_progress_ratio(attachment);
        format!(
            "{} · {:.0}%",
            crate::messaging::format_bytes(attachment.bytes),
            ratio * 100.0
        )
    } else {
        crate::messaging::format_bytes(attachment.bytes)
    };
    let ratio = crate::messaging::attachment_progress_ratio(attachment);
    let pct = (ratio * 100.0).round() as u32;
    rsx! {
        div {
            key: "attachment-{id}-{upload_epoch}",
            class: if attachment.uploading { "attachment-chip uploading" } else { "attachment-chip" },
            span { class: "attachment-kind", "{kind}" }
            div { class: "attachment-meta",
                span { class: "attachment-name", "{name}" }
                span { class: "attachment-detail", "{detail}" }
                if attachment.uploading {
                    div { class: "attachment-progress",
                        div { class: "attachment-progress-fill", style: "width: {pct}%" }
                    }
                }
            }
            button {
                class: "attachment-remove",
                title: "Remove",
                onclick: move |_| state.write().remove_attachment(id),
                "×"
            }
        }
    }
}

pub(crate) fn pending_attachment_strip(
    attachments: &[super_platinum_core::domain::ComposerAttachment],
    upload_epoch: u64,
) -> Element {
    rsx! {
        div { class: "pending-attachments", "data-upload-epoch": "{upload_epoch}",
            for attachment in attachments {
                {
                    let name = crate::messaging::truncate_filename(&attachment.name);
                    let kind = crate::messaging::attachment_kind_label(&attachment.path);
                    let size = crate::messaging::format_bytes(attachment.bytes);
                    let ratio = crate::messaging::attachment_progress_ratio(attachment);
                    let pct = (ratio * 100.0).round() as u32;
                    let detail = if attachment.uploading {
                        format!("Uploading… {pct}%")
                    } else {
                        size.clone()
                    };
                    rsx! {
                        div {
                            key: "pending-file-{attachment.id}-{upload_epoch}",
                            class: if attachment.uploading { "pending-file uploading" } else { "pending-file" },
                            div { class: "pending-file-preview",
                                span { "{kind}" }
                                if attachment.uploading {
                                    div { class: "upload-ring", style: "--progress: {pct}", title: "{pct}%" }
                                }
                            }
                            div { class: "pending-file-meta",
                                span { class: "attachment-name", "{name}" }
                                span { class: "attachment-detail", "{detail}" }
                                if attachment.uploading {
                                    div { class: "attachment-progress",
                                        div { class: "attachment-progress-fill", style: "width: {pct}%" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn message_plain_text(message: &crate::state::MessageVm) -> String {
    fn append(node: &RichNode, output: &mut String) {
        match node {
            RichNode::Text(text) | RichNode::StyledText { text, .. } | RichNode::Code(text) => {
                output.push_str(text)
            }
            RichNode::Link { label, .. }
            | RichNode::UserMention { label, .. }
            | RichNode::ChannelMention { label, .. } => output.push_str(label),
            RichNode::Emoji { glyph, .. } => output.push_str(glyph),
            RichNode::Media { name, .. } => output.push_str(name),
            RichNode::Paragraph(nodes) | RichNode::Quote(nodes) => {
                for node in nodes {
                    append(node, output);
                }
                output.push('\n');
            }
        }
    }
    let mut output = String::new();
    for node in &message.body {
        append(node, &mut output);
    }
    output.trim_end().to_owned()
}
