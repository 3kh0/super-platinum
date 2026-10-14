use super::*;

fn artifact(kind: Kind) -> Artifact {
    Artifact {
        kind,
        expected: if kind == Kind::Shortcut {
            Contents::File(vec![1, 2, 3])
        } else {
            Contents::Registry(vec![])
        },
        created: true,
        complete: true,
        removed: false,
    }
}
fn ledger() -> Ledger {
    Ledger {
        version: VERSION,
        nonce: "0123456789abcdefabcdef0123456789".into(),
        executable: Executable {
            path: r"C:\fixture\app.exe".into(),
            sha256: "a".repeat(64),
        },
        owner: Process {
            pid: 12,
            created: 99,
        },
        child: None,
        created_ms: 1,
        phase: Phase::Registered,
        artifacts: [Kind::Shortcut, Kind::Clsid, Kind::LocalServer, Kind::AppId]
            .into_iter()
            .map(artifact)
            .collect(),
        preview: None,
        preview_removed: false,
    }
}
fn valid(value: &Ledger) -> bool {
    Ledger::parse(
        &serde_json::to_vec(value).unwrap(),
        &value.nonce,
        &value.executable,
    )
    .is_ok()
}
fn reserve(value: &mut Ledger) {
    value
        .reserve_preview(value.nonce[..16].into(), value.nonce[16..].into())
        .unwrap();
}
fn ready() -> Ledger {
    let mut value = ledger();
    reserve(&mut value);
    value.phase = Phase::Ready;
    value
}
fn child() -> Process {
    Process {
        pid: 13,
        created: 100,
    }
}

#[test]
fn rejects_wrong_nonce_executable_unknown_fields_and_oversized_ledgers() {
    let value = ledger();
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(valid(&value));
    for nonce in ["../elsewhere", "0123456789abcdef0123456789abcdee"] {
        assert!(Ledger::parse(&bytes, nonce, &value.executable).is_err());
    }
    let mut other = value.executable.clone();
    other.sha256 = "b".repeat(64);
    assert!(Ledger::parse(&bytes, &value.nonce, &other).is_err());
    other = value.executable.clone();
    other.path = r"C:\different.exe".into();
    assert!(Ledger::parse(&bytes, &value.nonce, &other).is_err());
    let mut unknown = serde_json::to_value(&value).unwrap();
    unknown["arbitrary_path"] = "C:\\victim".into();
    assert!(
        Ledger::parse(
            &serde_json::to_vec(&unknown).unwrap(),
            &value.nonce,
            &value.executable
        )
        .is_err()
    );
    assert!(Ledger::parse(&vec![b' '; MAX_LEDGER + 1], &value.nonce, &value.executable).is_err());
    // Older ledgers without explicit creation proof must never imply ownership.
    let mut missing = serde_json::to_value(&value).unwrap();
    missing["artifacts"][0]
        .as_object_mut()
        .unwrap()
        .remove("created");
    assert!(
        Ledger::parse(
            &serde_json::to_vec(&missing).unwrap(),
            &value.nonce,
            &value.executable
        )
        .is_err()
    );
}

#[test]
fn acquisition_accepts_only_ordered_prefix_with_one_final_interruption_or_disown() {
    for count in 0..=4 {
        let mut value = ledger();
        value.phase = Phase::Acquiring;
        value.artifacts.truncate(count);
        assert!(valid(&value)); // Includes the immutable initial owner lease.
        if count == 0 {
            continue;
        }
        let last = count - 1;
        value.artifacts[last].complete = false;
        assert!(valid(&value)); // Created, interrupted write.
        value.artifacts[last].created = false;
        assert!(valid(&value)); // Planned, creation outcome unconfirmed.
        value.artifacts[last].removed = true;
        assert!(valid(&value)); // Exclusive create collided; target disowned.
        value.artifacts[last].created = true;
        assert!(!valid(&value)); // Acquiring never removes an owned artifact.
    }
    let mut value = ledger();
    value.phase = Phase::Acquiring;
    value.artifacts[0].complete = false;
    assert!(!valid(&value));
    value = ledger();
    value.phase = Phase::Acquiring;
    value.artifacts.swap(0, 1);
    assert!(!valid(&value));
    value = ledger();
    value.artifacts[1].kind = Kind::Shortcut;
    assert!(!valid(&value));
    value = ledger();
    value.artifacts[0].created = false;
    assert!(!valid(&value));
}

