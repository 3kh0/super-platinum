//! Draft-only emoji insertion. The picker never owns a Slack mutation.
use crate::{
    icons::{Icon, icon},
    model::MainView,
    state::{ComposerTarget, Overlay, ShellState},
};
use dioxus::prelude::*;

pub(crate) mod check;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(crate) struct State {
    serial: u64,
    active: Option<Visit>,
}

#[derive(Clone, PartialEq)]
struct Scope {
    field: &'static str,
    target: ComposerTarget,
    draft: String,
    channel: u64,
    thread: u64,
    surface: MainView,
    mount: Option<String>,
}

impl Scope {
    fn capture(shell: &ShellState, target: ComposerTarget) -> Option<Self> {
        shell.composer_key(target)?;
        if shell.composer_unavailable().is_some() {
            return None;
        }
        Some(Self {
            field: field_id(target),
            target,
            draft: crate::view::composer::field_scope(shell, target),
            channel: shell.channel_generation,
            thread: shell.thread_generation,
            surface: shell.main_view,
            mount: crate::view::composer::field_mount_token(shell, target),
        })
    }

    fn current(&self, shell: &ShellState) -> bool {
        Self::capture(shell, self.target).as_ref() == Some(self)
    }
}

#[derive(Clone)]
struct Visit {
    serial: u64,
    scope: Scope,
    snapshot: Option<super_platinum_core::composer_emoji::admission::Snapshot>,
}

fn field_id(target: ComposerTarget) -> &'static str {
    match target {
        ComposerTarget::Channel => "channel-composer",
        ComposerTarget::Thread => "thread-composer",
    }
}

fn script(call: &str) -> String {
    format!("{}\n{call}", include_str!("composer_emoji/dom.js"))
}

// Keep the JavaScript query alive until Rust consumes its reply. Dropping a
// cancelled receiver also attempts to release it; the acknowledgement does not
// repeat the action or replace its result.
struct ReplyAck(document::Eval);

