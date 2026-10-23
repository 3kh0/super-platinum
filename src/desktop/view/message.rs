//! One message row, shared by the channel timeline and the thread pane.
//!
//! Slack renders the same message with the same hover toolbar wherever it
//! appears — the channel, a thread, the pane Activity opens beside its list
//! (CDP-verified against the real client). Keeping one component here is what
//! stops the surfaces from drifting apart: the thread pane used to render its
//! own stripped-down row, so a reply opened from Activity had no way to be
//! edited, deleted, or replied to.

use crate::media::MediaUri;
use dioxus::prelude::*;

mod delivery;

use super::rich::{
    attachment_embeds, attachment_embeds_for_list, message_plain_text, pending_attachment_strip,
    reactions_row, reply_bar, rich_node, rich_node_for_list, rich_node_for_message,
};
use crate::state::{MessageVm, RichNode, ShellState};

/// Which surface a row is being rendered into.
///
/// The two differ in what a row can *do*, not in how it looks: a thread reply
/// has no "Reply" button, because it is already in the thread it would open,
/// and it carries no reply bar of its own.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowSurface {
    Timeline,
    Thread,
    /// An independently owned thread reader has no timeline mutation authority.
    ReadOnlyThread,
    /// List comments own only their loaded reaction toggle, never timeline actions.
    ListComments {
        generation: u64,
    },
}

impl RowSurface {
    fn in_thread(self) -> bool {
        matches!(
            self,
            Self::Thread | Self::ReadOnlyThread | Self::ListComments { .. }
        )
    }
    fn read_only_actions(self) -> bool {
        matches!(self, Self::ReadOnlyThread | Self::ListComments { .. })
    }
}

/// The hover toolbar.
///
/// Out of flow and outside `.message-meta`: a toolbar that joins the header row
/// on hover reflows — and wraps to a second line in a narrow pane — resizing
/// the message under the pointer. Compact rows get one this way too.
fn message_tools(
    mut state: Signal<ShellState>,
    message: &MessageVm,
    channel_id: &str,
    surface: RowSurface,
) -> Element {
    if surface.read_only_actions() {
        return rsx! {};
    }
    let shortcut_source = {
        let shell = state.read();
        let bag = if surface.in_thread() {
            shell
                .thread_root
                .clone()
                .map(|root| crate::message_buttons::source::Bag::Thread { root })
        } else {
            Some(crate::message_buttons::source::Bag::Timeline)
        };
        bag.and_then(|bag| {
            crate::message_buttons::source::Source::capture(&shell, message, channel_id, bag).ok()
        })
    };
    let can_reply = !surface.in_thread();
    if message.pending {
        return rsx! {};
    }
    let pinned = crate::pins::is_pinned(&state.read(), channel_id, message);
    let pin_busy = crate::pins::visible_state(&state.read())
        .is_some_and(|p| p.is_busy(channel_id, &message.ts));
    let saved_key = super_platinum_core::later::SavedKey::message(channel_id, &message.id);
    let saved = crate::later::visible_state(&state.read())
        .is_some_and(|later| later.items.contains_key(&saved_key));
    let saving = crate::later::visible_state(&state.read())
        .is_some_and(|later| later.busy.contains(&saved_key));
    rsx! {
        span { class: "message-tools",
            ondoubleclick: |event| event.stop_propagation(),
            button {
                title: "Add a reaction", "aria-label": "Add a reaction",
                onclick: {
                    let channel = channel_id.to_owned();
                    let ts = message.id.clone();
                    move |_| crate::reaction_picker::open(state, channel.clone(), ts.clone())
                },
                {crate::icons::icon(crate::icons::Icon::AddReaction, "icon sm")}
            }
            if let Some(source)=shortcut_source {
                button{title:"Message shortcuts","aria-label":"Message shortcuts",
                    onclick:move |_|crate::message_buttons::shortcuts::open_message(state,source.clone()),
                    {crate::icons::icon(crate::icons::Icon::GridView,"icon sm")}
                }
            }
            if can_reply {
                button {
                    onclick: {
                        let root = message.broadcast_root.clone().unwrap_or_else(||message.id.clone());
                        let target = message.broadcast_root.as_ref().map(|_|message.id.clone());
                        let channel = channel_id.to_owned();
                        let owner = thread_open_owner(&state.read());
                        move |_| {
                            open_row_thread(state, &owner, channel.clone(), root.clone(), target.clone(), false);
                        }
                    },
                    "Reply"
                }
            }
            button {
                title:if saved {"Remove from Later"} else {"Save for later"},
                "aria-label":if saved {"Remove from Later"} else {"Save for later"},
                class:if saved {"is-saved"}else{""},
                disabled:saving,
                onclick:move |_|crate::later::change(state,saved_key.clone(),if saved {super_platinum_core::later::SavedAction::Remove}else{super_platinum_core::later::SavedAction::Save}),
                {crate::icons::icon(crate::icons::Icon::Bookmark,"icon sm")}
            }
            button {
                title:if pinned {"Unpin from conversation"}else{"Pin to conversation"},
                "aria-label":if pinned {"Unpin from conversation"}else{"Pin to conversation"},
                class:if pinned {"is-saved"}else{""}, disabled:pin_busy,
                onclick:{let channel=channel_id.to_owned();let ts=message.ts.clone();move |_|crate::pins::change(state,channel.clone(),ts.clone(),!pinned)},
                {crate::icons::icon(crate::icons::Icon::Pin,"icon sm")}
            }
            button {
                title:"Forward message", "aria-label":"Forward message",
                onclick:{let channel=channel_id.to_owned();let message=message.clone();move |_|crate::forward::open(state,channel.clone(),message.clone())},
                {crate::icons::icon(crate::icons::Icon::Forward,"icon sm")}
            }
            if surface.in_thread() {
                button {
                    title:"Mark unread", "aria-label":"Mark unread",
                    disabled:crate::thread_unread::busy(&state.read()),
                    onclick:{let ts=message.ts.clone();move |_|crate::thread_unread::change(state,Some(ts.clone()))},
                    {crate::icons::icon(crate::icons::Icon::Unreads,"icon sm")}
                }
            } else {
                button {
                    title:"Mark unread", "aria-label":"Mark unread",
                    disabled:crate::unread::busy(&state.read(), channel_id),
                    onclick:{let channel=channel_id.to_owned();let ts=message.ts.clone();move |_|crate::unread::change(state, channel.clone(), Some(ts.clone()))},
                    {crate::icons::icon(crate::icons::Icon::Unreads,"icon sm")}
                }
            }
            if message.is_own {
                button {
                    onclick: {
                        let channel = channel_id.to_owned();
                        let ts = message.id.clone();
                        let text = message_plain_text(message);
                        move |_| state.write().start_edit(channel.clone(), ts.clone(), text.clone())
                    },
                    "Edit"
                }
                button {
                    onclick: {
                        let channel = channel_id.to_owned();
                        let ts = message.id.clone();
                        move |_| { spawn(crate::bootstrap::delete_message(state, channel.clone(), ts.clone())); }
                    },
                    "Delete"
                }
            }
        }
    }
}

