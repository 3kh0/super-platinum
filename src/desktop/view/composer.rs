use dioxus::prelude::*;
use super_platinum_core::FormatMark;

use super::rich::attachment_chip;
use crate::state::{ComposerTarget, ShellState};

mod commands;
mod field;
mod hosted;

pub(crate) fn field_mount_token(shell: &ShellState, target: ComposerTarget) -> Option<String> {
    field::current(shell, target).map(|lease| lease.token.clone())
}

pub(crate) mod projection;
pub(crate) mod protected_actions;
mod protected_admission;
pub(crate) mod protected_adoption;
pub(crate) mod protected_enrollment;
pub(crate) mod protected_operations;
pub(crate) mod protected_projection;
mod remote_review;
pub(crate) use projection::RendererApplied;

// Dioxus's implicit `.*` filter is passed as a literal extension on macOS.
// Calling the native dialog without a filter accepts all ordinary file types.
async fn choose_attachments(mut state: Signal<ShellState>, target: ComposerTarget) {
    let scope = {
        let shell = state.peek();
        (
            shell.core.active_account.clone(),
            shell.composer_key(target),
        )
    };
    let Some(files) = rfd::AsyncFileDialog::new().pick_files().await else {
        return;
    };
    let mut shell = state.write();
    if (
        shell.core.active_account.clone(),
        shell.composer_key(target),
    ) != scope
    {
        shell.show_toast("The conversation changed. Choose the files again to attach them here.");
        return;
    }
    if crate::drafts::protected::production::hosted::present(&shell, target) {
        shell.show_toast("Remove the attached Slack files before uploading from this computer.");
        return;
    }
    shell.add_attachments(files.into_iter().map(|file| file.path().to_owned()), target);
}

/// Writes a value into a live text field.
///
/// The text fields render `initial_value`, not `value`: Dioxus marks `value`
/// volatile, so it is re-written to the DOM on *every* render. A render that
/// lands while the field is ahead of the signal — fast typing, or any keystroke
/// during a busy channel's re-render — puts the older text back and drops
/// characters. The cost of `initial_value` is that changes the app makes itself
/// (send clears the box, the agent types into the switcher) have to be pushed.
pub(crate) fn set_field_text(id: &str, text: &str) {
    let id = serde_json::to_string(id).unwrap_or_else(|_| "\"\"".into());
    let text = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
    dioxus::document::eval(&format!(
        r#"const field = document.getElementById({id});
           if (field) {{
             field.value = {text};
             if (field.tagName === 'TEXTAREA') {{
               field.style.height = 'auto';
               field.style.height = Math.min(160, Math.max(28, field.scrollHeight)) + 'px';
             }}
           }}"#
    ));
}

pub(crate) fn clear_delivered_field(id: &str, scope: &str, expected: &str) {
    let args = serde_json::to_string(&(id, scope, expected)).unwrap();
    dioxus::document::eval(&format!(
        "{}; clearDeliveredComposer(...{args});",
        include_str!("../composer_delivery.js"),
    ));
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "status", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum ScheduledClearReceipt {
    Cleared { scope: String },
    Changed { scope: String, text: String },
    Composing,
    Unavailable,
}

pub(crate) async fn settle_scheduled_field(
    id: &str,
    scope: &str,
    expected: &str,
) -> ScheduledClearReceipt {
    let args = serde_json::to_string(&(id, scope, expected)).unwrap();
    let mut eval = dioxus::document::eval(&format!(
        "{}; try{{dioxus.send(settleScheduledComposer(...{args}));}}catch(error){{dioxus.send({{status:'unavailable'}});}} await dioxus.recv();",
        include_str!("../composer_delivery.js"),
    ));
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        eval.recv::<ScheduledClearReceipt>(),
    )
    .await;
    let _ = eval.send(());
    match result {
        Ok(Ok(receipt)) => receipt,
        _ => ScheduledClearReceipt::Unavailable,
    }
}