impl Drop for ReplyAck {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

async fn eval_reply<T: serde::de::DeserializeOwned>(call: &str) -> Result<T, document::EvalError> {
    let mut pending = ReplyAck(document::eval(&script(&format!(
        "{call}\nawait dioxus.recv();"
    ))));
    pending.0.recv::<T>().await
}

async fn open(mut state: Signal<ShellState>, target: ComposerTarget) {
    let mut visit = {
        let mut shell = state.write();
        if shell.overlay.is_some() {
            return;
        }
        let Some(scope) = Scope::capture(&shell, target) else {
            return;
        };
        shell.composer_emoji.serial = shell.composer_emoji.serial.wrapping_add(1);
        Visit {
            serial: shell.composer_emoji.serial,
            scope,
            snapshot: None,
        }
    };
    let args =
        serde_json::to_string(&(visit.serial, visit.scope.field, &visit.scope.draft)).unwrap();
    if crate::fixture::is_fixture() {
        eprintln!("composer-emoji-open: serial={} capture-start", visit.serial);
    }
    let capture_result = eval_reply::<Option<super_platinum_core::composer_emoji::admission::Snapshot>>(&format!(
        "const captured=window.superPlatinumComposerEmoji.capture(...{args});dioxus.send(captured?window.superPlatinumComposerEmoji.captured({}):null);",visit.serial
    ))
    .await;
    visit.snapshot = capture_result.as_ref().ok().cloned().flatten();
    let captured = visit.snapshot.is_some();
    let mut shell = state.write();
    if crate::fixture::is_fixture() {
        eprintln!(
            "composer-emoji-open: serial={} result={capture_result:?} current={} latest={} overlay={:?}",
            visit.serial,
            visit.scope.current(&shell),
            shell.composer_emoji.serial,
            shell.overlay
        );
    }
    if !captured
        || !visit.scope.current(&shell)
        || shell.composer_emoji.serial != visit.serial
        || shell.overlay.is_some()
    {
        document::eval(&script(&format!(
            "window.superPlatinumComposerEmoji.discard({});",
            visit.serial
        )));
        return;
    }
    if crate::emoji_native::enabled() {
        crate::emoji_native::observe(
            "picker-open",
            serde_json::json!({"serial":visit.serial,"snapshot":visit.snapshot}),
        );
    }
    shell.composer_emoji.active = Some(visit);
    shell.overlay = Some(Overlay::ComposerEmoji);
}

/// Used by generic Escape/dismissal as well as the picker close button.
pub(crate) fn close_shell(shell: &mut ShellState) {
    if shell.overlay != Some(Overlay::ComposerEmoji) {
        return;
    }
    if crate::fixture::is_fixture() {
        eprintln!(
            "composer-emoji-close: serial={:?}",
            shell
                .composer_emoji
                .active
                .as_ref()
                .map(|visit| visit.serial)
        );
    }
    shell.overlay = None;
    if let Some(visit) = shell.composer_emoji.active.take() {
        let method = if visit.scope.current(shell) {
            "cancel"
        } else {
            "discard"
        };
        document::eval(&script(&format!(
            "window.superPlatinumComposerEmoji.{method}({});",
            visit.serial
        )));
    }
}

fn close_visit(shell: &mut ShellState, serial: u64) {
    if shell
        .composer_emoji
        .active
        .as_ref()
        .is_some_and(|visit| visit.serial == serial)
    {
        close_shell(shell);
    }
}

fn insertion(
    catalog: &super_platinum_core::composer_emoji::catalog::Catalog,
    preferences: &super_platinum_core::composer_emoji::preferences::Preferences,
    name: &str,
    prefix: &str,
) -> Option<super_platinum_core::composer_emoji::catalog::Resolved> {
    let _ = prefix;
    if !preferences.loaded() {
        return None;
    }
    catalog.resolve(name, preferences.preferred_tone())
}

pub(crate) fn release_choice(serial: u64) {
    document::eval(&script(&format!(
        "window.superPlatinumComposerEmoji.release({serial});"
    )));
}

fn choose(mut state: Signal<ShellState>, serial: u64, selection: crate::emoji_catalog::Selection) {
    let prepared = (|| {
        let mut shell = state.write();
        let Some(visit) = shell
            .composer_emoji
            .active
            .as_ref()
            .filter(|visit| visit.serial == serial && visit.scope.current(&shell))
            .cloned()
        else {
            return None;
        };
        if shell.overlay != Some(Overlay::ComposerEmoji)
            || !crate::emoji_catalog::current(&shell)
                .is_some_and(|current| std::sync::Arc::ptr_eq(&current, &selection.loaded))
        {
            return None;
        }
        let Some(snapshot) = visit.snapshot.as_ref() else {
            return None;
        };
        let Some((anchor, _)) = snapshot.byte_selection() else {
            return None;
        };
        let Some(workspace) = shell
            .core
            .active_team
            .as_ref()
            .and_then(|t| shell.core.workspaces.get(t))
        else {
            return None;
        };
        let preferences = &workspace.emoji_preferences;
        let Some(resolved) = insertion(
            &selection.loaded.catalog,
            preferences,
            &selection.name,
            &snapshot.text[..anchor],
        ) else {
            return None;
        };
        let representation = if preferences.as_text()
            || super_platinum_core::composer_emoji::context::in_markdown_code(
                &snapshot.text[..anchor],
            ) {
            super_platinum_core::composer_emoji::insertion::Representation::Shortcode
        } else {
            super_platinum_core::composer_emoji::insertion::Representation::Rendered
        };
        let value = resolved.insertion(representation);
        let guard = serde_json::json!({"catalog":selection.loaded.token,"preferences":preferences.revision().to_string()});
        let usage = super_platinum_core::composer_emoji::admission::Usage {
            id: resolved.usage_id,
            query: selection.query,
        };
        shell.composer_emoji.active = None;
        shell.overlay = None;
        Some((visit, value, guard, usage))
    })();
    let Some(prepared) = prepared else {
        release_choice(serial);
        return;
    };
    dioxus::core::spawn_forever(async move {
        let (visit, value, guard, usage) = prepared;
        if crate::emoji_native::enabled() {
            crate::emoji_native::boundary("picker-choose",serde_json::json!({"serial":visit.serial,"snapshot":visit.snapshot,"value":value,"guard":guard})).await;
        }
        let args = serde_json::to_string(&(visit.serial, value, guard)).unwrap();
        let confirmed = eval_reply::<bool>(&format!(
            "dioxus.send(await window.superPlatinumComposerEmoji.chooseConfirmed(...{args}));"
        ))
        .await
        .unwrap_or(false);
        if crate::emoji_native::enabled() {
            crate::emoji_native::observe(
                "picker-ack",
                serde_json::json!({"serial":visit.serial,"confirmed":confirmed,"scopeCurrent":visit.scope.current(&state.peek()),"catalogCurrent":crate::emoji_catalog::current(&state.peek()).is_some_and(|c|std::sync::Arc::ptr_eq(&c,&selection.loaded))}),
            );
        }
        if confirmed
            && visit.scope.current(&state.peek())
            && crate::emoji_catalog::current(&state.peek())
                .is_some_and(|current| std::sync::Arc::ptr_eq(&current, &selection.loaded))
        {
            crate::emoji_preferences::record(state, usage).await;
        }
    });
}

#[component]
pub(crate) fn EmojiButton(state: Signal<ShellState>, target: ComposerTarget) -> Element {
    let id = format!("{}-emoji", field_id(target));
    rsx! {
        button { id, r#type:"button", class:"composer-icon-btn", title:"Insert emoji",
            "aria-label":"Insert emoji", "aria-haspopup":"dialog",
            onmousedown:move |event|event.prevent_default(),
            onclick:move |_|{spawn(open(state,target));},
            {icon(Icon::AddReaction,"icon sm")}
        }
    }
}

#[component]
pub(crate) fn Picker(mut state: Signal<ShellState>) -> Element {
    use_effect(move || {
        let shell = state.read();
        let stale = shell.overlay == Some(Overlay::ComposerEmoji)
            && shell
                .composer_emoji
                .active
                .as_ref()
                .is_none_or(|visit| !visit.scope.current(&shell));
        drop(shell);
        if stale {
            close_shell(&mut state.write());
        }
    });
    let Some(visit) = state.read().composer_emoji.active.clone() else {
        return rsx! {};
    };
    if !visit.scope.current(&state.read()) {
        return rsx! {};
    }
    rsx! {crate::reaction_picker::EmojiPicker {
        key:"composer-emoji-{visit.serial}",state,title:"Insert emoji".to_owned(),
        composer_visit:Some(visit.serial),
        onchoose:move |name|choose(state,visit.serial,name),
        onclose:move |_|close_visit(&mut state.write(),visit.serial),
    }}
}