/// The author line, with the hover card and profile pane wired to the name.
fn message_meta(
    mut state: Signal<ShellState>,
    snapshot: &ShellState,
    message: &MessageVm,
) -> Element {
    rsx! {
        div { class: "message-meta",
            strong {
                ondoubleclick: |event| event.stop_propagation(),
                onmouseenter: {
                    let user = message.user_id.clone();
                    move |event: MouseEvent| {
                        if let Some(user) = user.clone() {
                            let point = event.data().client_coordinates();
                            crate::profile::show_profile_hover(state, user, point.x, point.y);
                        }
                    }
                },
                onmouseleave: move |_| crate::profile::schedule_profile_hover_close(state),
                onclick: {
                    let user = message.user_id.clone();
                    move |_| {
                        if let Some(user) = user.clone() {
                            state.write().profile_hover = None;
                            spawn(crate::bootstrap::open_profile(state, user));
                        }
                    }
                },
                "{message.author}"
            }
            if message.is_app { span { class: "app-badge", "APP" } }
            time { "{message.timestamp}" }
            if message.edited { span { "(edited)" } }
            if message.pending {
                span { class: "pending-label",
                    "{snapshot.pending_delivery_label(&message.id)}"
                }
            }
        }
    }
}

fn message_avatar(
    mut state: Signal<ShellState>,
    snapshot: &ShellState,
    message: &MessageVm,
) -> Element {
    rsx! {
        button {
            class: "avatar",
            ondoubleclick: |event| event.stop_propagation(),
            onmouseenter: {
                let user = message.user_id.clone();
                move |event: MouseEvent| {
                    if let Some(user) = user.clone() {
                        let point = event.data().client_coordinates();
                        crate::profile::show_profile_hover(state, user, point.x, point.y);
                    }
                }
            },
            onmouseleave: move |_| crate::profile::schedule_profile_hover_close(state),
            onclick: {
                let user = message.user_id.clone();
                move |_| {
                    if let Some(user) = user.clone() {
                        state.write().profile_hover = None;
                        spawn(crate::bootstrap::open_profile(state, user));
                    }
                }
            },
            // Initials until the bytes land: an `img` with nothing behind it
            // paints the platform's broken-image icon.
            if let Some(avatar) = message.avatar.as_ref().filter(|avatar| snapshot.media.is_ready(avatar)) {
                super::media_image::MediaImage {
                    state, id:avatar.clone(), media_epoch:snapshot.media_epoch,
                    alt:message.author.clone(), compact:true,
                    fallback:rsx! { "{message.avatar_initials}" }
                }
            } else {
                "{message.avatar_initials}"
            }
            if let Some(team) = &message.external_team {
                span {
                    class: "avatar-team-icon",
                    title: "{team.name}",
                    "aria-label": "{team.name}",
                    if let Some(icon) = team.icon.as_ref().filter(|icon| snapshot.media.is_ready(icon)) {
                        img { src: "{icon.webview_uri_at(snapshot.media_epoch)}", alt: "" }
                    } else {
                        "{team.initials}"
                    }
                }
            }
        }
    }
}

