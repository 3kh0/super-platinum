use dioxus::prelude::*;
use serde_json::Value;

use crate::state::{ChannelOpen, PresenceVm, ProfileHoverVm, ShellState};
use super_platinum_core::MediaAssetId;

#[derive(Clone)]
struct ProfileDisplay {
    user_id: String,
    name: String,
    handle: String,
    title: String,
    pronouns: String,
    status: String,
    status_glyph: String,
    status_image: Option<MediaAssetId>,
    local_time: String,
    email: String,
    phone: String,
    start_date: String,
    initials: String,
    avatar: Option<MediaAssetId>,
    presence: PresenceVm,
    deactivated: bool,
    is_self: bool,
    is_vip: bool,
    team_name: String,
    recent_dms: Vec<RecentDm>,
    has_more_dms: bool,
    fields: Vec<ProfileField>,
}

#[derive(Clone)]
struct RecentDm {
    id: String,
    label: String,
    initials: String,
    avatar: Option<MediaAssetId>,
}

#[derive(Clone)]
struct ProfileField {
    id: String,
    label: String,
    value: Vec<crate::state::RichNode>,
}

pub(crate) fn profile_hover_card(
    mut state: Signal<ShellState>,
    snapshot: &ShellState,
    hover: &ProfileHoverVm,
) -> Element {
    let Some(profile) = profile_display(snapshot, &hover.user_id) else {
        return rsx! {};
    };
    let style = format!(
        "--profile-hover-x:{};--profile-hover-y:{}",
        hover.x, hover.y
    );
    let user_for_profile = profile.user_id.clone();
    let user_for_avatar = profile.user_id.clone();
    let user_for_message = profile.user_id.clone();
    let user_for_vip = profile.user_id.clone();
    let can_vip = !profile.is_self && !profile.deactivated;
    let vip_label = if profile.is_vip { "Remove VIP" } else { "VIP" };
    let message_icon = crate::icons::message_uri();
    let vip_icon = crate::icons::user_add_uri();
    let clock_icon = crate::icons::clock_uri();
    rsx! {
        div {
            class: "profile-hover",
            style: "{style}",
            role: "dialog",
            "aria-label": "{profile.name} profile preview",
            onmouseenter: move |_| state.write().hold_profile_hover(),
            onmouseleave: move |_| {
                let mut shell = state.write();
                shell.profile_hover_card_active = false;
                shell.profile_hover = None;
            },
            div { class: "profile-hover-primary",
                button {
                    class: "profile-hover-avatar",
                    title: "Open {profile.name}'s profile",
                    onclick: move |_| { spawn(crate::bootstrap::open_profile(state, user_for_avatar.clone())); },
                    {profile_avatar(&profile, snapshot.media_epoch, "profile-hover-avatar-image", profile.avatar.as_ref().is_some_and(|avatar| snapshot.media.is_ready(avatar)))}
                }
                div { class: "profile-hover-identity",
                    button {
                        class: "profile-name-button",
                        onclick: move |_| { spawn(crate::bootstrap::open_profile(state, user_for_profile.clone())); },
                        strong { "{profile.name}" }
                        span { class: presence_class(profile.presence), "aria-label": "{profile.presence.label()}" }
                    }
                    if !profile.title.is_empty() { span { class: "profile-hover-title", "{profile.title}" } }
                    if !profile.pronouns.is_empty() { span { class: "profile-hover-pronouns", "{profile.pronouns}" } }
                }
            }
            div { class: "profile-hover-secondary",
                if !profile.status.is_empty() || !profile.status_glyph.is_empty() || profile.status_image.is_some() {
                    div { class: "profile-info-row profile-status-row",
                        {status_icon(&profile, snapshot.media_epoch, profile.status_image.as_ref().is_none_or(|image| snapshot.media.is_ready(image)))}
                        span { "{profile.status}" }
                    }
                }
                if !profile.local_time.is_empty() {
                    div { class: "profile-info-row",
                        img { class: "profile-row-icon", src: "{clock_icon}", alt: "" }
                        span { "{profile.local_time}" }
                    }
                }
                div { class: "profile-actions profile-hover-actions",
                    button {
                        class: "profile-action profile-action-message",
                        onclick: move |_| { spawn(crate::bootstrap::open_profile_dm(state, user_for_message.clone())); },
                        img { src: "{message_icon}", alt: "" }
                        span { "Message" }
                    }
                    if can_vip {
                        button {
                            class: if profile.is_vip { "profile-action active" } else { "profile-action" },
                            disabled: snapshot.profile_vip_loading,
                            onclick: move |_| { spawn(crate::bootstrap::toggle_profile_vip(state, user_for_vip.clone())); },
                            img { src: "{vip_icon}", alt: "" }
                            span { "{vip_label}" }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn profile_pane(mut state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let Some(user_id) = snapshot.profile_user.as_deref() else {
        return rsx! {};
    };
    let Some(profile) = profile_display(snapshot, user_id) else {
        return rsx! {};
    };
    let loading = snapshot
        .core
        .profile_pane
        .as_ref()
        .is_some_and(|pane| pane.user == user_id && pane.loading);
    let error = snapshot
        .core
        .profile_pane
        .as_ref()
        .filter(|pane| pane.user == user_id)
        .and_then(|pane| pane.error.clone());
    let user_for_message = profile.user_id.clone();
    let user_for_vip = profile.user_id.clone();
    let user_for_files = profile.user_id.clone();
    let name_for_all = profile.name.clone();
    let display_name = if profile.handle.is_empty() {
        profile.name.clone()
    } else {
        format!("@{}", profile.handle)
    };
    let member_id = profile.user_id.clone();
    let profile_link = snapshot
        .core
        .active_team
        .as_ref()
        .map(|team| format!("slack://user?team={team}&id={}", profile.user_id))
        .unwrap_or_default();
    let share_link = profile_link.clone();
    let can_vip = !profile.is_self && !profile.deactivated;
    let vip_label = if profile.is_vip { "Remove VIP" } else { "VIP" };
    let message_icon = crate::icons::message_uri();
    let vip_icon = crate::icons::user_add_uri();
    let more_icon = crate::icons::more_uri();
    let clock_icon = crate::icons::clock_uri();
    let pane_width = snapshot.profile_pane_width.max(200.0);
    let pane_style = format!("--profile-pane-width:{pane_width}px");
    rsx! {
        aside {
            class: if pane_width < 300.0 { "profile-pane profile-pane-narrow" } else { "profile-pane" },
            style: "{pane_style}",
            role: "dialog",
            "aria-label": "{profile.name} Profile",
            div {
                class: "profile-resize-handle",
                title: "Resize profile",
                "aria-label": "Resize profile panel",
                onmousedown: move |event: MouseEvent| {
                    event.prevent_default();
                    let point = event.data().client_coordinates();
                    start_profile_resize(state, point.x, pane_width);
                }
            }
            header { class: "profile-pane-header",
                h2 { "Profile" }
                button {
                    class: "profile-close",
                    title: "Close profile",
                    "aria-label": "Close profile",
                    onclick: move |_| state.write().close_profile(),
                    "×"
                }
            }
            div {
                class: "profile-pane-scroll",
                onclick: move |_| state.write().profile_menu_open = false,
                div { class: "profile-pane-hero",
                    div { class: "profile-pane-avatar",
                        {profile_avatar(&profile, snapshot.media_epoch, "profile-pane-avatar-image", profile.avatar.as_ref().is_some_and(|avatar| snapshot.media.is_ready(avatar)))}
                        if loading { span { class: "profile-avatar-loading" } }
                    }
                }
                section { class: "profile-pane-section profile-main-section",
                    div { class: "profile-pane-name-row",
                        h1 { "{profile.name}" }
                        if profile.is_vip { span { class: "profile-vip-badge", "VIP" } }
                    }
                    if !profile.title.is_empty() { p { class: "profile-title", "{profile.title}" } }
                    if !profile.pronouns.is_empty() { p { class: "profile-pronouns", "{profile.pronouns}" } }
                    if profile.deactivated {
                        div { class: "profile-info-row profile-deactivated", "Deactivated account" }
                    } else {
                        div { class: "profile-info-row profile-presence-row",
                            span { class: presence_class(profile.presence) }
                            span { "{profile.presence.label()}" }
                        }
                    }
                    if !profile.status.is_empty() || !profile.status_glyph.is_empty() || profile.status_image.is_some() {
                        div { class: "profile-info-row profile-status-row",
                            {status_icon(&profile, snapshot.media_epoch, profile.status_image.as_ref().is_none_or(|image| snapshot.media.is_ready(image)))}
                            span { "{profile.status}" }
                        }
                    }
                    if !profile.local_time.is_empty() {
                        div { class: "profile-info-row",
                            img { class: "profile-row-icon", src: "{clock_icon}", alt: "" }
                            span { "{profile.local_time}" }
                        }
                    }
                    if let Some(error) = error { p { class: "profile-load-error", "Some profile details could not be refreshed: {error}" } }
                    div { class: "profile-actions profile-pane-actions",
                        button {
                            class: "profile-action profile-action-message",
                            onclick: move |event| {
                                event.stop_propagation();
                                spawn(crate::bootstrap::open_profile_dm(state, user_for_message.clone()));
                            },
                            img { src: "{message_icon}", alt: "" }
                            span { "Message" }
                        }
                        if can_vip {
                            button {
                                class: if profile.is_vip { "profile-action active" } else { "profile-action" },
                                disabled: snapshot.profile_vip_loading,
                                onclick: move |event| {
                                    event.stop_propagation();
                                    spawn(crate::bootstrap::toggle_profile_vip(state, user_for_vip.clone()));
                                },
                                img { src: "{vip_icon}", alt: "" }
                                span { "{vip_label}" }
                            }
                        }
                        div { class: "profile-more-wrap",
                            button {
                                class: "profile-action profile-more-button",
                                title: "More",
                                "aria-label": "More profile actions",
                                onclick: move |event| {
                                    event.stop_propagation();
                                    let open = state.read().profile_menu_open;
                                    state.write().profile_menu_open = !open;
                                },
                                img { src: "{more_icon}", alt: "" }
                            }
                            if snapshot.profile_menu_open {
                                div { class: "profile-more-menu", role: "menu", onclick: move |event| event.stop_propagation(),
                                    button { role: "menuitem", onclick: move |_| copy_text(state, display_name.clone(), "Display name copied"), "Copy display name: {display_name}" }
                                    button { role: "menuitem", onclick: move |_| copy_text(state, share_link.clone(), "Profile link copied for sharing"), "Share contact" }
                                    button { role: "menuitem", onclick: move |_| open_profile_files(state, user_for_files.clone()), "View files" }
                                    hr {}
                                    button { role: "menuitem", onclick: move |_| copy_text(state, member_id.clone(), "Member ID copied"), "Copy member ID" }
                                    button { role: "menuitem", onclick: move |_| copy_text(state, profile_link.clone(), "Profile link copied"), "Copy link to profile" }
                                }
                            }
                        }
                    }
                }
                if !profile.recent_dms.is_empty() {
                    section { class: "profile-pane-section profile-detail-section",
                        h3 { "Recent DMs" }
                        div { class: "profile-recent-dms",
                            for dm in &profile.recent_dms {
                                button {
                                    key: "profile-dm-{dm.id}",
                                    onclick: {
                                        let channel = dm.id.clone();
                                        move |_| open_recent_dm(state, channel.clone())
                                    },
                                    span { class: "profile-recent-avatar",
                                        if let Some(avatar) = dm.avatar.as_ref().filter(|avatar| snapshot.media.is_ready(avatar)) {
                                            img { src: "{avatar.uri_at(snapshot.media_epoch)}", alt: "" }
                                        } else { "{dm.initials}" }
                                    }
                                    span { "{dm.label}" }
                                }
                            }
                        }
                        if profile.has_more_dms || !profile.recent_dms.is_empty() {
                            button { class: "profile-see-all", onclick: move |_| see_all_conversations(state, name_for_all.clone()), "See all conversations with {profile.name}" }
                        }
                    }
                }
                if !profile.fields.is_empty() || !profile.email.is_empty() || !profile.phone.is_empty() || !profile.start_date.is_empty() {
                    section { class: "profile-pane-section profile-detail-section",
                        h3 { "About me" }
                        for field in &profile.fields {
                            div { class: "profile-field", key: "profile-field-{field.id}",
                                strong { "{field.label}" }
                                span { class: "profile-field-value",
                                    for node in &field.value {
                                        {crate::view::rich::rich_node(node, snapshot.media_epoch, state)}
                                    }
                                }
                            }
                        }
                        if !profile.email.is_empty() { div { class: "profile-field", strong { "Email" } a { href: "mailto:{profile.email}", "{profile.email}" } } }
                        if !profile.phone.is_empty() { div { class: "profile-field", strong { "Phone" } a { href: "tel:{profile.phone}", "{profile.phone}" } } }
                        if !profile.start_date.is_empty() { div { class: "profile-field", strong { "Start date" } span { "{profile.start_date}" } } }
                    }
                }
                section { class: "profile-pane-section profile-detail-section profile-shared-workspaces",
                    h3 { "Shared Workspaces" }
                    p { "{profile.team_name}" }
                }
            }
        }
    }
}

pub(crate) fn show_profile_hover(mut state: Signal<ShellState>, user_id: String, x: f64, y: f64) {
    state.write().show_profile_hover(user_id, x, y);
}

pub(crate) fn schedule_profile_hover_close(mut state: Signal<ShellState>) {
    let generation = {
        let mut shell = state.write();
        shell.profile_hover_generation = shell.profile_hover_generation.wrapping_add(1);
        shell.profile_hover_card_active = false;
        shell.profile_hover_generation
    };
    spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(140)).await;
        let should_close = {
            let shell = state.read();
            shell.profile_hover_generation == generation && !shell.profile_hover_card_active
        };
        if should_close {
            state.write().profile_hover = None;
        }
    });
}

fn profile_display(snapshot: &ShellState, user_id: &str) -> Option<ProfileDisplay> {
    let team = snapshot.core.active_team.as_ref()?;
    let workspace = snapshot.core.workspaces.get(team)?;
    let user = workspace.users.get(user_id)?;
    let user_profile = user.profile.as_ref();
    let name = super_platinum_core::state::display_name(Some(user), user_id);
    let handle = user.name.clone().unwrap_or_default();
    let title = user_profile
        .and_then(|profile| profile.title.clone())
        .unwrap_or_default();
    let pronouns = user_profile
        .and_then(|profile| profile.pronouns.clone())
        .unwrap_or_default();
    let status_current = user_profile.is_none_or(|profile| {
        profile.status_expiration.unwrap_or(0) == 0
            || profile.status_expiration.unwrap_or(0) > unix_now()
    });
    let status = status_current
        .then(|| user_profile.and_then(|profile| profile.status_text.clone()))
        .flatten()
        .unwrap_or_default();
    let status_name = status_current
        .then(|| user_profile.and_then(|profile| profile.status_emoji.as_deref()))
        .flatten()
        .unwrap_or_default()
        .trim_matches(':');
    let status_image = workspace
        .custom_emoji_url(status_name)
        .map(|url| snapshot.media.register_emoji(status_name, url));
    let status_glyph =
        if status_image.is_none() && super_platinum_core::state::is_standard_emoji(status_name) {
            super_platinum_core::state::emoji_glyph(status_name)
        } else {
            String::new()
        };
    let avatar = super_platinum_core::state::user_profile_image_url(user)
        .map(|url| snapshot.media.register_avatar(user_id, &url));
    let presence = workspace
        .presence
        .get(user_id)
        .copied()
        .map(PresenceVm::from_core)
        .unwrap_or_default();
    let mut recent_dms = user
        .im_mpim_ids
        .iter()
        .filter_map(|id| {
            snapshot
                .channels
                .iter()
                .find(|channel| &channel.id == id)
                .map(|channel| RecentDm {
                    id: id.clone(),
                    label: channel.name.clone(),
                    initials: channel.avatar_initials.clone(),
                    avatar: channel.avatar.clone(),
                })
        })
        .take(3)
        .collect::<Vec<_>>();
    if recent_dms.is_empty()
        && let Some(channel) = snapshot
            .channels
            .iter()
            .find(|channel| channel.is_im && channel.user_id.as_deref() == Some(user_id))
    {
        recent_dms.push(RecentDm {
            id: channel.id.clone(),
            label: channel.name.clone(),
            initials: channel.avatar_initials.clone(),
            avatar: channel.avatar.clone(),
        });
    }
    let mut schemas = snapshot
        .core
        .profile_fields
        .get(team)
        .cloned()
        .unwrap_or_default();
    schemas.sort_by_key(|field| field.ordering.unwrap_or(i64::MAX));
    let ctx = crate::blocks::BlockCtx::new(workspace, &snapshot.media);
    let fields: Vec<ProfileField> = schemas
        .into_iter()
        .filter(|field| !field.is_hidden)
        .filter_map(|field| {
            let value = user_profile?.fields.get(&field.id)?;
            let label = field.label.filter(|label| !label.trim().is_empty())?;
            if matches!(
                label.to_ascii_lowercase().as_str(),
                "manager" | "title" | "name pronunciation"
            ) {
                return None;
            }
            let value = profile_field_nodes(ctx, field.field_type.as_deref(), value)?;
            Some(ProfileField {
                id: field.id,
                label,
                value,
            })
        })
        .collect();
    let has_start_date = fields
        .iter()
        .any(|field| field.label.eq_ignore_ascii_case("start date"));
    Some(ProfileDisplay {
        user_id: user_id.to_owned(),
        name: name.clone(),
        handle,
        title,
        pronouns,
        status,
        status_glyph,
        status_image,
        local_time: super_platinum_core::state::format_user_local_time(user.tz_offset)
            .unwrap_or_default(),
        email: user_profile
            .and_then(|profile| profile.email.clone())
            .unwrap_or_default(),
        phone: user_profile
            .and_then(|profile| profile.phone.clone())
            .unwrap_or_default(),
        start_date: if has_start_date {
            String::new()
        } else {
            user_profile
                .and_then(|profile| profile.start_date.clone())
                .unwrap_or_default()
        },
        initials: name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "?".into()),
        avatar,
        presence,
        deactivated: user.deleted,
        is_self: workspace.self_user_id == user_id,
        is_vip: workspace.vip_users.contains(user_id),
        team_name: workspace.name.clone(),
        recent_dms,
        has_more_dms: user.has_more_mpims || user.im_mpim_ids.len() > 3,
        fields,
    })
}

fn profile_avatar(
    profile: &ProfileDisplay,
    media_epoch: u64,
    class: &'static str,
    ready: bool,
) -> Element {
    rsx! {
        if let Some(avatar) = profile.avatar.as_ref().filter(|_| ready) {
            img { class: "{class}", src: "{avatar.uri_at(media_epoch)}", alt: "Profile photo for {profile.name}" }
        } else {
            span { class: "profile-avatar-initials", "{profile.initials}" }
        }
    }
}

fn status_icon(profile: &ProfileDisplay, media_epoch: u64, ready: bool) -> Element {
    rsx! {
        if let Some(image) = profile.status_image.as_ref().filter(|_| ready) {
            img { class: "profile-status-emoji", src: "{image.uri_at(media_epoch)}", alt: "" }
        } else if !profile.status_glyph.is_empty() {
            span { class: "profile-status-glyph", "{profile.status_glyph}" }
        }
    }
}

fn start_profile_resize(mut state: Signal<ShellState>, start_x: f64, start_width: f64) {
    spawn(async move {
        let script = format!(
            r#"
            const startX = {start_x};
            const startWidth = {start_width};
            const clamp = x => Math.max(200, Math.min(window.innerWidth - 68, x));
            const moved = event => dioxus.send({{width: clamp(startWidth + startX - event.clientX)}});
            const stopped = event => {{
              document.removeEventListener('mousemove', moved);
              document.removeEventListener('mouseup', stopped);
              document.body.classList.remove('profile-resizing');
              dioxus.send({{done: true, width: clamp(startWidth + startX - event.clientX)}});
            }};
            document.body.classList.add('profile-resizing');
            document.addEventListener('mousemove', moved);
            document.addEventListener('mouseup', stopped);
            "#
        );
        let mut bridge = dioxus::document::eval(&script);
        while let Ok(payload) = bridge.recv::<Value>().await {
            if let Some(width) = payload.get("width").and_then(Value::as_f64) {
                state.write().profile_pane_width = width.max(200.0);
            }
            if payload.get("done").and_then(Value::as_bool) == Some(true) {
                break;
            }
        }
    });
}

fn profile_field_nodes(
    ctx: crate::blocks::BlockCtx<'_>,
    field_type: Option<&str>,
    value: &super_platinum_core::slack::models::ProfileFieldValue,
) -> Option<Vec<crate::state::RichNode>> {
    let raw = profile_field_text(&value.value);
    let alt = value.alt.clone().filter(|value| !value.trim().is_empty());
    let url = profile_field_url(&value.value).or_else(|| {
        raw.as_deref()
            .filter(|value| value.starts_with("https://") || value.starts_with("http://"))
            .map(str::to_owned)
    });
    let is_link = field_type.is_some_and(|kind| kind.to_ascii_lowercase().contains("link"));
    if let Some(url) =
        url.filter(|_| is_link || raw.as_deref().is_some_and(|raw| raw.starts_with("http")))
    {
        return Some(vec![crate::state::RichNode::Link {
            label: alt.unwrap_or_else(|| url.clone()),
            url,
        }]);
    }
    let text = raw.or(alt)?;
    Some(crate::blocks::plain_inline_nodes(ctx, &text))
}

fn profile_field_url(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => {
            (value.starts_with("https://") || value.starts_with("http://")).then(|| value.clone())
        }
        Value::Array(values) => values.iter().find_map(profile_field_url),
        Value::Object(value) => ["url", "link", "value"]
            .into_iter()
            .find_map(|key| value.get(key).and_then(profile_field_url)),
        _ => None,
    }
}

fn presence_class(presence: PresenceVm) -> &'static str {
    match presence {
        PresenceVm::Active => "profile-presence-dot active",
        PresenceVm::Away => "profile-presence-dot away",
        PresenceVm::Unknown => "profile-presence-dot",
    }
}

fn profile_field_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(value) => (!value.trim().is_empty()).then(|| value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(if *value { "Yes" } else { "No" }.into()),
        Value::Array(values) => {
            let values = values
                .iter()
                .filter_map(profile_field_text)
                .collect::<Vec<_>>();
            (!values.is_empty()).then(|| values.join(", "))
        }
        Value::Object(value) => ["label", "text", "value", "name", "url", "link"]
            .into_iter()
            .find_map(|key| value.get(key).and_then(profile_field_text)),
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn open_recent_dm(mut state: Signal<ShellState>, channel: String) {
    let index = {
        state
            .read()
            .channels
            .iter()
            .position(|item| item.id == channel)
    };
    if let Some(index) = index {
        state.write().select_channel(index, ChannelOpen::Global);
        state.write().close_profile();
        spawn(crate::bootstrap::refresh_selected_channel(state));
    }
}

fn see_all_conversations(mut state: Signal<ShellState>, name: String) {
    let mut shell = state.write();
    shell.main_view = crate::state::MainView::Dms;
    shell.dm_query = name;
    shell.close_profile();
}

fn open_profile_files(mut state: Signal<ShellState>, user: String) {
    {
        let mut shell = state.write();
        shell.search_query = format!("from:<@{user}> has:file");
        shell.overlay = Some(crate::state::Overlay::Search);
        shell.profile_menu_open = false;
    }
    spawn(crate::bootstrap::search(state));
}

fn copy_text(mut state: Signal<ShellState>, text: String, toast: &'static str) {
    let value = serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".into());
    dioxus::document::eval(&format!("navigator.clipboard.writeText({value});"));
    let mut shell = state.write();
    shell.profile_menu_open = false;
    shell.show_toast(toast);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_field_text_handles_slack_scalar_and_collection_shapes() {
        assert_eq!(
            profile_field_text(&Value::String("Design".into())).as_deref(),
            Some("Design")
        );
        assert_eq!(
            profile_field_text(&serde_json::json!(["Rust", "Dioxus"])).as_deref(),
            Some("Rust, Dioxus")
        );
        assert_eq!(
            profile_field_text(&serde_json::json!({"label":"March 2"})).as_deref(),
            Some("March 2")
        );
        assert_eq!(profile_field_text(&Value::String("  ".into())), None);
    }

    #[test]
    fn profile_fields_keep_link_destinations_and_render_emoji() {
        let core = crate::fixture::fixture_core();
        let media = crate::media::MediaRegistry::default();
        let ctx = crate::blocks::BlockCtx::new(&core.workspaces["T1"], &media);
        let link = serde_json::from_value(serde_json::json!({
            "value": "https://example.com/maya",
            "alt": "My tiny corner of the web"
        }))
        .expect("link field");
        assert_eq!(
            profile_field_nodes(ctx, Some("link"), &link),
            Some(vec![crate::state::RichNode::Link {
                label: "My tiny corner of the web".into(),
                url: "https://example.com/maya".into(),
            }])
        );

        let emoji = serde_json::from_value(serde_json::json!({
            "value": ":joy:, :fox_face:"
        }))
        .expect("emoji field");
        let nodes = profile_field_nodes(ctx, Some("text"), &emoji).expect("emoji nodes");
        assert_eq!(
            nodes
                .iter()
                .filter(|node| matches!(node, crate::state::RichNode::Emoji { .. }))
                .count(),
            2
        );
    }
}
