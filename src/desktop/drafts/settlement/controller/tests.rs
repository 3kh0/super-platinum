use super::*;
use crate::media::MediaRegistry;
use super_platinum_core::ComposerState;

fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
fn fixture() -> (ShellState, Visit) {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    shell.main_view = MainView::Scheduled;
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000089".into());
    let key = shell.composer_key(ComposerTarget::Channel).unwrap();
    shell.core.composer = ComposerState::with_text("Original");
    shell
        .core
        .drafts
        .items
        .insert(key.clone(), local("Original"));
    let visit = Visit {
        account: shell.core.active_account.clone().unwrap(),
        epoch: shell.core.account_epoch,
        transport: shell.core.transport.clone(),
        channel_generation: shell.channel_generation,
        thread_generation: shell.thread_generation,
        key: key.clone(),
        live: Live::capture(&shell, &key),
    };
    (shell, visit)
}
#[test]
fn selected_local_updates_only_unchanged_bound_content() {
    let (mut shell, visit) = fixture();
    apply_local(&mut shell, &visit, &local("Selected"));
    assert_eq!(
        shell.core.drafts.items[&visit.key].composer.text,
        "Selected"
    );
    assert_eq!(shell.core.composer.text, "Selected");
}
#[test]
fn newer_bound_typing_prevents_replacing_lagging_book() {
    let (mut shell, visit) = fixture();
    shell.core.composer = ComposerState::with_text("Newer typing");
    apply_local(&mut shell, &visit, &local("Selected"));
    assert_eq!(shell.core.composer.text, "Newer typing");
    assert_eq!(
        shell.core.drafts.items[&visit.key].composer.text,
        "Original"
    );
}
#[test]
fn navigation_or_new_account_never_receives_stale_review_projection() {
    let (mut shell, visit) = fixture();
    shell.main_view = MainView::Home;
    assert!(!visit.current(&shell));
    apply_local(&mut shell, &visit, &local("Selected"));
    assert_eq!(shell.core.composer.text, "Selected");
    shell.main_view = MainView::Scheduled;
    shell.core.active_account = Some("other".into());
    assert!(!visit.account_current(&shell));
}

#[test]
fn navigate_after_choice_without_typing_checkpoints_selected_content() {
    let (mut shell, visit) = fixture();
    shell.main_view = MainView::Home;
    shell.channel_generation += 1;
    let field = apply_local(&mut shell, &visit, &local("Selected after review"));
    assert_eq!(field.as_ref().unwrap().2, "Original");
    assert_eq!(field.as_ref().unwrap().3, "Selected after review");
    // Exercise the real capture path with persistence explicitly disconnected.
    shell.core.active_account = None;
    shell.checkpoint_composers();
    assert_eq!(shell.core.composer.text, "Selected after review");
    assert_eq!(
        shell.core.drafts.items[&visit.key].composer.text,
        "Selected after review"
    );
}
