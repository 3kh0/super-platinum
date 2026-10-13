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

async fn open(mut state: Signal<ShellState>, target: ComposerTarget) {
    let visit = {
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
        }
    };
    let args =
        serde_json::to_string(&(visit.serial, visit.scope.field, &visit.scope.draft)).unwrap();
    if crate::fixture::is_fixture() {
        eprintln!("composer-emoji-open: serial={} capture-start", visit.serial);
    }
    let capture_result = document::eval(&script(&format!(
        "dioxus.send(window.superPlatinumComposerEmoji.capture(...{args}));"
    )))
    .recv::<bool>()
    .await;
    let captured = capture_result.as_ref().copied().unwrap_or(false);
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

fn insertion(shell: &ShellState, name: &str) -> Option<String> {
    // Custom names can override a standard name or alias. Preserve that exact
    // workspace identity in Slack's shortcode form instead of substituting a glyph.
    let workspace = shell
        .core
        .workspaces
        .get(shell.core.active_team.as_ref()?)?;
    if workspace.custom_emoji.contains_key(name)
        && super_platinum_core::state::custom_emoji_source(&workspace.custom_emoji, name).is_some()
    {
        return Some(format!(":{name}:"));
    }
    super_platinum_core::state::standard_emoji_catalog()
        .iter()
        .find(|emoji| emoji.name == name)
        .map(|emoji| emoji.glyph.to_owned())
}

fn choose(mut state: Signal<ShellState>, serial: u64, name: String) {
    let mut shell = state.write();
    if crate::fixture::is_fixture() {
        eprintln!(
            "composer-emoji-choose: serial={serial} name={name:?} active={:?} overlay={:?}",
            shell
                .composer_emoji
                .active
                .as_ref()
                .map(|visit| visit.serial),
            shell.overlay
        );
    }
    let Some(visit) = shell
        .composer_emoji
        .active
        .as_ref()
        .filter(|visit| visit.serial == serial && visit.scope.current(&shell))
    else {
        return;
    };
    if shell.overlay != Some(Overlay::ComposerEmoji) {
        return;
    }
    let Some(value) = insertion(&shell, &name) else {
        return;
    };
    let serial = visit.serial;
    shell.composer_emoji.active = None;
    shell.overlay = None;
    let args = serde_json::to_string(&(serial, value)).unwrap();
    document::eval(&script(&format!(
        "window.superPlatinumComposerEmoji.choose(...{args});"
    )));
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
        onchoose:move |name|choose(state,visit.serial,name),
        onclose:move |_|close_visit(&mut state.write(),visit.serial),
    }}
}
