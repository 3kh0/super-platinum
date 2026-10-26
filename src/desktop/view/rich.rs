mod file_mentions;
use super::media_image::MediaImage;
use dioxus::prelude::*;
pub(super) use file_mentions::ListSource as ListFileSource;

use crate::state::{AttachmentVm, MessageVm, Overlay, ReactionVm, RichNode, ShellState};

pub(crate) fn rich_node(node: &RichNode, media_epoch: u64, state: Signal<ShellState>) -> Element {
    rich_node_with_source(node, media_epoch, state, None, None, None)
}

/// Only canonical message bodies provide action ownership. Attachments, app
/// forms, scheduled messages and drafts continue through ownerless rich_node.
pub(crate) fn rich_node_for_message(
    node: &RichNode,
    media_epoch: u64,
    state: Signal<ShellState>,
    source: &crate::message_buttons::source::Source,
) -> Element {
    rich_node_with_source(node, media_epoch, state, Some(source), None, None)
}

pub(crate) fn rich_node_for_home(
    node: &RichNode,
    media_epoch: u64,
    state: Signal<ShellState>,
    key: crate::app_home::view::Key,
) -> Element {
    rich_node_with_source(node, media_epoch, state, None, Some(key), None)
}

/// List ownership stays distinct even when capture refused; it must never fall
/// through to ownerless URL navigation.
pub(super) fn rich_node_for_list(
    node: &RichNode,
    media_epoch: u64,
    state: Signal<ShellState>,
    source: &ListFileSource,
) -> Element {
    rich_node_with_source(node, media_epoch, state, None, None, Some(source))
}