/// Renders one message.
///
/// `index` is the row's position in the timeline, used by the scroll measurer;
/// thread replies are not part of that window and pass `None`.
pub(crate) fn message_row(
    mut state: Signal<ShellState>,
    snapshot: &ShellState,
    message: &MessageVm,
    channel_id: &str,
    index: Option<usize>,
    surface: RowSurface,
    action_bag: Option<crate::message_buttons::source::Bag>,
) -> Element {
    // Text-only rows never need canonical bag scans or action revision clones.
    // This allocation-free walk short-circuits at the first supported app control or identified file.
    let read_only = surface.read_only_actions();
    let action_source = action_bag
        .filter(|_| !read_only)
        .filter(|_| !message.pending && body_has_owned_control(&message.body))
        .and_then(|bag| {
            crate::message_buttons::source::Source::capture(snapshot, message, channel_id, bag).ok()
        });
    let list_source = match surface {
        RowSurface::ListComments { generation } => Some(super::rich::ListFileSource::capture(
            snapshot, generation, message,
        )),
        _ => None,
    };
    let in_thread = surface.in_thread();
    let compact = message.compact && !message.pending;
    let class = if in_thread {
        if compact {
            "message thread-message compact"
        } else if message.pending {
            "message thread-message pending"
        } else {
            "message thread-message"
        }
    } else if snapshot.message_arrivals.contains_key(&message.id) {
        if compact {
            "message compact entering"
        } else {
            "message entering"
        }
    } else if compact {
        "message compact"
    } else if message.pending {
        "message pending"
    } else {
        "message"
    };
    let editing = !read_only
        && snapshot
            .core
            .editing
            .as_ref()
            .is_some_and(|(channel, ts)| channel == channel_id && ts == &message.id);
    // Double-clicking a channel message opens its thread with the reply box
    // focused. A thread reply is already where that would go, a row being
    // edited needs its clicks for the editor, and a pending message has no
    // `ts` Slack would recognise as a thread root yet.
    let opens_thread = !in_thread && !editing && !message.pending;
    rsx! {
        article {
            class: "{class}",
            tabindex: "0",
            "aria-label":"{message.author}, {message.timestamp}",
            "data-message-index": if let Some(index) = index { "{index}" },
            "data-message-id": "{message.id}",
            ondoubleclick: {
                let channel = channel_id.to_owned();
                let root = message.broadcast_root.clone().unwrap_or_else(||message.id.clone());
                let target = message.broadcast_root.as_ref().map(|_|message.id.clone());
                let owner = thread_open_owner(snapshot);
                move |_| {
                    if opens_thread {
                        open_row_thread(state, &owner, channel.clone(), root.clone(), target.clone(), true);
                    }
                }
            },
            if compact {
                div { class: "compact-gutter", title: "{message.timestamp}", "{message.timestamp}" }
            } else {
                {message_avatar(state, snapshot, message)}
            }
            {message_tools(state, message, channel_id, surface)}
            div { class: "message-content",
                if !in_thread { {broadcast_context(state,snapshot,message,channel_id)} }
                if !read_only && crate::pins::is_pinned(snapshot,channel_id,message) {
                    div {class:"message-pin-label",{crate::icons::icon(crate::icons::Icon::Pin,"icon sm")} "Pinned"}
                }
                if !compact {
                    {message_meta(state, snapshot, message)}
                }
                if editing {
                    div { class: "message-editor",
                        textarea {
                            initial_value: "{snapshot.core.edit_composer.text}",
                            oninput: move |event| {
                                let value = event.value();
                                let end = value.len();
                                let mut shell = state.write();
                                shell.core.edit_composer.text = value;
                                shell.core.edit_composer.set_selection(end, end);
                            }
                        }
                        button {
                            onclick: move |_| { spawn(crate::bootstrap::save_edit(state)); },
                            "Save"
                        }
                        button { onclick: move |_| state.write().cancel_edit(), "Cancel" }
                    }
                } else {
                    div { class: if message.is_system { "message-body message-system" } else { "message-body" },
                        for node in &message.body {
                            {if let Some(source)=list_source.as_ref() {
                                rich_node_for_list(node,snapshot.media_epoch,state,source)
                            } else if let Some(source)=action_source.as_ref() {
                                rich_node_for_message(node,snapshot.media_epoch,state,source)
                            } else {rich_node(node,snapshot.media_epoch,state)}}
                        }
                        if compact && message.edited {span {class:"message-edited"," (edited)"}}
                    }
                    if !message.attachments.is_empty() {
                        {if let Some(source)=list_source.as_ref() {
                            attachment_embeds_for_list(state, snapshot.media_epoch, &message.attachments,source)
                        } else {attachment_embeds(state, snapshot.media_epoch, &message.attachments)}}
                    }
                    if let Some(attachments) = (!read_only).then(|| snapshot.pending_attachments_for_message(&message.id)).flatten() {
                        {pending_attachment_strip(attachments, snapshot.upload_ui_epoch)}
                        {delivery::row(state,snapshot,&message.id)}
                    }
                    if !read_only {
                        {crate::app_home::about::management::permissions::finish_up::share_delivery_row(state,snapshot,channel_id,&message.id)}
                    }
                }
                if !message.reactions.is_empty() || matches!(surface, RowSurface::ListComments { .. }) {
                    div { class: "message-actions",
                        ondoubleclick: |event| event.stop_propagation(),
                        {match surface {
                            RowSurface::ListComments { generation } => super::rich::reactions_list_comments(state, snapshot, generation, message),
                            RowSurface::ReadOnlyThread => super::rich::reactions_read_only(state, &snapshot.media, snapshot.media_epoch, message),
                            _ => reactions_row(state, &snapshot.media, snapshot.media_epoch, channel_id, message),
                        }}
                    }
                }
                if !in_thread && message.reply_count > 0 {
                    {reply_bar(state, &snapshot.media, snapshot.media_epoch, channel_id, message)}
                }
            }
        }
    }
}