pub(crate) fn channel_field_scope(shell: &ShellState) -> String {
    field_scope(shell, ComposerTarget::Channel)
}

pub(crate) fn replace_reviewed_field(id: &str, scope: &str, expected: &str, text: &str) {
    let args = serde_json::to_string(&(id, scope, expected, text)).unwrap();
    dioxus::document::eval(&format!(
        "{}; replaceReviewedComposer(...{args});",
        include_str!("../composer_delivery.js"),
    ));
}

pub(crate) fn field_scope(shell: &ShellState, target: ComposerTarget) -> String {
    let scope = format!(
        "{}:{}:{:?}:{:?}",
        shell.draft_writes.view_id,
        shell.core.account_epoch,
        shell.core.active_account,
        shell.composer_key(target)
    );
    match target {
        ComposerTarget::Channel => format!("{scope}:{}", shell.channel_generation),
        ComposerTarget::Thread => format!("{scope}:{}", shell.thread_generation),
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    #[test]
    fn reopening_same_channel_retires_dom_scope() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let original = shell.active_channel;
        let scope = channel_field_scope(&shell);
        let other = if original == 0 { 1 } else { 0 };
        shell.select_channel(other, crate::state::ChannelOpen::Global);
        shell.select_channel(original, crate::state::ChannelOpen::Global);
        assert_ne!(scope, channel_field_scope(&shell));
    }

    #[test]
    fn reopening_same_thread_retires_dom_scope_without_changing_channel_scope() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.thread_root = Some("100.000001".into());
        let thread = field_scope(&shell, ComposerTarget::Thread);
        let channel = field_scope(&shell, ComposerTarget::Channel);
        shell.thread_generation += 1;
        assert_ne!(thread, field_scope(&shell, ComposerTarget::Thread));
        assert_eq!(channel, field_scope(&shell, ComposerTarget::Channel));
    }
}

/// Focuses a text field that may not be mounted yet, caret at the end.
///
/// The thread pane renders a frame after `thread_root` is set, so the field is
/// looked for on each animation frame for a short while rather than once. The
/// double-click that got us here also selected a word in the message; that
/// selection is dropped so it does not linger behind the caret.
pub(crate) fn focus_field_when_mounted(id: &str) {
    let id = serde_json::to_string(id).unwrap_or_else(|_| "\"\"".into());
    dioxus::document::eval(&format!(
        r#"let frames = 0;
           const attempt = () => {{
             const field = document.getElementById({id});
             if (field) {{
               window.getSelection()?.removeAllRanges();
               field.focus();
               const end = field.value.length;
               field.setSelectionRange(end, end);
             }} else if (++frames < 60) {{
               requestAnimationFrame(attempt);
             }}
           }};
           requestAnimationFrame(attempt);"#
    ));
}