#[test]
fn registration_and_handoff_phases_require_complete_unremoved_artifacts_and_preview() {
    for phase in [
        Phase::Registered,
        Phase::Reserved,
        Phase::Submitted,
        Phase::Ready,
    ] {
        let mut value = ledger();
        if phase != Phase::Registered {
            reserve(&mut value);
        }
        value.phase = phase;
        assert!(valid(&value));
        for index in 0..4 {
            let mut bad = value.clone();
            bad.artifacts[index].complete = false;
            assert!(!valid(&bad));
            bad = value.clone();
            bad.artifacts[index].removed = true;
            assert!(!valid(&bad));
        }
        let mut bad = value.clone();
        bad.artifacts.pop();
        assert!(!valid(&bad));
        bad = value.clone();
        bad.preview_removed = true;
        assert!(!valid(&bad));
        bad = value.clone();
        bad.preview = if phase == Phase::Registered {
            ready().preview
        } else {
            None
        };
        assert!(!valid(&bad));
    }
    let mut value = ready();
    value.phase = Phase::Acquiring;
    assert!(!valid(&value));
}

#[test]
fn preview_identity_is_nonce_bound_and_reservation_never_replays() {
    let mut value = ledger();
    let before = serde_json::to_value(&value).unwrap();
    assert!(
        value
            .reserve_preview("a".repeat(16), "b".repeat(16))
            .is_err()
    );
    assert_eq!(serde_json::to_value(&value).unwrap(), before);
    reserve(&mut value);
    assert!(valid(&value));
    for phase in [
        Phase::Reserved,
        Phase::Submitted,
        Phase::Ready,
        Phase::Cleaning,
        Phase::Cleaned,
    ] {
        let mut used = value.clone();
        used.phase = phase;
        assert!(
            used.reserve_preview(used.nonce[..16].into(), used.nonce[16..].into())
                .is_err()
        );
    }
    let mut bad = value.clone();
    bad.preview.as_mut().unwrap().tag = "a".repeat(16);
    assert!(!valid(&bad));
    bad = value;
    bad.preview.as_mut().unwrap().group = "b".repeat(16);
    assert!(!valid(&bad));
}

#[test]
fn child_claim_requires_ready_complete_registration_exact_process_and_parent_departure() {
    let mut value = ready();
    assert!(value.claim_child(child(), true).is_err());
    for candidate in [
        value.owner.clone(),
        Process {
            pid: 0,
            created: 100,
        },
        Process {
            pid: 13,
            created: 0,
        },
        Process {
            pid: 13,
            created: 98,
        },
    ] {
        assert!(value.claim_child(candidate, false).is_err());
        assert!(value.child.is_none());
    }
    for phase in [
        Phase::Acquiring,
        Phase::Registered,
        Phase::Reserved,
        Phase::Submitted,
        Phase::Cleaning,
        Phase::Cleaned,
    ] {
        let mut bad = ready();
        bad.phase = phase;
        assert!(bad.claim_child(child(), false).is_err());
        bad.child = Some(child());
        assert!(!valid(&bad) || matches!(phase, Phase::Cleaning));
    }
    for index in 0..4 {
        let mut bad = ready();
        bad.artifacts[index].complete = false;
        assert!(bad.claim_child(child(), false).is_err());
        assert!(bad.child.is_none());
    }
    value.claim_child(child(), false).unwrap();
    assert!(valid(&value));
    assert!(
        value
            .claim_child(
                Process {
                    pid: 14,
                    created: 101
                },
                false
            )
            .is_err()
    );
    // A later process may legitimately reuse the exited parent's PID.
    let mut reused = ready();
    reused
        .claim_child(
            Process {
                pid: 12,
                created: 101,
            },
            false,
        )
        .unwrap();
    assert!(valid(&reused));
}

