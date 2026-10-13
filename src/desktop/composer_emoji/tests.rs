use super::*;
use crate::media::MediaRegistry;
use super_platinum_core::slack::models::Emoji;

#[test]
fn stale_picker_close_cannot_dismiss_a_new_picker_visit() {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    let scope = Scope::capture(&shell, ComposerTarget::Channel).unwrap();
    shell.composer_emoji.active = Some(Visit { serial: 2, scope });
    shell.overlay = Some(Overlay::ComposerEmoji);
    close_visit(&mut shell, 1);
    assert_eq!(shell.overlay, Some(Overlay::ComposerEmoji));
    assert_eq!(shell.composer_emoji.active.as_ref().unwrap().serial, 2);
}

#[test]
fn composer_scope_rejects_account_destination_visit_and_surface_changes() {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    let scope = Scope::capture(&shell, ComposerTarget::Channel).unwrap();
    assert!(scope.current(&shell));
    shell.core.account_epoch += 1;
    assert!(!scope.current(&shell));
    shell.core.account_epoch -= 1;
    shell.channel_generation += 1;
    assert!(!scope.current(&shell));
    shell.channel_generation -= 1;
    shell.thread_generation += 1;
    assert!(!scope.current(&shell));
    shell.thread_generation -= 1;
    shell.main_view = MainView::Dms;
    assert!(!scope.current(&shell));
    shell.main_view = MainView::Home;
    shell.core.active_channel = Some("C1".into());
    assert!(!scope.current(&shell));
}

#[test]
fn unavailable_composer_and_missing_thread_cannot_admit_a_picker() {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    shell.thread_root = None;
    assert!(Scope::capture(&shell, ComposerTarget::Thread).is_none());
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .get_mut("C2")
        .unwrap()
        .is_archived = true;
    assert!(Scope::capture(&shell, ComposerTarget::Channel).is_none());
}

#[test]
fn standard_glyphs_and_custom_aliases_keep_correct_workspace_identity() {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    assert_eq!(insertion(&shell, "rocket").as_deref(), Some("🚀"));
    assert!(insertion(&shell, "unknown-not-a-choice").is_none());
    let custom = &mut shell.core.workspaces.get_mut("T1").unwrap().custom_emoji;
    for (name, value) in [
        ("rocket", "https://example.invalid/rocket.png"),
        ("ship_it", "alias:rocket"),
    ] {
        custom.insert(
            name.into(),
            Emoji {
                name: name.into(),
                value: value.into(),
                ..Default::default()
            },
        );
    }
    assert_eq!(insertion(&shell, "rocket").as_deref(), Some(":rocket:"));
    assert_eq!(insertion(&shell, "ship_it").as_deref(), Some(":ship_it:"));
}