pub(crate) fn composer(state: Signal<ShellState>, text: &str, channel_name: &str) -> Element {
    if let Some(reason) = state.read().composer_unavailable() {
        let channel = state.read().core.active_channel.clone().unwrap_or_default();
        return rsx! {footer{class:"composer-unavailable",p{role:"status","{reason}"}
            super::channel_membership::MembershipControls{state,channel,ready:true,disabled:false,join_only:true,onbusy:move |_|{},onchanged:move |_|{}}
        }};
    }
    let is_dm_like = state
        .read()
        .channels
        .get(state.read().active_channel)
        .is_some_and(|channel| channel.is_im || channel.is_mpim);
    let placeholder = if is_dm_like {
        format!("Message {channel_name}")
    } else {
        format!("Message #{channel_name}")
    };
    let field_key = channel_field_scope(&state.read());
    let upload_epoch = state.read().upload_ui_epoch;
    let composer_attachments = state.read().core.composer_attachments.clone();
    rsx! {
        footer { key: "{field_key}", class: "composer-wrap",
            super::completion::ComposerCompletion {state,target:ComposerTarget::Channel,field_id:"channel-composer"}
            {commands::panel(state, ComposerTarget::Channel)}
            {format_toolbar(state, ComposerTarget::Channel)}
            hosted::HostedChips {state}
            if !composer_attachments.is_empty() {
                div { class: "attachment-chips",
                    for attachment in composer_attachments.iter() {
                        {attachment_chip(state, attachment, upload_epoch)}
                    }
                }
            }
            div { class: "composer-row",
                hosted::AttachmentControl {state}
                button { class:"composer-icon-btn", title:"App shortcuts", "aria-label":"App shortcuts",
                    onclick:{let expected=field_key.clone();move |_|crate::message_buttons::shortcuts::open_global(state,ComposerTarget::Channel,&expected)},
                    {crate::icons::icon(crate::icons::Icon::GridView,"icon sm")}
                }
                field::ComposerField {
                    key: "{field_key}", state, target: ComposerTarget::Channel,
                    scope: field_key.clone(), text: text.to_owned(), placeholder,
                }
                button {
                    class: "composer-icon-btn composer-send-btn",
                    title: "Send",
                    onclick: move |_| { spawn(crate::bootstrap::send_composer(state)); },
                    {crate::icons::icon(crate::icons::Icon::Send, "icon sm")}
                }
                super::scheduled::ScheduleButton {state,target:ComposerTarget::Channel}
            }
            remote_review::RemoteReview { key:"{field_key}:remote-review", state, target:ComposerTarget::Channel }
        }
    }
}

/// The thread pane's reply box.
///
/// Built like the channel composer — attach, field, send — because a reply can
/// carry files too, and they must be sent into the thread rather than the
/// channel behind it.
pub(crate) fn thread_composer(mut state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    if let Some(reason) = snapshot.composer_unavailable() {
        return rsx! {footer{class:"composer-unavailable",role:"status","{reason}"}};
    }
    let field_key = field_scope(snapshot, ComposerTarget::Thread);
    let upload_epoch = snapshot.upload_ui_epoch;
    let broadcast_scope = field_key.clone();
    let broadcast_generation = snapshot.thread_generation;
    let broadcast_label = snapshot.thread_broadcast_label();
    let broadcast_disabled = snapshot.thread_broadcast_disabled();
    rsx! {
        footer { key: "{field_key}", class: "thread-composer",
            super::completion::ComposerCompletion {state,target:ComposerTarget::Thread,field_id:"thread-composer"}
            {commands::panel(state, ComposerTarget::Thread)}
            {format_toolbar(state, ComposerTarget::Thread)}
            if !snapshot.core.thread_composer_attachments.is_empty() {
                div { class: "attachment-chips",
                    for attachment in snapshot.core.thread_composer_attachments.iter() {
                        {attachment_chip(state, attachment, upload_epoch)}
                    }
                }
            }
            div { class: "composer-row",
                button { class: "composer-icon-btn", title: "Attach files", "aria-label":"Attach files",
                    onclick:move |_|{spawn(choose_attachments(state,ComposerTarget::Thread));},
                    {crate::icons::icon(crate::icons::Icon::Add, "icon sm")}
                }
                button { class:"composer-icon-btn", title:"App shortcuts", "aria-label":"App shortcuts",
                    onclick:{let expected=field_key.clone();move |_|crate::message_buttons::shortcuts::open_global(state,ComposerTarget::Thread,&expected)},
                    {crate::icons::icon(crate::icons::Icon::GridView,"icon sm")}
                }
                field::ComposerField {
                    key: "{field_key}", state, target: ComposerTarget::Thread,
                    scope: field_key.clone(), text: snapshot.core.thread_composer.text.clone(),
                    placeholder: "Reply…".to_owned(),
                }
                button {
                    class: "composer-icon-btn composer-send-btn",
                    title: "Send",
                    onclick: move |_| { spawn(crate::bootstrap::send_thread_composer(state)); },
                    {crate::icons::icon(crate::icons::Icon::Send, "icon sm")}
                }
                super::scheduled::ScheduleButton {state,target:ComposerTarget::Thread}
            }
            label {class:"thread-broadcast", r#for:"thread-reply-broadcast",
                input {id:"thread-reply-broadcast", r#type:"checkbox",
                    checked:snapshot.core.thread_composer.reply_broadcast,
                    disabled:broadcast_disabled,
                    onchange:move |event| {state.write().set_thread_broadcast(&broadcast_scope,broadcast_generation,event.checked());}
                }
                span {"{broadcast_label}"}
            }
            remote_review::RemoteReview { key:"{field_key}:remote-review", state, target:ComposerTarget::Thread }
        }
    }
}

