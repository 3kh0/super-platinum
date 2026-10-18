//! Shared ownership predicate for modal and channel-embedded Canvas presentation.
use super::*;
use crate::{channel_tabs::selection, state::ChannelTab};

pub(crate) fn hold_local(shell: &mut ShellState, generation: u64) -> bool {
    if shell.shortcut_generation != generation
        || (shell.overlay.is_none() && selection::removal_pending(shell))
    {
        return false;
    }
    if shell.canvas_checklist.local_editor != Some(generation) {
        // A link may still be resolving metadata. Opening an editor is a newer
        // user action; its eventual response must not navigate away from it.
        shell.slack_link_generation = shell.slack_link_generation.wrapping_add(1);
        shell.canvas_checklist.local_editor = Some(generation);
    }
    true
}
pub(crate) fn release_local(shell: &mut ShellState, generation: u64) {
    if shell.canvas_checklist.local_editor == Some(generation) {
        shell.canvas_checklist.local_editor = None;
    }
}
pub(crate) fn can_leave(shell: &ShellState) -> bool {
    shell.canvas_checklist.local_editor != Some(shell.shortcut_generation)
}

pub(crate) fn same_conversation(shell: &ShellState, main_view: crate::state::MainView) -> bool {
    shell.main_view == main_view && shell.overlay.is_none() && shell.conversation_visible()
}

pub(crate) fn visible(shell: &ShellState, file: &str) -> bool {
    if shell.canvas.as_ref().is_none_or(|target| target.id != file) {
        return false;
    }
    if shell.overlay == Some(Overlay::Canvas) {
        return true;
    }
    shell.overlay.is_none()
        && shell.conversation_visible()
        && shell.channel_tab == ChannelTab::Canvas
        && selection::target(shell).is_some_and(|target| target.id == file)
}
pub(super) fn mount(shell: &mut ShellState, target: &CanvasTarget) -> bool {
    if shell.overlay.is_some()
        || !shell.conversation_visible()
        || selection::target(shell).as_ref() != Some(target)
    {
        return false;
    }
    let generation = shell.shortcut_generation;
    release_local(shell, generation);
    shell.canvas = Some(target.clone());
    checklist::open_visit(shell);
    true
}

pub(super) fn focus(generation: u64) {
    dioxus::document::eval(&format!(
        r#"requestAnimationFrame(()=>{{const p=document.getElementById('channel-canvas-panel');if(p?.dataset.dialogGeneration==={} && document.activeElement===document.body)p.focus({{preventScroll:true}});}});"#,
        serde_json::json!(generation.to_string())
    ));
}

/// Keep an existing unsaved editor mounted after metadata invalidation. This
/// supplies no read/write authority: visible() still requires the current route.
fn retained_target(
    shell: &ShellState,
    original: Option<&CanvasTarget>,
    generation: u64,
) -> Option<CanvasTarget> {
    let original = original?;
    retains_local(shell, &original.id, generation).then(|| original.clone())
}

pub(super) fn retains_local(shell: &ShellState, file: &str, generation: u64) -> bool {
    shell.overlay.is_none()
        && shell.conversation_visible()
        && shell.channel_tab == ChannelTab::Canvas
        && shell.shortcut_generation == generation
        && shell.canvas_checklist.local_editor == Some(generation)
        && shell
            .canvas
            .as_ref()
            .is_some_and(|target| target.id == file)
}

#[component]
pub(crate) fn Panel(state: Signal<ShellState>) -> Element {
    let original = use_hook(|| selection::target(&state.peek()));
    let generation = use_hook(|| state.peek().shortcut_generation);
    let shell = state.read();
    if shell.overlay.is_some() {
        return rsx! {};
    }
    let Some(target) = retained_target(&shell, original.as_ref(), generation)
        .or_else(|| selection::target(&shell))
    else {
        return rsx! {section {id:"channel-canvas-unavailable",class:"channel-resources",role:"status","data-dialog-generation":shell.shortcut_generation,
            p {"This tab is no longer available in this conversation."}
            button {onclick:move |_|crate::channel_resources::select(state,ChannelTab::Messages),"Back to messages"}
        }};
    };
    rsx! {super::CanvasDialog {state,target,embedded:true}}
}

