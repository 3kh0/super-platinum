use dioxus::prelude::*;
use super_platinum_core::FormatMark;

use super::rich::attachment_chip;
use crate::state::ShellState;

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

pub(crate) fn composer(mut state: Signal<ShellState>, text: &str, channel_name: &str) -> Element {
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
    let upload_epoch = state.read().upload_ui_epoch;
    let composer_attachments = state.read().core.composer_attachments.clone();
    rsx! {
        footer { class: "composer-wrap",
            if !composer_attachments.is_empty() {
                div { class: "attachment-chips",
                    for attachment in composer_attachments.iter() {
                        {attachment_chip(state, attachment, upload_epoch)}
                    }
                }
            }
            div { class: "composer-row",
                label { class: "composer-icon-btn", title: "Attach files",
                    {crate::icons::icon(crate::icons::Icon::Add, "icon sm")}
                    input {
                        r#type: "file",
                        multiple: true,
                        onchange: move |event| {
                            state.write().add_attachments(event.files().into_iter().map(|file| file.path()));
                        }
                    }
                }
                textarea {
                    id: "channel-composer",
                    initial_value: "{text}",
                    placeholder: "{placeholder}",
                    onmounted: move |_| {
                        dioxus::document::eval(
                            r#"const editor = document.getElementById('channel-composer');
                               if (!editor) return;
                               const resize = () => {
                                 editor.style.height = 'auto';
                                 editor.style.height = Math.min(160, Math.max(28, editor.scrollHeight)) + 'px';
                               };
                               editor.addEventListener('input', resize);
                               resize();"#,
                        );
                    },
                    oninput: move |event| {
                        let value = event.value();
                        let end = value.len();
                        let mut state = state.write();
                        state.core.composer.text = value;
                        state.core.composer.set_selection(end, end);
                        state.notify_typing();
                        // Autosizing runs off the DOM's own `input` listener
                        // registered at mount — one IPC round trip per keystroke
                        // is exactly what makes typing feel gluey.
                    },
                    onkeydown: move |event| {
                        if event.key() == Key::Enter && !event.modifiers().shift() {
                            event.prevent_default();
                            spawn(crate::bootstrap::send_composer(state));
                        }
                    },
                }
                button {
                    class: "composer-icon-btn",
                    title: "Send",
                    onclick: move |_| { spawn(crate::bootstrap::send_composer(state)); },
                    {crate::icons::icon(crate::icons::Icon::Send, "icon sm")}
                }
            }
        }
    }
}

#[allow(dead_code)]
async fn format_composer_dom(mut state: Signal<ShellState>, mark: FormatMark) {
    let selection = dioxus::document::eval(
        r#"const editor = document.getElementById('channel-composer');
            dioxus.send(editor ? [editor.selectionStart, editor.selectionEnd] : [0, 0]);"#,
    )
    .recv::<Vec<usize>>()
    .await
    .unwrap_or_default();
    let (start_utf16, end_utf16) = match selection.as_slice() {
        [start, end, ..] => (*start, *end),
        _ => (0, 0),
    };
    let (selection_start, selection_end, text) = {
        let mut shell = state.write();
        let start = utf16_to_byte(&shell.core.composer.text, start_utf16);
        let end = utf16_to_byte(&shell.core.composer.text, end_utf16);
        shell.core.composer.set_selection(start, end);
        shell.format_composer(mark);
        (
            byte_to_utf16(
                &shell.core.composer.text,
                shell.core.composer.selection.start,
            ),
            byte_to_utf16(&shell.core.composer.text, shell.core.composer.selection.end),
            shell.core.composer.text.clone(),
        )
    };
    let text = serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".into());
    dioxus::document::eval(&format!(
        r#"requestAnimationFrame(() => {{
              const editor = document.getElementById('channel-composer');
              if (editor) {{ editor.value = {text}; editor.focus(); editor.setSelectionRange({selection_start}, {selection_end}); }}
            }});"#
    ));
}

pub(crate) async fn measure_timeline(mut state: Signal<ShellState>) {
    let first_measurement = state.read().row_heights.is_empty();
    let Ok(value) = dioxus::document::eval(
        r#"const timeline = document.getElementById('message-timeline');
            if (!timeline) { dioxus.send({first: 0, last: 0, rows: []}); }
            else {
              const bounds = timeline.getBoundingClientRect();
              const rows = [...timeline.querySelectorAll('[data-message-index]')].map(row => ({
                index: Number(row.dataset.messageIndex), id: row.dataset.messageId,
                height: row.getBoundingClientRect().height,
                top: row.getBoundingClientRect().top, bottom: row.getBoundingClientRect().bottom
              }));
              const visible = rows.filter(row => row.bottom >= bounds.top && row.top <= bounds.bottom);
              dioxus.send({
                first: visible.length ? visible[0].index : (rows[0]?.index ?? 0),
                last: visible.length ? visible[visible.length - 1].index : (rows[rows.length - 1]?.index ?? 0),
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
    let first = value
        .get("first")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let last = value
        .get("last")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(first as u64) as usize;
    let measurements = value
        .get("rows")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            Some((row.first()?.as_str()?.to_owned(), row.get(1)?.as_f64()?))
        })
        .collect::<Vec<_>>();
    let stick_to_bottom = value
        .get("atBottom")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    state
        .write()
        .set_timeline_window(first, last, measurements, stick_to_bottom);
    if first_measurement {
        dioxus::document::eval(
            "requestAnimationFrame(() => requestAnimationFrame(() => { const timeline = document.getElementById('message-timeline'); if (timeline) timeline.scrollTop = timeline.scrollHeight; }));",
        );
    }
    if stick_to_bottom || first_measurement {
        spawn(crate::bootstrap::mark_visible_read(state));
    }
}

/// Tracks whether the thread pane is parked on its newest reply. The pane only
/// auto-scrolls while it is, so a reader who scrolled up to read history is not
/// yanked back down by every arriving reply.
pub(crate) async fn measure_thread(mut state: Signal<ShellState>) {
    let at_bottom = dioxus::document::eval(
        "const body = document.getElementById('thread-body');
         dioxus.send(Boolean(body && body.scrollHeight - body.scrollTop - body.clientHeight < 48));",
    )
    .recv::<bool>()
    .await
    .unwrap_or(true);
    if state.read().thread_at_bottom != at_bottom {
        state.write().thread_at_bottom = at_bottom;
    }
}

pub(crate) async fn load_older_if_needed(state: Signal<ShellState>) {
    let near_top = dioxus::document::eval(
        "const t=document.getElementById('message-timeline'); dioxus.send(Boolean(t && t.scrollTop < 96));",
    )
    .recv::<bool>()
    .await
    .unwrap_or(false);
    if near_top {
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