fn mount_autosize(id: &str) {
    let id = serde_json::to_string(id).unwrap();
    dioxus::document::eval(&format!(
        r#"const editor = document.getElementById({id});
           if (!editor) return;
           const resize = () => {{
             editor.style.height = 'auto';
             editor.style.height = Math.min(160, Math.max(28, editor.scrollHeight)) + 'px';
           }};
           editor.addEventListener('input', resize);
           editor.addEventListener('compositionstart', () => editor.dataset.composing = 'true');
           editor.addEventListener('compositionend', () => editor.dataset.composing = 'false');
           resize();"#,
    ));
}

fn format_shortcut(event: &KeyboardEvent) -> Option<FormatMark> {
    if !(event.modifiers().meta() || event.modifiers().ctrl()) || event.modifiers().alt() {
        return None;
    }
    let Key::Character(key) = event.key() else {
        return None;
    };
    match (key.to_lowercase().as_str(), event.modifiers().shift()) {
        ("b", false) => Some(FormatMark::Bold),
        ("i", false) => Some(FormatMark::Italic),
        ("x", true) => Some(FormatMark::Strike),
        ("c", true) => Some(FormatMark::Code),
        _ => None,
    }
}

fn format_toolbar(state: Signal<ShellState>, target: ComposerTarget) -> Element {
    use crate::icons::{Icon, icon};
    let controls = [
        (FormatMark::Bold, Icon::FormatBold, "Bold"),
        (FormatMark::Italic, Icon::FormatItalic, "Italic"),
        (FormatMark::Strike, Icon::FormatStrike, "Strikethrough"),
        (FormatMark::Code, Icon::Code, "Inline code"),
        (FormatMark::CodeBlock, Icon::CodeBlock, "Code block"),
        (FormatMark::Quote, Icon::Quote, "Quote"),
    ];
    rsx! {
        div { class: "composer-formatting", role: "group", "aria-label": "Text formatting",
            for (mark, symbol, label) in controls {
                button {
                    r#type: "button",
                    class: "composer-icon-btn",
                    title: label,
                    "aria-label": label,
                    onmousedown: |event| event.prevent_default(),
                    onclick: move |_| { spawn(format_composer_dom(state, target, mark)); },
                    {icon(symbol, "icon sm")}
                }
            }
            crate::composer_emoji::EmojiButton {state,target}
        }
    }
}