pub(super) fn fixture_target(target: &CanvasTarget) -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("channel-canvas-surface" | "channel-canvas-surface-narrow")
    ) && target.id == "F00000001"
}
pub(crate) fn install_fixture(shell: &mut ShellState) {
    crate::channel_tabs::fixture::install(shell);
    let Some(base) = crate::channel_tabs::inventory(shell) else {
        return;
    };
    let Some(route) = selection::CanvasRoute::capture(&base, "Ct00000001") else {
        return;
    };
    if selection::record(shell, selection::Route::Canvas(route), None) {
        shell.channel_tab = ChannelTab::Canvas;
        shell.overlay = None;
        shell.shortcut_generation = shell.shortcut_generation.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acquiring_canvas_editor_cancels_pending_link_once() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let generation = shell.shortcut_generation;
        let link = shell.slack_link_generation;
        assert!(hold_local(&mut shell, generation));
        assert_ne!(shell.slack_link_generation, link);
        let reserved = shell.slack_link_generation;
        assert!(hold_local(&mut shell, generation));
        assert_eq!(shell.slack_link_generation, reserved);
        release_local(&mut shell, generation);
        assert!(hold_local(&mut shell, generation));
        assert_ne!(shell.slack_link_generation, reserved);
    }
    #[test]
    fn embedded_owner_requires_exact_channel_route_and_visible_surface() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        install_fixture(&mut shell);
        let target = selection::target(&shell).unwrap();
        assert!(mount(&mut shell, &target));
        assert!(visible(&shell, &target.id));
        assert!(checklist::presentation(&shell).is_some());
        shell.overlay = Some(Overlay::Palette);
        assert!(!visible(&shell, &target.id));
        assert!(checklist::presentation(&shell).is_none());
        shell.overlay = None;
        shell.channel_tab = ChannelTab::Messages;
        assert!(!visible(&shell, &target.id));
    }
    #[test]
    fn modal_canvas_keeps_its_existing_independent_destination() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.canvas = Some(CanvasTarget {
            id: "Fmodal".into(),
            title: "Independent".into(),
            permalink: None,
        });
        shell.overlay = Some(Overlay::Canvas);
        assert!(visible(&shell, "Fmodal"));
        assert!(!visible(&shell, "Fother"));
    }

    #[test]
    fn old_read_cannot_adopt_after_tab_round_trip_or_account_rotation() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        install_fixture(&mut shell);
        let target = selection::target(&shell).unwrap();
        assert!(mount(&mut shell, &target));
        let old = checklist::begin_read(&mut shell).unwrap();
        assert!(old.owns_view(&shell));
        checklist::close_visit(&mut shell);
        shell.shortcut_generation += 1;
        assert!(mount(&mut shell, &target));
        assert!(!old.owns_view(&shell));
        let current = checklist::begin_read(&mut shell).unwrap();
        shell.core.account_epoch += 1;
        assert!(!current.owns_view(&shell));
    }

    #[test]
    fn local_editor_opened_after_native_observation_still_refuses_departure() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let generation = shell.shortcut_generation;
        assert!(can_leave(&shell));
        // Renderer observation can complete before the next input opens an editor.
        // The final serial Rust admission independently checks this synchronous lease.
        hold_local(&mut shell, generation);
        assert!(!can_leave(&shell));
        shell.shortcut_generation += 1;
        let replacement = shell.shortcut_generation;
        hold_local(&mut shell, replacement);
        release_local(&mut shell, generation);
        assert!(!can_leave(&shell));
        release_local(&mut shell, replacement);
        assert!(can_leave(&shell));
    }
    #[test]
    fn hidden_list_and_thread_only_surfaces_retire_embedded_read_ownership() {
        use crate::state::{MainView, SurfaceTarget};
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        install_fixture(&mut shell);
        let target = selection::target(&shell).unwrap();
        assert!(mount(&mut shell, &target));
        let read = checklist::begin_read(&mut shell).unwrap();
        for view in [
            MainView::Unreads,
            MainView::Threads,
            MainView::Files,
            MainView::Scheduled,
        ] {
            shell.main_view = view;
            assert!(!shell.conversation_visible());
            assert!(!visible(&shell, &target.id));
            assert!(!read.owns_view(&shell));
            assert!(!same_conversation(&shell, MainView::Home));
        }
        for view in [MainView::Activity, MainView::Later] {
            shell.main_view = view;
            shell.surfaces.insert(
                view,
                SurfaceTarget {
                    channel: shell.core.active_channel.clone().unwrap(),
                    thread_root: None,
                },
            );
            shell.thread_root = None;
            assert!(shell.conversation_visible());
            assert!(visible(&shell, &target.id));
            assert!(!same_conversation(&shell, MainView::Home));
            shell.thread_root = Some("1000.000001".into());
            assert!(!shell.conversation_visible());
            assert!(!read.owns_view(&shell));
        }
    }
    #[test]
    fn invalidated_route_keeps_exact_local_editor_without_read_authority() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        install_fixture(&mut shell);
        let target = selection::target(&shell).unwrap();
        assert!(mount(&mut shell, &target));
        let generation = shell.shortcut_generation;
        assert!(hold_local(&mut shell, generation));
        let read = checklist::begin_read(&mut shell).unwrap();
        let team = shell.core.active_team.clone().unwrap();
        shell
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .deleted_files
            .insert(target.id.clone());
        assert!(selection::target(&shell).is_none());
        assert!(!visible(&shell, &target.id));
        assert!(!read.owns_view(&shell));
        assert_eq!(
            retained_target(&shell, Some(&target), generation),
            Some(target.clone())
        );
        release_local(&mut shell, generation);
        assert!(retained_target(&shell, Some(&target), generation).is_none());
    }
}
