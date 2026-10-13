use super::*;

fn key(channel: &str) -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: channel.into(),
        thread: None,
    }
}
fn shell() -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "drafts-empty");
    for channel in ["C_COLD", "C_OTHER"] {
        shell.core.drafts.capture(
            &key(channel),
            &super_platinum_core::ComposerState::with_text(format!("Saved {channel} 🚀")),
            &[],
        );
    }
    shell
}
fn info(id: &str) -> Channel {
    Channel {
        id: id.into(),
        name: Some("restored-conversation".into()),
        is_channel: true,
        extra: std::collections::BTreeMap::from([("is_member".into(), serde_json::json!(true))]),
        ..Default::default()
    }
}

#[test]
fn cold_destination_resolves_before_navigation_and_retains_both_drafts() {
    let mut shell = shell();
    let outgoing = shell.core.active_channel.clone().unwrap();
    shell.core.composer = super_platinum_core::ComposerState::with_text("Outgoing unsent text");
    let mut progress = Progress::default();
    let visit = Visit::begin(&shell, &mut progress, key("C_COLD")).unwrap();
    assert!(!visit.finish(&mut shell, &progress));
    assert!(visit.resolve(&mut shell, &progress, info("C_COLD"), 0));
    assert_eq!(shell.core.active_channel.as_ref(), Some(&outgoing));
    assert!(visit.finish(&mut shell, &progress));
    assert_eq!(shell.main_view, MainView::Home);
    assert_eq!(shell.core.active_channel.as_deref(), Some("C_COLD"));
    assert_eq!(shell.core.composer.text, "Saved C_COLD 🚀");
    assert_eq!(
        shell.core.drafts.items[&key(&outgoing)].composer.text,
        "Outgoing unsent text"
    );
}

#[test]
fn archived_destination_projects_without_rebinding_outgoing_draft() {
    let mut shell = shell();
    let outgoing = shell.core.active_channel.clone().unwrap();
    let mut channel = info("C_COLD");
    channel.is_archived = true;
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .insert(channel.id.clone(), channel);
    shell.refresh_from_core();
    assert!(!shell.channels.iter().any(|c| c.id == "C_COLD"));
    shell.core.composer.text = "Keep current text".into();
    let mut progress = Progress::default();
    let visit = Visit::begin(&shell, &mut progress, key("C_COLD")).unwrap();
    assert!(visit.finish(&mut shell, &progress));
    assert!(shell.composer_unavailable().is_some());
    assert_eq!(
        shell.core.drafts.items[&key(&outgoing)].composer.text,
        "Keep current text"
    );
    assert_eq!(
        shell.core.drafts.items[&key("C_COLD")].composer.text,
        "Saved C_COLD 🚀"
    );
}

#[test]
fn stale_navigation_accounts_transport_and_deleted_drafts_cannot_hydrate() {
    let changes: Vec<Box<dyn Fn(&mut ShellState)>> = vec![
        Box::new(|s| s.core.account_epoch += 1),
        Box::new(|s| s.core.active_team = Some("OTHER".into())),
        Box::new(|s| s.core.transport = Some(Arc::new(Transport::new("fixture").unwrap()))),
        Box::new(|s| s.channel_generation += 1),
        Box::new(|s| s.thread_generation += 1),
        Box::new(|s| s.main_view = MainView::Home),
        Box::new(|s| s.overlay = Some(Overlay::Search)),
        Box::new(|s| s.shortcut_generation += 1),
        Box::new(|s| s.dismiss_overlay()),
        Box::new(|s| {
            s.core.drafts.items.remove(&key("C_COLD"));
        }),
    ];
    for change in changes {
        let mut shell = shell();
        let mut progress = Progress::default();
        let visit = Visit::begin(&shell, &mut progress, key("C_COLD")).unwrap();
        change(&mut shell);
        assert!(!visit.resolve(&mut shell, &progress, info("C_COLD"), 0));
        assert!(!visit.finish(&mut shell, &progress));
        assert!(!shell.core.workspaces["T1"].channels.contains_key("C_COLD"));
    }
}

#[test]
fn newer_draft_request_wins_even_when_returning_to_the_same_draft() {
    let mut shell = shell();
    let mut progress = Progress::default();
    let first = Visit::begin(&shell, &mut progress, key("C_COLD")).unwrap();
    assert!(Visit::begin(&shell, &mut progress, key("C_COLD")).is_none());
    let second = Visit::begin(&shell, &mut progress, key("C_OTHER")).unwrap();
    let third = Visit::begin(&shell, &mut progress, key("C_COLD")).unwrap();
    assert!(!first.resolve(&mut shell, &progress, info("C_COLD"), 0));
    assert!(!second.resolve(&mut shell, &progress, info("C_OTHER"), 0));
    assert!(!third.resolve(&mut shell, &progress, info("WRONG"), 0));
    assert!(third.resolve(&mut shell, &progress, info("C_COLD"), 0));
    assert!(third.finish(&mut shell, &progress));
}

#[test]
fn thread_draft_restores_to_its_root_and_preserves_the_channel_composer() {
    let mut shell = shell();
    let mut thread_key = key("C_COLD");
    thread_key.thread = Some("10.2".into());
    shell.core.drafts.capture(
        &thread_key,
        &super_platinum_core::ComposerState::with_text("Unsent reply"),
        &[],
    );
    let mut progress = Progress::default();
    let visit = Visit::begin(&shell, &mut progress, thread_key).unwrap();
    assert!(visit.resolve(&mut shell, &progress, info("C_COLD"), 0));
    assert!(visit.finish(&mut shell, &progress));
    shell.begin_thread("10.2".into(), None);
    assert_eq!(shell.core.thread_composer.text, "Unsent reply");
    assert_eq!(shell.core.composer.text, "Saved C_COLD 🚀");
}