async fn format_composer_dom(state: Signal<ShellState>, target: ComposerTarget, mark: FormatMark) {
    let id = match target {
        ComposerTarget::Channel => "channel-composer",
        ComposerTarget::Thread => "thread-composer",
    };
    let identity = {
        let shell = state.read();
        (
            shell.core.active_team.clone(),
            shell.channel_generation,
            shell.thread_root.clone(),
        )
    };
    // Read the DOM's current value as well as its selection. A fast keystroke can
    // be ahead of the Rust signal; formatting must never overwrite it.
    let Ok((original, start, end)) = dioxus::document::eval(&format!(
        "const e = document.getElementById('{id}'); dioxus.send(e ? [e.value, e.selectionStart, e.selectionEnd] : null);"
    )).recv::<(String, usize, usize)>().await else { return; };
    {
        let shell = state.read();
        if identity
            != (
                shell.core.active_team.clone(),
                shell.channel_generation,
                shell.thread_root.clone(),
            )
        {
            return;
        }
    }
    let mut composer = super_platinum_core::ComposerState::with_text(&original);
    composer.set_selection(
        utf16_to_byte(&original, start),
        utf16_to_byte(&original, end),
    );
    composer.apply_format(mark);
    let selection_start = byte_to_utf16(&composer.text, composer.selection.start);
    let selection_end = byte_to_utf16(&composer.text, composer.selection.end);
    let text = serde_json::to_string(&composer.text).unwrap();
    let original = serde_json::to_string(&original).unwrap();
    dioxus::document::eval(&format!(
        r#"const editor = document.getElementById('{id}');
           if (editor && editor.value === {original} && editor.selectionStart === {start} && editor.selectionEnd === {end}) {{
             editor.value = {text};
             editor.focus();
             editor.setSelectionRange({selection_start}, {selection_end});
             editor.dispatchEvent(new Event('input', {{ bubbles: true }}));
           }}"#,
    ));
}

pub(crate) fn schedule_timeline_measure(
    state: Signal<ShellState>,
    mut pending: Signal<bool>,
    mut queued: Signal<bool>,
) {
    if *pending.read() {
        queued.set(true);
        return;
    }
    pending.set(true);
    queued.set(false);
    let source = {
        let shell = state.read();
        (
            shell.channel_generation,
            shell.core.active_team.clone(),
            shell.core.active_channel.clone(),
        )
    };
    spawn(async move {
        measure_timeline(state, queued).await;
        pending.set(false);
        let shell = state.read();
        let changed = source
            != (
                shell.channel_generation,
                shell.core.active_team.clone(),
                shell.core.active_channel.clone(),
            );
        drop(shell);
        if changed || *queued.read() {
            schedule_timeline_measure(state, pending, queued);
        }
    });
}