#[test]
fn cleanup_accepts_interrupted_registration_and_reverse_removal_without_erasing_proof() {
    for count in 0..=4 {
        let mut value = ledger();
        value.phase = Phase::Cleaning;
        value.artifacts.truncate(count);
        if let Some(last) = value.artifacts.last_mut() {
            last.complete = false;
            last.created = false;
        }
        assert!(valid(&value));
        value.preview_removed = true;
        for index in (0..count).rev() {
            value.artifacts[index].removed = true;
            assert!(valid(&value));
        }
        value.phase = Phase::Cleaned;
        assert!(valid(&value));
    }
    let mut value = ready();
    value.claim_child(child(), false).unwrap();
    value.phase = Phase::Cleaning;
    assert!(valid(&value));
    value.artifacts[3].removed = true;
    assert!(!valid(&value));
    value.preview_removed = true;
    assert!(valid(&value));
    value.artifacts[1].removed = true;
    assert!(!valid(&value)); // Gap in reverse removal.
    value.artifacts[2].removed = true;
    assert!(valid(&value));
    value.phase = Phase::Cleaned;
    assert!(!valid(&value));
    value.artifacts[0].removed = true;
    assert!(valid(&value));
    value.preview_removed = false;
    assert!(!valid(&value));
    value = ledger();
    value.phase = Phase::Cleaning;
    value.artifacts[3].created = false;
    value.artifacts[3].complete = false;
    value.artifacts[3].removed = true;
    assert!(valid(&value)); // Disowned tail predates preview cleanup.
}

#[test]
fn cleanup_distinguishes_reused_pids_and_requires_valid_caller_and_ledger() {
    let mut value = ready();
    let reused = Process {
        pid: 12,
        created: 101,
    };
    assert!(!value.cleanup_allowed(&reused, true, false));
    assert!(value.cleanup_allowed(&reused, false, false));
    assert!(value.cleanup_allowed(&value.owner, true, false));
    assert!(!value.cleanup_allowed(&Process { pid: 0, created: 1 }, false, false));
    value.claim_child(child(), false).unwrap();
    assert!(!value.cleanup_allowed(&value.owner, false, true));
    assert!(value.cleanup_allowed(&child(), false, true));
    assert!(!value.cleanup_allowed(
        &Process {
            pid: 13,
            created: 101
        },
        false,
        true
    ));
    value.preview = None;
    assert!(!value.cleanup_allowed(&child(), false, false));
}

#[test]
fn only_created_partial_exact_writes_are_recoverable() {
    let mut value = artifact(Kind::Shortcut);
    value.complete = false;
    assert!(value.unchanged(&Contents::File(vec![])));
    assert!(value.unchanged(&Contents::File(vec![1, 2])));
    assert!(!value.unchanged(&Contents::File(vec![1, 9])));
    value.created = false;
    assert!(!value.unchanged(&Contents::File(vec![])));
    assert!(!value.unchanged(&value.expected));
    value.created = true;
    value.complete = true;
    assert!(!value.unchanged(&Contents::File(vec![1, 2])));
    assert!(value.unchanged(&value.expected));
    value.removed = true;
    assert!(!value.unchanged(&value.expected));
    let expected = RegistryValue {
        name: "CustomActivator".into(),
        kind: 1,
        bytes: vec![1, 2],
    };
    value = Artifact {
        kind: Kind::AppId,
        expected: Contents::Registry(vec![expected.clone()]),
        created: true,
        complete: false,
        removed: false,
    };
    assert!(value.unchanged(&Contents::Registry(vec![])));
    assert!(!value.unchanged(&Contents::Registry(vec![RegistryValue {
        name: "Other".into(),
        ..expected
    }])));
    value.created = false;
    assert!(!value.unchanged(&Contents::Registry(vec![])));
}
