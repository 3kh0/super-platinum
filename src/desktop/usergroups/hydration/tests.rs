use super::*;
use crate::media::MediaRegistry;
use super_platinum_core::slack::models::Channel;

fn fixture(extra_channels: usize) -> ShellState {
    fixture_with_media(extra_channels, MediaRegistry::default())
}

fn fixture_with_media(extra_channels: usize, media: MediaRegistry) -> ShellState {
    let mut shell = ShellState::fixture(media);
    let workspace = shell.core.workspaces.get_mut("T1").unwrap();
    for i in 0..extra_channels {
        let id = format!("D{i:08}");
        workspace.channels.insert(
            id.clone(),
            Channel {
                id,
                is_im: true,
                user: Some(format!("U{i:08}")),
                ..Default::default()
            },
        );
    }
    workspace.usergroups.insert(
        "S_TEST".into(),
        UserGroup {
            id: "S_TEST".into(),
            users: vec!["U0".into()],
            members_loaded: true,
            ..Default::default()
        },
    );
    shell.refresh_from_core();
    shell
}

fn group(handle: &str) -> UserGroup {
    UserGroup {
        id: "S_TEST".into(),
        handle: handle.into(),
        name: "Test group".into(),
        users: vec!["not-authoritative".into()],
        ..Default::default()
    }
}

#[test]
fn empty_success_keeps_projection_allocations_geometry_and_metadata_untouched() {
    let mut shell = fixture(20);
    let channels = shell.channels.as_ptr();
    let messages = shell.messages.as_ptr();
    let rows = shell.channel_rows.as_ptr();
    let bounds = (shell.timeline_start, shell.timeline_end);
    let old = serde_json::to_value(&shell.core.workspaces["T1"].usergroups).unwrap();
    let mut progress = Progress::default();
    for _ in 0..3 {
        assert!(progress.apply(&mut shell, "T1", Ok(vec![])));
    }
    assert!(!progress.should_persist());
    assert_eq!(shell.channels.as_ptr(), channels);
    assert_eq!(shell.messages.as_ptr(), messages);
    assert_eq!(shell.channel_rows.as_ptr(), rows);
    assert_eq!((shell.timeline_start, shell.timeline_end), bounds);
    assert_eq!(
        serde_json::to_value(&shell.core.workspaces["T1"].usergroups).unwrap(),
        old
    );
}

#[test]
fn mixed_batches_project_nonempty_immediately_and_preserve_loaded_membership() {
    // Both projections must resolve the same avatar URLs through one registry:
    // independently created registries intentionally issue different opaque IDs.
    let media = MediaRegistry::default();
    let mut shell = fixture_with_media(20, media.clone());
    let mut reference = fixture_with_media(20, media);
    let mut progress = Progress::default();
    assert!(progress.apply(&mut shell, "T1", Ok(vec![])));
    let previous_projection = shell.channels.as_ptr();
    assert!(progress.apply(&mut shell, "T1", Ok(vec![group("resolved")])));
    assert_ne!(shell.channels.as_ptr(), previous_projection);
    let actual = &shell.core.workspaces["T1"].usergroups["S_TEST"];
    assert_eq!(actual.handle, "resolved");
    assert_eq!(actual.users, ["U0"]);
    assert!(actual.members_loaded);
    // Former nonempty batch behavior remains the reference projection.
    let mut expected = group("resolved");
    expected.users = vec!["U0".into()];
    expected.members_loaded = true;
    reference
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .usergroups
        .insert(expected.id.clone(), expected);
    reference.refresh_from_core();
    assert_eq!(shell.messages, reference.messages);
    assert_eq!(shell.channels, reference.channels);
    assert_eq!(shell.sidebar_sections, reference.sidebar_sections);
    let projected = shell.channels.as_ptr();
    assert!(progress.apply(&mut shell, "T1", Ok(vec![])));
    assert_eq!(shell.channels.as_ptr(), projected);
    assert!(progress.should_persist());
}

#[test]
fn failed_later_batch_keeps_prior_visible_success_without_persistence_or_replay() {
    let mut shell = fixture(20);
    let mut progress = Progress::default();
    assert!(progress.apply(&mut shell, "T1", Ok(vec![group("first")])));
    let projected = shell.channels.as_ptr();
    assert!(!progress.apply(&mut shell, "T1", Err(Error::Offline("fixture".into()))));
    assert!(!progress.should_persist());
    assert_eq!(shell.channels.as_ptr(), projected);
    assert_eq!(
        shell.core.workspaces["T1"].usergroups["S_TEST"].handle,
        "first"
    );
    assert!(!progress.apply(&mut shell, "T1", Ok(vec![group("not-applied")])));
    assert_eq!(
        shell.core.workspaces["T1"].usergroups["S_TEST"].handle,
        "first"
    );
    // Failure adds no negative cache: a separate invocation can still hydrate.
    let mut next = Progress::default();
    assert!(next.apply(&mut shell, "T1", Ok(vec![group("retry")])));
    assert!(next.should_persist());
    assert_eq!(
        shell.core.workspaces["T1"].usergroups["S_TEST"].handle,
        "retry"
    );
}

#[test]
fn first_error_or_removed_workspace_never_requests_persistence() {
    let mut shell = fixture(0);
    let projected = shell.channels.as_ptr();
    let mut failed = Progress::default();
    assert!(!failed.apply(&mut shell, "T1", Err(Error::Api("fixture".into()))));
    assert!(!failed.should_persist());
    let mut removed = Progress::default();
    assert!(!removed.apply(&mut shell, "missing", Ok(vec![])));
    assert!(!removed.should_persist());
    assert_eq!(shell.channels.as_ptr(), projected);
}

#[test]
#[ignore = "paired actual full-shell projection versus empty hydration commit"]
fn empty_group_hydration_projection_cost() {
    let mut shell = fixture(2000);
    let mut before = Vec::new();
    let mut after = Vec::new();
    for round in 0..45 {
        for optimized in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let start = std::time::Instant::now();
            if optimized {
                let mut progress = Progress::default();
                assert!(progress.apply(&mut shell, "T1", Ok(vec![])));
                std::hint::black_box(progress.should_persist());
            } else {
                // Former successful-empty response path performs this complete
                // projection even though no group was inserted.
                shell.refresh_from_core();
            }
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            std::hint::black_box(&shell);
            if round >= 5 {
                if optimized { &mut after } else { &mut before }.push(ms);
            }
        }
    }
    for (label, mut samples) in [("full_projection", before), ("empty_commit", after)] {
        samples.sort_by(f64::total_cmp);
        println!(
            "empty_group_hydration mode={label} channels={} samples={} median_ms={:.6} p95_ms={:.6}",
            shell.channels.len(),
            samples.len(),
            (samples[19] + samples[20]) / 2.0,
            samples[37]
        );
    }
}