fn rich_node_with_source(
    node: &RichNode,
    media_epoch: u64,
    state: Signal<ShellState>,
    source: Option<&crate::message_buttons::source::Source>,
    home: Option<crate::app_home::view::Key>,
    list: Option<&ListFileSource>,
) -> Element {
    match node {
        RichNode::Canvas(target) => {
            let target = target.clone();
            rsx! {button {class:"file-link",onclick:move |_|crate::canvas::open_target(state,target.clone()),{crate::icons::icon(crate::icons::Icon::AttachFile,"icon file-link-icon")} "{target.title}"}}
        }
        RichNode::FileMention {
            team,
            raw,
            fallback,
        } => file_mentions::render(team, raw, fallback, state, node, list),
        RichNode::Text(text) => rsx! { span { "{text}" } },
        RichNode::Underlined(child) => rsx! {
            u { class: "rich-underline",
                {rich_node_with_source(child, media_epoch, state, source, home, list)}
            }
        },
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
                        open_link(state, &destination);
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
        RichNode::GroupMention {
            group_id,
            label,
            member,
        } => {
            let id = group_id.clone();
            rsx! {
                button {
                    class: if *member { "mention-chip group-mention member" } else { "mention-chip group-mention" },
                    onclick: move |_| { spawn(crate::usergroups::open(state, id.clone())); },
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
            if source.is_some()
                && nodes
                    .iter()
                    .any(|node| matches!(node, RichNode::Media { file: Some(_), .. }))
            {
                // File actions contain a form: a paragraph cannot own this flow content.
                rsx! { div { class:"message-file-paragraph", for node in nodes { {rich_node_with_source(node, media_epoch, state, source, home, list)} } } }
            } else {
                rsx! { p { for node in nodes { {rich_node_with_source(node, media_epoch, state, source, home, list)} } } }
            }
        }
        RichNode::Quote(nodes) => {
            rsx! { blockquote { for node in nodes { {rich_node_with_source(node, media_epoch, state, source, home, list)} } } }
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
            rsx! {
                MediaImage {
                    state, id:id.clone(), media_epoch, compact:true,
                    fallback:rsx! { span {class:"custom-emoji",title:":{name}:",":{name}:"} },
                    class: "emoji-image",
                    alt: ":{name}:",
                    title: ":{name}:",
                }
            }
        }
        RichNode::Media {
            file,
            id,
            full,
            name,
            download_name,
            mime,
            size,
        } => {
            let preview = (|| -> Element {
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
                    return rsx! { span { class: "file-link", {crate::icons::icon(crate::icons::Icon::AttachFile, "icon file-link-icon")} "{name}" } };
                };
                let mut viewer = viewer_vm(target, name, mime);
                viewer.download_name = download_name.clone();
                viewer.file = file.clone();
                let Some(id) = id.as_ref() else {
                    return rsx! {
                        button {
                            class: "file-link",
                            onclick: move |_| open_viewer(state, viewer.clone()),
                            {crate::icons::icon(crate::icons::Icon::AttachFile, "icon file-link-icon")}
                            "{name}"
                        }
                    };
                };
                if mime.starts_with("image/") {
                    rsx! {
                        button {
                            class: "media-button",
                            onclick: move |_| open_viewer(state, viewer.clone()),
                            MediaImage {
                                state, id:id.clone(), media_epoch,
                                class: "message-media",
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
                            MediaImage {
                                state, id:id.clone(), media_epoch,
                                class: "message-media",
                                alt: "{name}",
                                style: "{sizing}",
                            }
                            span { class: "media-play", {crate::icons::icon(crate::icons::Icon::Play, "media-play-icon")} }
                        }
                    }
                } else {
                    rsx! {
                        button {
                            class: "file-link",
                            onclick: move |_| open_viewer(state, viewer.clone()),
                            {crate::icons::icon(crate::icons::Icon::AttachFile, "icon file-link-icon")}
                            "{name}"
                        }
                    }
                }
            })();
            let target = source.zip(file.as_ref()).and_then(|(source, file)| {
                crate::file_actions::attachment::target(&state.read(), source, file, name)
            });
            if let Some(target) = target {
                rsx! { crate::file_actions::attachment::MessageFile { state, target, children:preview } }
            } else {
                preview
            }
        }

        // ── Block Kit layout ─────────────────────────────────────────────
        RichNode::Divider => rsx! { div { class: "block-divider" } },
        RichNode::Header(nodes) => rsx! {
            h3 { class: "block-header", for node in nodes { {rich_node_with_source(node, media_epoch, state, source, home, list)} } }
        },
        RichNode::Context(nodes) => rsx! {
            div { class: "block-context", for node in nodes { {rich_node_with_source(node, media_epoch, state, source, home, list)} } }
        },
        RichNode::Section {
            text,
            fields,
            accessory,
        } => rsx! {
            div { class: "block-section",
                div { class: "block-section-text",
                    for node in text { {rich_node_with_source(node, media_epoch, state, source, home, list)} }
                    if !fields.is_empty() {
                        div { class: "block-fields",
                            for (index, field) in fields.iter().enumerate() {
                                div { key: "field-{index}", class: "block-field",
                                    for node in field { {rich_node_with_source(node, media_epoch, state, source, home, list)} }
                                }
                            }
                        }
                    }
                }
                if let Some(accessory) = accessory.as_ref() {
                    div { class: "block-accessory", {rich_node_with_source(accessory, media_epoch, state, source, home, list)} }
                }
            }
        },
        RichNode::Actions(nodes) => rsx! {
            div { class: "block-actions", for node in nodes { {rich_node_with_source(node, media_epoch, state, source, home, list)} } }
        },
        RichNode::Button {
            label,
            url,
            style,
            metadata,
        } => {
            let accessible_label = metadata
                .accessibility_label
                .as_deref()
                .filter(|label| !label.trim().is_empty())
                .unwrap_or(label);
            if let Some(key) = home {
                if let Some(control)=crate::app_home::inputs::static_select::wrappers::render(state,key,metadata) {
                    control
                } else {
                    crate::app_home::view::button(state,key,metadata.as_ref().clone(),label.clone(),accessible_label.to_owned(),*style)
                }
            } else if let Some(source) = source.filter(|_| {
                matches!(
                    metadata.source,
                    super_platinum_core::block_actions::BlockElementSource::Button
                )
            }) {
                rsx! {super::message_button::MessageAppButton {
                    state,source:source.clone(),metadata:metadata.as_ref().clone(),
                    label:label.clone(),accessibility_label:accessible_label.to_owned(),
                    url:url.clone(),style:*style,
                }}
            } else if let Some(source) = source.filter(|_| {
                matches!(&metadata.source,
                    super_platinum_core::block_actions::BlockElementSource::UnsupportedControl { kind }
                        if kind == "static_select")
            }) {
                rsx! {super::message_select::MessageStaticSelect {
                    state,source:source.clone(),metadata:metadata.as_ref().clone(),
                    accessibility_label:metadata.accessibility_label.clone().unwrap_or_default(),
                }}
            } else if let Some(source) = source.filter(|_| {
                matches!(&metadata.source,super_platinum_core::block_actions::BlockElementSource::UnsupportedControl {kind} if matches!(kind.as_str(),"external_select"|"multi_external_select"))
            }) {
                rsx! {super::message_external::MessageExternalSelect {state,source:source.clone(),metadata:metadata.as_ref().clone()}}
            } else if let Some(source) = source.filter(|_| {
                matches!(&metadata.source,
                    super_platinum_core::block_actions::BlockElementSource::UnsupportedControl { kind } if kind == "overflow")
            }) {
                rsx! {super::message_overflow::MessageOverflow {
                    state,source:source.clone(),metadata:metadata.as_ref().clone(),
                    accessibility_label:metadata.accessibility_label.clone().unwrap_or_default(),
                }}
            } else if let Some(source) = source.filter(|_| {
                matches!(&metadata.source,
                    super_platinum_core::block_actions::BlockElementSource::UnsupportedControl { kind }
                        if kind == "datepicker")
            }) {
                rsx! {super::message_date::MessageDatepicker {
                    state,source:source.clone(),metadata:metadata.as_ref().clone(),
                    accessibility_label:metadata.accessibility_label.clone().unwrap_or_default(),
                }}
            } else if let Some(source) = source.filter(|_| {
                matches!(&metadata.source,
                    super_platinum_core::block_actions::BlockElementSource::UnsupportedControl { kind }
                        if matches!(kind.as_str(), "users_select" | "multi_users_select" | "multi_channels_select" | "multi_conversations_select" | "conversations_select" | "channels_select"))
            }) {
                rsx! {super::message_user::MessageUserSelect {
                    state,source:source.clone(),metadata:metadata.as_ref().clone(),
                    accessibility_label:metadata.accessibility_label.clone().unwrap_or_default(),
                }}
            } else if let Some(source) = source.filter(|_| {
                matches!(&metadata.source,
                    super_platinum_core::block_actions::BlockElementSource::UnsupportedControl { kind }
                        if kind == "timepicker")
            }) {
                rsx! {super::message_time::MessageTimepicker {
                    state,source:source.clone(),metadata:metadata.as_ref().clone(),
                    accessibility_label:metadata.accessibility_label.clone().unwrap_or_default(),
                }}
            } else if let Some(source) = source.filter(|_| {
                matches!(&metadata.source,
                    super_platinum_core::block_actions::BlockElementSource::UnsupportedControl { kind }
                        if kind == "radio_buttons")
            }) {
                rsx! {super::message_radio::MessageRadioGroup {
                    state,source:source.clone(),metadata:metadata.as_ref().clone(),
                    accessibility_label:metadata.accessibility_label.clone().unwrap_or_default(),
                }}
            } else if let Some(source) = source.filter(|_| {
                matches!(&metadata.source,
                    super_platinum_core::block_actions::BlockElementSource::UnsupportedControl { kind }
                        if kind == "checkboxes")
            }) {
                rsx! {super::message_checkboxes::MessageCheckboxGroup {
                    state,source:source.clone(),metadata:metadata.as_ref().clone(),
                    accessibility_label:metadata.accessibility_label.clone().unwrap_or_default(),
                }}
            } else if let Some(source) = source.filter(|_| {
                matches!(&metadata.source,
                    super_platinum_core::block_actions::BlockElementSource::UnsupportedControl { kind }
                        if kind == "multi_static_select")
            }) {
                rsx! {super::message_multi::MessageMultiSelect {
                    state,source:source.clone(),metadata:metadata.as_ref().clone(),
                    accessibility_label:metadata.accessibility_label.clone().unwrap_or_default(),
                }}
            } else if let Some(url) = url.as_ref().filter(|_| {
                matches!(
                    metadata.confirm,
                    super_platinum_core::block_actions::BlockConfirmationState::Missing
                )
            }) {
                let destination = url.clone();
                rsx! {button {
                    class:"{style.class()}",title:"{url}","aria-label":"{accessible_label}",
                    onclick:move |_|open_link(state, &destination),"{label}"
                }}
            } else {
                rsx! {button {
                    class:"{style.class()} inert",disabled:true,
                    title:"This action is unavailable here", "aria-label":"{accessible_label}","{label}"
                }}
            }
        }

        RichNode::InlineImage { id, alt } => {
            rsx! {
                MediaImage {
                    state, id:id.clone(), media_epoch, compact:true,
                    class: "block-inline-image",
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
                        MediaImage {
                            state, id:id.clone(), media_epoch,
                            class: "block-image-media",
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
                                for node in item { {rich_node_with_source(node, media_epoch, state, source, home, list)} }
                            }
                        }
                    }
                } else {
                    ul { class: "block-list", style: "{indent}",
                        for (index, item) in items.iter().enumerate() {
                            li { key: "li-{index}",
                                for node in item { {rich_node_with_source(node, media_epoch, state, source, home, list)} }
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
        file: None,
        id: id.clone(),
        name: name.to_owned(),
        download_name: name.to_owned(),
        mime: mime.to_owned(),
    }
}

fn open_viewer(mut state: Signal<ShellState>, viewer: crate::state::ViewerVm) {
    let mut shell = state.write();
    if shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t))
        .is_some_and(|w| {
            viewer
                .file
                .as_ref()
                .map(|f| &f.id)
                .is_some_and(|id| w.deleted_files.contains(id))
        })
    {
        shell.show_toast("This file has been deleted.");
        return;
    }
    // Full-resolution bytes are deferred until exactly this moment.
    shell.media.request(&viewer.id);
    shell.viewer = Some(viewer);
    shell.overlay = Some(Overlay::Viewer);
    crate::file_preview::refresh::opened(&mut shell);
}

fn open_link(state: Signal<ShellState>, url: &str) {
    crate::slack_links::open(state, url);
}

fn open_channel(mut state: Signal<ShellState>, channel: &str) {
    let index = state
        .read()
        .channels
        .iter()
        .position(|candidate| candidate.id == channel);
    if let Some(index) = index {
        state
            .write()
            .select_channel(index, crate::state::ChannelOpen::Global);
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
    attachment_embeds_with_list(state, media_epoch, attachments, None)
}

pub(super) fn attachment_embeds_for_list(
    state: Signal<ShellState>,
    media_epoch: u64,
    attachments: &[AttachmentVm],
    source: &ListFileSource,
) -> Element {
    attachment_embeds_with_list(state, media_epoch, attachments, Some(source))
}

fn attachment_embeds_with_list(
    state: Signal<ShellState>,
    media_epoch: u64,
    attachments: &[AttachmentVm],
    list: Option<&ListFileSource>,
) -> Element {
    rsx! {
        div { class: "message-attachments",
            for attachment in attachments {
                {attachment_embed(state, media_epoch, attachment, list)}
            }
        }
    }
}

fn attachment_embed(
    state: Signal<ShellState>,
    media_epoch: u64,
    attachment: &AttachmentVm,
    list: Option<&ListFileSource>,
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
                        for node in &attachment.pretext { {rich_node_with_source(node, media_epoch, state, None, None, list)} }
                    }
                }
                if let Some(service) = attachment.service.as_ref() {
                    div { class: "attachment-service",
                        if let Some(icon) = attachment.service_icon.as_ref() {
                            MediaImage { state, id:icon.clone(), media_epoch, compact:true, fallback:rsx!{span {}} }
                        }
                        span { "{service}" }
                    }
                }
                if let Some(author) = attachment.author.as_ref() {
                    div { class: "attachment-author",
                        if let Some(icon) = author.icon.as_ref() {
                            MediaImage { state, id:icon.clone(), media_epoch, compact:true, fallback:rsx!{span {}} }
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
                                    open_link(state, &link);
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
                        for node in &attachment.body { {rich_node_with_source(node, media_epoch, state, None, None, list)} }
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
                                    for node in value { {rich_node_with_source(node, media_epoch, state, None, None, list)} }
                                }
                            }
                        }
                    }
                }
                if !attachment.files.is_empty() {
                    div { class: "attachment-files",
                        for node in &attachment.files { {rich_node_with_source(node, media_epoch, state, None, None, list)} }
                    }
                }
                if let Some(thumb) = attachment.thumb.as_ref() {
                    MediaImage { state, id:thumb.clone(), media_epoch, class:"attachment-thumb", compact:true }
                }
                if let Some((image, size)) = attachment.image.as_ref() {
                    {
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
                                MediaImage { state, id:image.clone(), media_epoch, style:ratio }
                            }
                        }
                    }
                }
                if let Some(footer) = attachment.footer.as_ref() {
                    div { class: "attachment-footer",
                        if let Some(icon) = footer.icon.as_ref() {
                            MediaImage { state, id:icon.clone(), media_epoch, compact:true, fallback:rsx!{span {}} }
                        }
                        {
                            let channel = footer.channel_id.clone();
                            rsx! {
                                span { class: "attachment-footer-origin",
                                    "{footer.lead}"
                                    if let Some(label) = footer.channel_label.as_ref() {
                                        " "
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
                                    open_link(state, &url);
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

mod reactions;
pub(crate) use reactions::{reactions_list_comments, reactions_read_only, reactions_row};

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
                        MediaImage {state,id:avatar.clone(),media_epoch,compact:true,fallback:rsx! {"{initials}"}}
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
                {crate::icons::icon(crate::icons::Icon::Close, "icon sm")}
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
