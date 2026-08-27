use dioxus::prelude::*;

use crate::state::{AttachmentVm, MessageVm, Overlay, ReactionVm, RichNode, ShellState};

pub(crate) fn rich_node(node: &RichNode, media_epoch: u64, state: Signal<ShellState>) -> Element {
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
                        open_link(&destination);
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
                    onmouseenter: {
                        let user = user_id.clone();
                        move |event: MouseEvent| {
                            let point = event.data().client_coordinates();
                            crate::profile::show_profile_hover(state, user.clone(), point.x, point.y);
                        }
                    },
                    onmouseleave: move |_| crate::profile::schedule_profile_hover_close(state),
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
                    onclick: move |_| open_channel(state, &channel),
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
        RichNode::EmojiImage { id, name } => {
            // Custom emoji paint over a transparent placeholder, so a message
            // that is nothing but one emoji reads as an empty row until the
            // bytes land — or forever, if that download never succeeds. Show
            // the shortcode until there is a picture to put in its place.
            if !state.read().media.is_ready(id) {
                return rsx! { span { class: "custom-emoji", title: ":{name}:", ":{name}:" } };
            }
            let uri = id.uri_at(media_epoch);
            rsx! {
                img {
                    key: "emoji-{media_epoch}-{uri}",
                    class: "emoji-image",
                    src: "{uri}",
                    alt: ":{name}:",
                    title: ":{name}:",
                }
            }
        }
        RichNode::Media {
            id,
            full,
            name,
            mime,
            size,
        } => {
            // The box the preview will fill, declared before its bytes land:
            // without it every arriving image shoves the transcript around
            // under the reader.
            let sizing = size
                .filter(|(width, height)| *width > 0 && *height > 0)
                .map(|(width, height)| {
                    format!("width: {width}px; aspect-ratio: {width} / {height}")
                })
                .unwrap_or_default();
            let Some(target) = full.as_ref().or(id.as_ref()) else {
                return rsx! { span { class: "file-link", "📎 {name}" } };
            };
            let viewer = viewer_vm(target, name, mime);
            let Some(id) = id.as_ref() else {
                return rsx! {
                    button {
                        class: "file-link",
                        onclick: move |_| open_viewer(state, viewer.clone()),
                        "📎 {name}"
                    }
                };
            };
            let uri = id.uri_at(media_epoch);
            if mime.starts_with("image/") {
                rsx! {
                    button {
                        class: "media-button",
                        onclick: move |_| open_viewer(state, viewer.clone()),
                        img {
                            key: "media-{media_epoch}-{uri}",
                            class: "message-media",
                            src: "{uri}",
                            alt: "{name}",
                            style: "{sizing}",
                        }
                    }
                }
            } else if mime.starts_with("video/") {
                // Inline is the poster frame Slack already rendered; the movie
                // itself downloads only once the viewer opens it.
                rsx! {
                    button {
                        class: "media-button media-poster",
                        onclick: move |_| open_viewer(state, viewer.clone()),
                        img {
                            key: "media-{media_epoch}-{uri}",
                            class: "message-media",
                            src: "{uri}",
                            alt: "{name}",
                            style: "{sizing}",
                        }
                        span { class: "media-play", "▶" }
                    }
                }
            } else {
                rsx! {
                    button {
                        class: "file-link",
                        onclick: move |_| open_viewer(state, viewer.clone()),
                        "📎 {name}"
                    }
                }
            }
        }

        // ── Block Kit layout ─────────────────────────────────────────────
        RichNode::Divider => rsx! { div { class: "block-divider" } },
        RichNode::Header(nodes) => rsx! {
            h3 { class: "block-header", for node in nodes { {rich_node(node, media_epoch, state)} } }
        },
        RichNode::Context(nodes) => rsx! {
            div { class: "block-context", for node in nodes { {rich_node(node, media_epoch, state)} } }
        },
        RichNode::Section {
            text,
            fields,
            accessory,
        } => rsx! {
            div { class: "block-section",
                div { class: "block-section-text",
                    for node in text { {rich_node(node, media_epoch, state)} }
                    if !fields.is_empty() {
                        div { class: "block-fields",
                            for (index, field) in fields.iter().enumerate() {
                                div { key: "field-{index}", class: "block-field",
                                    for node in field { {rich_node(node, media_epoch, state)} }
                                }
                            }
                        }
                    }
                }
                if let Some(accessory) = accessory.as_ref() {
                    div { class: "block-accessory", {rich_node(accessory, media_epoch, state)} }
                }
            }
        },
        RichNode::Actions(nodes) => rsx! {
            div { class: "block-actions", for node in nodes { {rich_node(node, media_epoch, state)} } }
        },
        RichNode::Button { label, url, style } => match url {
            Some(url) => {
                let destination = url.clone();
                rsx! {
                    button {
                        class: "{style.class()}",
                        title: "{url}",
                        onclick: move |_| open_link(&destination),
                        "{label}"
                    }
                }
            }
            // Menus and submit buttons need a Slack interaction round-trip we
            // do not implement; keep the shape, drop the affordance.
            None => rsx! {
                button {
                    class: "{style.class()} inert",
                    disabled: true,
                    title: "Interactive Slack element",
                    "{label}"
                }
            },
        },
        RichNode::InlineImage { id, alt } => {
            let uri = id.uri_at(media_epoch);
            rsx! {
                img {
                    key: "inline-{media_epoch}-{uri}",
                    class: "block-inline-image",
                    src: "{uri}",
                    alt: "{alt}",
                    title: "{alt}",
                }
            }
        }
        RichNode::ImageBlock {
            id,
            alt,
            title,
            size,
        } => {
            let uri = id.uri_at(media_epoch);
            let viewer = viewer_vm(id, title.as_deref().unwrap_or(alt), "image/png");
            // Slack draws block images at their declared size, capped; keeping
            // the aspect ratio also stops the row reflowing when bytes land.
            let sizing = size
                .map(|(width, height)| {
                    format!("width: {width}px; aspect-ratio: {width} / {height}")
                })
                .unwrap_or_default();
            rsx! {
                figure { class: "block-image",
                    if let Some(title) = title.as_ref() {
                        figcaption { "{title}" }
                    }
                    button {
                        class: "media-button",
                        onclick: move |_| open_viewer(state, viewer.clone()),
                        img {
                            key: "block-image-{media_epoch}-{uri}",
                            class: "block-image-media",
                            src: "{uri}",
                            alt: "{alt}",
                            style: "{sizing}",
                        }
                    }
                }
            }
        }
        RichNode::List {
            ordered,
            indent,
            offset,
            items,
        } => {
            let indent = format!("--list-indent: {indent}");
            rsx! {
                if *ordered {
                    ol { class: "block-list", style: "{indent}", start: "{offset + 1}",
                        for (index, item) in items.iter().enumerate() {
                            li { key: "li-{index}",
                                for node in item { {rich_node(node, media_epoch, state)} }
                            }
                        }
                    }
                } else {
                    ul { class: "block-list", style: "{indent}",
                        for (index, item) in items.iter().enumerate() {
                            li { key: "li-{index}",
                                for node in item { {rich_node(node, media_epoch, state)} }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn viewer_vm(
    id: &super_platinum_core::MediaAssetId,
    name: &str,
    mime: &str,
) -> crate::state::ViewerVm {
    crate::state::ViewerVm {
        id: id.clone(),
        name: name.to_owned(),
        mime: mime.to_owned(),
    }
}

fn open_viewer(mut state: Signal<ShellState>, viewer: crate::state::ViewerVm) {
    let mut shell = state.write();
    // Full-resolution bytes are deferred until exactly this moment.
    shell.media.request(&viewer.id);
    shell.viewer = Some(viewer);
    shell.overlay = Some(Overlay::Viewer);
}

fn open_link(url: &str) {
    if let Err(error) = crate::media::open_external(url) {
        eprintln!("super-platinum: blocked external navigation: {error}");
    }
}

fn open_channel(mut state: Signal<ShellState>, channel: &str) {
    let index = state
        .read()
        .channels
        .iter()
        .position(|candidate| candidate.id == channel);
    if let Some(index) = index {
        state.write().select_channel(index);
        spawn(crate::bootstrap::refresh_selected_channel(state));
    }
}

/// The embeds under a message body: Slack message unfurls, link unfurls, app
/// unfurls, and legacy bot attachments.
pub(crate) fn attachment_embeds(
    state: Signal<ShellState>,
    media_epoch: u64,
    attachments: &[AttachmentVm],
) -> Element {
    rsx! {
        div { class: "message-attachments",
            for attachment in attachments {
                {attachment_embed(state, media_epoch, attachment)}
            }
        }
    }
}

fn attachment_embed(
    state: Signal<ShellState>,
    media_epoch: u64,
    attachment: &AttachmentVm,
) -> Element {
    let bar = attachment
        .color
        .as_deref()
        .map(|color| format!("background: {color}"))
        .unwrap_or_default();
    rsx! {
        div { key: "attachment-{attachment.key}", class: "message-attachment",
            div { class: "attachment-bar", style: "{bar}" }
            div { class: "attachment-body",
                if !attachment.pretext.is_empty() {
                    div { class: "attachment-pretext",
                        for node in &attachment.pretext { {rich_node(node, media_epoch, state)} }
                    }
                }
                if let Some(service) = attachment.service.as_ref() {
                    div { class: "attachment-service",
                        if let Some(icon) = attachment.service_icon.as_ref() {
                            img { key: "svc-{icon.uri()}", src: "{icon.uri_at(media_epoch)}", alt: "" }
                        }
                        span { "{service}" }
                    }
                }
                if let Some(author) = attachment.author.as_ref() {
                    div { class: "attachment-author",
                        if let Some(icon) = author.icon.as_ref() {
                            img { key: "author-{icon.uri()}", src: "{icon.uri_at(media_epoch)}", alt: "" }
                        }
                        {
                            let user = author.user_id.clone();
                            rsx! {
                                button {
                                    class: "attachment-author-name",
                                    disabled: user.is_none(),
                                    onmouseenter: {
                                        let user = user.clone();
                                        move |event: MouseEvent| {
                                            if let Some(user) = user.clone() {
                                                let point = event.data().client_coordinates();
                                                crate::profile::show_profile_hover(state, user, point.x, point.y);
                                            }
                                        }
                                    },
                                    onmouseleave: move |_| crate::profile::schedule_profile_hover_close(state),
                                    onclick: move |_| {
                                        if let Some(user) = user.clone() {
                                            spawn(crate::bootstrap::open_profile(state, user));
                                        }
                                    },
                                    "{author.name}"
                                }
                            }
                        }
                    }
                }
                if let Some(title) = attachment.title.as_ref() {
                    div { class: "attachment-title",
                        if let Some(link) = attachment.title_link.clone() {
                            a {
                                href: "{link}",
                                onclick: move |event| {
                                    event.prevent_default();
                                    open_link(&link);
                                },
                                "{title}"
                            }
                        } else {
                            span { "{title}" }
                        }
                    }
                }
                if !attachment.body.is_empty() {
                    div { class: "attachment-text",
                        for node in &attachment.body { {rich_node(node, media_epoch, state)} }
                    }
                }
                if !attachment.fields.is_empty() {
                    div { class: "attachment-fields",
                        for (index, (title, value, short)) in attachment.fields.iter().enumerate() {
                            div {
                                key: "af-{index}",
                                class: if *short { "attachment-field short" } else { "attachment-field" },
                                if !title.is_empty() { div { class: "attachment-field-title", "{title}" } }
                                div { class: "attachment-field-value",
                                    for node in value { {rich_node(node, media_epoch, state)} }
                                }
                            }
                        }
                    }
                }
                if !attachment.files.is_empty() {
                    div { class: "attachment-files",
                        for node in &attachment.files { {rich_node(node, media_epoch, state)} }
                    }
                }
                if let Some(thumb) = attachment.thumb.as_ref() {
                    img { key: "thumb-{thumb.uri()}", class: "attachment-thumb", src: "{thumb.uri_at(media_epoch)}", alt: "" }
                }
                if let Some((image, size)) = attachment.image.as_ref() {
                    {
                        let uri = image.uri_at(media_epoch);
                        // Reserve the real aspect ratio so the row does not
                        // reflow when the bytes land.
                        let ratio = size
                            .map(|(width, height)| format!("aspect-ratio: {width} / {height}"))
                            .unwrap_or_default();
                        let viewer = viewer_vm(image, attachment.title.as_deref().unwrap_or("Image"), "image/png");
                        rsx! {
                            button {
                                class: "media-button attachment-image",
                                onclick: move |_| open_viewer(state, viewer.clone()),
                                img { key: "att-img-{media_epoch}-{uri}", src: "{uri}", alt: "", style: "{ratio}" }
                            }
                        }
                    }
                }
                if let Some(footer) = attachment.footer.as_ref() {
                    div { class: "attachment-footer",
                        if let Some(icon) = footer.icon.as_ref() {
                            img { key: "footer-{icon.uri()}", src: "{icon.uri_at(media_epoch)}", alt: "" }
                        }
                        {
                            let channel = footer.channel_id.clone();
                            rsx! {
                                span { class: "attachment-footer-origin",
                                    "{footer.lead}"
                                    if let Some(label) = footer.channel_label.as_ref() {
                                        button {
                                            class: "attachment-channel-link",
                                            disabled: channel.is_none(),
                                            onclick: move |_| {
                                                if let Some(channel) = channel.as_deref() {
                                                    open_channel(state, channel);
                                                }
                                            },
                                            "#{label}"
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(stamp) = footer.stamp.as_ref() {
                            span { class: "attachment-footer-sep", "|" }
                            span { "{stamp}" }
                        }
                        if let Some((label, url)) = footer.permalink.clone() {
                            span { class: "attachment-footer-sep", "|" }
                            a {
                                href: "{url}",
                                onclick: move |event| {
                                    event.prevent_default();
                                    open_link(&url);
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The reaction pills under a message. Clicking one toggles it.
pub(crate) fn reactions_row(
    state: Signal<ShellState>,
    media: &crate::media::MediaRegistry,
    media_epoch: u64,
    channel: &str,
    message: &MessageVm,
) -> Element {
    rsx! {
        for reaction in &message.reactions {
            {reaction_pill(state, media, media_epoch, channel, &message.id, reaction)}
        }
    }
}

fn reaction_pill(
    state: Signal<ShellState>,
    media: &crate::media::MediaRegistry,
    media_epoch: u64,
    channel: &str,
    ts: &str,
    reaction: &ReactionVm,
) -> Element {
    let channel = channel.to_owned();
    let ts = ts.to_owned();
    let name = reaction.name.clone();
    let own = reaction.own;
    rsx! {
        button {
            key: "reaction-{reaction.name}",
            class: if own { "reaction own" } else { "reaction" },
            title: ":{reaction.name}:",
            onclick: move |_| {
                spawn(crate::bootstrap::toggle_reaction(state, channel.clone(), ts.clone(), name.clone(), own));
            },
            if let Some(emoji) = reaction.media.as_ref().filter(|emoji| media.is_ready(emoji)) {
                img {
                    class: "reaction-emoji",
                    src: "{emoji.uri_at(media_epoch)}",
                    alt: ":{reaction.name}:",
                }
            } else if reaction.media.is_some() {
                // A custom emoji still downloading. Hold the pill's shape rather
                // than leaving a gap where the art will be.
                span { class: "reaction-emoji pending" }
            } else {
                span { class: "reaction-glyph", "{reaction.glyph.clone().unwrap_or_else(|| reaction.fallback())}" }
            }
            span { class: "reaction-count", "{reaction.count}" }
        }
    }
}

/// Slack's thread reply bar: replier avatars, the count, and the last reply age.
pub(crate) fn reply_bar(
    state: Signal<ShellState>,
    media: &crate::media::MediaRegistry,
    media_epoch: u64,
    channel: &str,
    message: &MessageVm,
) -> Element {
    let channel = channel.to_owned();
    let ts = message.id.clone();
    let label = if message.reply_count == 1 {
        "1 reply".to_owned()
    } else {
        format!("{} replies", message.reply_count)
    };
    rsx! {
        button {
            class: "reply-bar",
            onclick: move |_| {
                spawn(crate::bootstrap::open_thread(state, channel.clone(), ts.clone()));
            },
            for (user, avatar, initials) in &message.reply_avatars {
                span { key: "replier-{user}", class: "reply-avatar",
                    if let Some(avatar) = avatar.as_ref().filter(|avatar| media.is_ready(avatar)) {
                        img { src: "{avatar.uri_at(media_epoch)}", alt: "" }
                    } else {
                        "{initials}"
                    }
                }
            }
            span { class: "reply-count", "{label}" }
            if let Some(last) = message.last_reply.as_ref() {
                span { class: "reply-last", "Last reply {last}" }
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
    message
        .body
        .iter()
        .map(RichNode::plain_text)
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_owned()
}