type ThreadOpenOwner = (
    u64,
    Option<super_platinum_core::config::AccountId>,
    Option<String>,
    u64,
);

fn thread_open_owner(shell: &ShellState) -> ThreadOpenOwner {
    (
        shell.core.account_epoch,
        shell.core.active_account.clone(),
        shell.core.active_team.clone(),
        shell.channel_generation,
    )
}

/// Admit synchronously, then load only that already-opened thread visit. This
/// keeps focus immediate without letting a queued task navigate a newer owner.
fn open_row_thread(
    mut state: Signal<ShellState>,
    owner: &ThreadOpenOwner,
    channel: String,
    root: String,
    target: Option<String>,
    focus: bool,
) {
    let scope = {
        let mut shell = state.write();
        if thread_open_owner(&shell) != *owner
            || shell.core.active_channel.as_ref() != Some(&channel)
        {
            return;
        }
        shell.begin_thread(root.clone(), target.clone());
        shell.remember_surface();
        shell.thread_scope()
    };
    if focus {
        super::composer::focus_field_when_mounted("thread-composer");
    }
    spawn(crate::bootstrap::load_started_thread(
        state, channel, root, target, scope,
    ));
}

fn broadcast_context(
    state: Signal<ShellState>,
    snapshot: &ShellState,
    message: &MessageVm,
    channel: &str,
) -> Element {
    let Some(root) = message.broadcast_root.clone() else {
        return rsx! {};
    };
    let channel = channel.to_owned();
    let reply = message.id.clone();
    let owner = thread_open_owner(snapshot);
    let preview = message.broadcast_parent_preview.clone();
    let pending = message.pending;
    rsx! {
        button {class:"message-broadcast-context",r#type:"button",disabled:message.pending,
            "data-thread-root":"{root}","data-thread-reply":"{reply}",
            title:preview.clone().unwrap_or_else(||"Open parent thread".into()),
            ondoubleclick:move |event|event.stop_propagation(),
            onclick:move |_| {
                if !pending {
                    open_row_thread(state, &owner, channel.clone(), root.clone(), Some(reply.clone()), false);
                }
            },
            {crate::icons::icon(crate::icons::Icon::Reply,"icon sm")}
            span {class:"message-broadcast-label","Replied to a thread"}
            if let Some(preview)=preview.as_ref() {span {class:"message-broadcast-preview","{preview}"}}
        }
    }
}