async fn measure_timeline(mut state: Signal<ShellState>, queued: Signal<bool>) {
    let (generation, team, channel, window, first_measurement) = {
        let shell = state.read();
        (
            shell.channel_generation,
            shell.core.active_team.clone(),
            shell.core.active_channel.clone(),
            (shell.timeline_start, shell.timeline_end),
            shell.row_heights.is_empty()
                && shell.stick_to_bottom
                && !matches!(
                    shell.core.pending_scroll_to,
                    Some((
                        _,
                        super_platinum_core::domain::PendingScrollTarget::Message(_)
                    ))
                ),
        )
    };
    let Ok(value) = dioxus::document::eval(
        r#"const timeline = document.getElementById('message-timeline');
            if (!timeline) { dioxus.send(null); }
            else {
              const bounds = timeline.getBoundingClientRect();
              const rows = [...timeline.querySelectorAll('[data-row-index]')].map(row => ({
                index: Number(row.dataset.rowIndex), id: row.dataset.rowId,
                height: row.getBoundingClientRect().height,
                top: row.getBoundingClientRect().top, bottom: row.getBoundingClientRect().bottom
              }));
              const visible = rows.filter(row => row.bottom >= bounds.top && row.top <= bounds.bottom);
              dioxus.send({
                windowStart: Number(timeline.dataset.windowStart),
                windowEnd: Number(timeline.dataset.windowEnd),
                visible: visible.length > 0,
                scrollTop: timeline.scrollTop,
                viewportHeight: timeline.clientHeight,
                first: visible.length ? visible[0].index : (rows[0]?.index ?? 0),
                last: visible.length ? visible[visible.length - 1].index : (rows[rows.length - 1]?.index ?? 0),
                firstId: visible.length ? visible[0].id : rows[0]?.id,
                lastId: visible.length ? visible[visible.length - 1].id : rows[rows.length - 1]?.id,
                rows: rows.map(row => [row.id, row.height]),
                atBottom: timeline.scrollHeight - timeline.scrollTop - timeline.clientHeight < 48
              });
            }"#,
    )
    .recv::<serde_json::Value>()
    .await
    else {
        return;
    };
    // A newer scroll has already requested another sample. Applying this one
    // would move the virtual window back underneath the reader's next gesture.
    if value.is_null() || *queued.peek() {
        return;
    }
    let mut first = value
        .get("first")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let mut last = value
        .get("last")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(first as u64) as usize;
    let visible = value
        .get("visible")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !visible {
        (first, last) = state.peek().estimated_timeline_window(
            value
                .get("scrollTop")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0),
            value
                .get("viewportHeight")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0),
        );
    }
    let measurements = value
        .get("rows")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            Some((row.first()?.as_str()?.to_owned(), row.get(1)?.as_f64()?))
        })
        .filter(|(_, height)| height.is_finite() && *height > 0.0)
        .collect::<Vec<_>>();
    let stick_to_bottom = value
        .get("atBottom")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    // A queued eval may complete after the user has opened another channel.
    {
        let shell = state.read();
        if shell.channel_generation != generation
            || shell.core.active_team != team
            || shell.core.active_channel != channel
        {
            return;
        }
        // A target jump can replace the window within the same conversation.
        // The old row IDs still exist in full metadata, so identity alone does
        // not authorize its late measurement to move the new window back.
        if window != (shell.timeline_start, shell.timeline_end)
            || value.get("windowStart").and_then(serde_json::Value::as_u64) != Some(window.0 as u64)
            || value.get("windowEnd").and_then(serde_json::Value::as_u64) != Some(window.1 as u64)
            || (visible
                && (value.get("firstId").and_then(serde_json::Value::as_str)
                    != shell.channel_rows.get(first).map(|m| m.id.as_str())
                    || value.get("lastId").and_then(serde_json::Value::as_str)
                        != shell.channel_rows.get(last).map(|m| m.id.as_str())))
        {
            drop(shell);
            let mut queued = queued;
            queued.set(true);
            return;
        }
    }
    if state
        .read()
        .timeline_window_changed(first, last, &measurements, stick_to_bottom)
    {
        state
            .write()
            .set_timeline_window(first, last, measurements, stick_to_bottom);
    }
    if first_measurement {
        spawn(async move {
            if state.peek().channel_generation == generation && state.peek().stick_to_bottom {
                crate::bootstrap::scroll_to_pending(
                    state,
                    super_platinum_core::domain::PendingScrollTarget::Latest,
                )
                .await;
            }
        });
    }
    if stick_to_bottom || first_measurement {
        spawn(crate::bootstrap::mark_visible_read(state));
    }
}

pub(crate) async fn load_older_if_needed(state: Signal<ShellState>) {
    let visit = {
        let shell = state.read();
        (shell.core.account_epoch, shell.channel_generation)
    };
    let near_top = dioxus::document::eval(
        "const t=document.getElementById('message-timeline'); dioxus.send(Boolean(t && t.scrollTop < 96));",
    )
    .recv::<bool>()
    .await
    .unwrap_or(false);
    let current = {
        let shell = state.read();
        visit == (shell.core.account_epoch, shell.channel_generation)
    };
    if near_top && current {
        crate::bootstrap::load_older(state).await;
    }
}

#[allow(dead_code)]
pub(crate) fn utf16_to_byte(text: &str, target: usize) -> usize {
    let mut units = 0;
    for (index, character) in text.char_indices() {
        if units >= target {
            return index;
        }
        units += character.len_utf16();
        if units > target {
            return index;
        }
    }
    text.len()
}

#[allow(dead_code)]
pub(crate) fn byte_to_utf16(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].encode_utf16().count()
}