fn body_has_owned_control(nodes: &[RichNode]) -> bool {
    nodes.iter().any(node_has_owned_control)
}

fn node_has_owned_control(node: &RichNode) -> bool {
    match node {
        RichNode::Media { file: Some(_), .. } => true,
        RichNode::Button { metadata, .. } => match &metadata.source {
            super_platinum_core::block_actions::BlockElementSource::Button => true,
            super_platinum_core::block_actions::BlockElementSource::UnsupportedControl { kind } => {
                matches!(kind.as_str(), "static_select" | "radio_buttons")
            }
        },
        RichNode::Paragraph(nodes)
        | RichNode::Quote(nodes)
        | RichNode::Header(nodes)
        | RichNode::Context(nodes)
        | RichNode::Actions(nodes) => body_has_owned_control(nodes),
        RichNode::Section {
            text,
            fields,
            accessory,
        } => {
            body_has_owned_control(text)
                || fields.iter().any(|field| body_has_owned_control(field))
                || accessory.as_deref().is_some_and(node_has_owned_control)
        }
        RichNode::List { items, .. } => items.iter().any(|item| body_has_owned_control(item)),
        _ => false,
    }
}

#[cfg(test)]
mod button_presence_tests {
    use super::*;
    use crate::model::ButtonStyle;
    use super_platinum_core::block_actions::{
        BlockConfirmationState, BlockElementMetadata, BlockElementSource,
    };

    #[test]
    fn list_comments_keep_thread_layout_without_timeline_action_authority() {
        for surface in [
            RowSurface::ReadOnlyThread,
            RowSurface::ListComments { generation: 7 },
        ] {
            assert!(surface.in_thread());
            assert!(surface.read_only_actions());
        }
        assert!(RowSurface::Thread.in_thread());
        assert!(!RowSurface::Thread.read_only_actions());
        assert!(!RowSurface::Timeline.in_thread());
        assert!(!RowSurface::Timeline.read_only_actions());
    }

    fn button(source: BlockElementSource) -> RichNode {
        RichNode::Button {
            label: "Review".into(),
            url: None,
            style: ButtonStyle::Default,
            metadata: Box::new(BlockElementMetadata {
                source,
                block_id: Some("review".into()),
                action_id: Some("open".into()),
                value: None,
                confirm: BlockConfirmationState::Missing,
                accessibility_label: None,
            }),
        }
    }

    #[test]
    fn ordinary_text_links_and_inert_controls_need_no_source_capture() {
        let body = vec![
            RichNode::Paragraph(vec![
                RichNode::Text("A normal message".into()),
                RichNode::Link {
                    label: "Reference".into(),
                    url: "https://example.test".into(),
                },
            ]),
            RichNode::Actions(vec![button(BlockElementSource::UnsupportedControl {
                kind: "multi_static_select".into(),
            })]),
        ];
        assert!(!body_has_owned_control(&body));
    }

    #[test]
    fn supported_static_select_keeps_ownership_even_without_a_button() {
        assert!(body_has_owned_control(&[RichNode::Actions(vec![button(
            BlockElementSource::UnsupportedControl {
                kind: "static_select".into()
            }
        )])]));
    }

    #[test]
    fn supported_radio_group_keeps_ownership_without_a_button() {
        assert!(body_has_owned_control(&[RichNode::Actions(vec![button(
            BlockElementSource::UnsupportedControl {
                kind: "radio_buttons".into()
            }
        )])]));
    }

    #[test]
    fn app_buttons_in_every_recursive_body_container_keep_ownership() {
        let button = button(BlockElementSource::Button);
        let containers = vec![
            RichNode::Paragraph(vec![button.clone()]),
            RichNode::Quote(vec![button.clone()]),
            RichNode::Header(vec![button.clone()]),
            RichNode::Context(vec![button.clone()]),
            RichNode::Actions(vec![button.clone()]),
            RichNode::Section {
                text: vec![button.clone()],
                fields: vec![],
                accessory: None,
            },
            RichNode::Section {
                text: vec![],
                fields: vec![vec![button.clone()]],
                accessory: None,
            },
            RichNode::Section {
                text: vec![],
                fields: vec![],
                accessory: Some(Box::new(button.clone())),
            },
            RichNode::List {
                ordered: false,
                indent: 0,
                offset: 0,
                items: vec![vec![button]],
            },
        ];
        for container in containers {
            assert!(body_has_owned_control(&[container]));
        }
    }
}
