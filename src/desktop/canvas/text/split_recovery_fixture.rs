//! Isolated restored split scene. A checkpoint is data only; never replay it.
use super::*;
use super_platinum_core::{
    canvas::{
        edit::structure::{
            EndIntent, EndWitness, SplitIntent, SplitWitness,
            recovery::{Checkpoint, Scope},
        },
        edit::text::TextDraft,
        wire::{EncodedField as F, RawMessage, Section},
    },
    slack::api::CanvasCollabSnapshot,
};

pub(super) const ACCOUNT: &str = "00000000-0000-4000-8000-000000009994";
pub(in crate::canvas) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-local-review" | "canvas-local-review-narrow")
        )
        && matches!(
            std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_RECOVERY_CHECK").as_deref(),
            Ok("seed" | "restart" | "review" | "storage-failure" | "reviewed")
        )
}
pub(in crate::canvas) fn enabled(shell: &ShellState) -> bool {
    requested()
        && structure_fixture::enabled(shell)
        && shell.core.active_account.as_deref() == Some(ACCOUNT)
        && shell.self_account.user_id == "U1"
}
pub(super) fn scope() -> Scope {
    Scope {
        workspace: "T1".into(),
        user: "U1".into(),
        file: "Ffixture".into(),
    }
}
pub(in crate::canvas) fn snapshot() -> Arc<CanvasCollabSnapshot> {
    if super::split_fixture::pre_enter::enabled()
        || super::split_fixture::parent_edit::enabled()
        || super::split_fixture::second_middle::enabled()
    {
        return super::split_fixture::pre_enter::snapshot(&structure_fixture::snapshot());
    }
    if super::split_fixture::is_sequence() {
        return super::split_fixture::sequence::snapshot(&super::structure_fixture::snapshot());
    }
    if super::split_fixture::is_repeat() {
        return super::split_fixture::repeat_snapshot(&structure_fixture::snapshot());
    }
    if super::split_fixture::is_empty() {
        return super::split_fixture::empty_snapshot(&structure_fixture::snapshot());
    }
    let mut snapshot = (*structure_fixture::snapshot()).clone();
    let paragraph = snapshot
        .sections
        .iter_mut()
        .find(|s| s.id() == "paragraph")
        .unwrap();
    let text = RawMessage::from_fields(&[
        F::bytes(1, super::split_fixture::corpus().seed_rtml.as_bytes()).unwrap(),
        F::bytes(2, b"en").unwrap(),
    ])
    .unwrap();
    let body = RawMessage::from_fields(&[F::message(1, &text).unwrap()]).unwrap();
    *paragraph = Section::from_raw(super::split_fixture::parent_source(
        &paragraph
            .raw()
            .replace_fields(12, &[F::message(12, &body).unwrap()])
            .unwrap(),
    ))
    .unwrap();
    super::split_fixture::reuse_source(Arc::new(snapshot))
}
pub(super) fn install_owner(shell: &mut ShellState) -> bool {
    if !requested()
        || shell.core.session.is_some()
        || shell.core.transport.is_some()
        || shell.core.active_team.as_deref() != Some("T1")
    {
        return false;
    }
    // A scope-keyed remount must not reseed a replaced account back to the
    // original fixture owner. Initial scene setup is the only owner mutation.
    if shell.canvas_checklist.structure_fixture.is_some() {
        return shell.core.active_account.as_deref() == Some(ACCOUNT)
            && shell.self_account.user_id == "U1"
            && shell
                .core
                .workspaces
                .get("T1")
                .is_some_and(|w| w.self_user_id == "U1");
    }
    let Some(workspace) = shell.core.workspaces.get_mut("T1") else {
        return false;
    };
    shell.core.active_account = Some(ACCOUNT.into());
    workspace.self_user_id = "U1".into();
    shell.self_account.user_id = "U1".into();
    true
}
/// Seed-only authoring, never called for a restored receipt. No request is sent.
pub(super) fn checkpoint() -> Checkpoint {
    if super::split_fixture::is_sequence() {
        return super::split_fixture::sequence::checkpoint();
    }
    if super::split_fixture::is_repeat() {
        return super::split_fixture::repeat_checkpoint();
    }
    let source = snapshot();
    let now = checklist::save::now_usec().unwrap();
    if super::split_fixture::is_empty() {
        let witness = EndWitness::empty();
        let witness = if super::split_fixture::typing() != "none" {
            witness
                .with_child_updates(&super::split_fixture::typing_updates())
                .unwrap()
        } else {
            witness
        };
        return EndIntent::capture_empty(&source, super::split_fixture::parent_id(), now)
            .unwrap()
            .prepare(&source, &witness, now)
            .unwrap()
            .checkpoint()
            .unwrap();
    }
    if super::split_fixture::is_start_typing() {
        let intent = super_platinum_core::canvas::edit::structure::StartTypingIntent::capture(
            &source,
            "paragraph",
            now,
        )
        .unwrap();
        let witness = super_platinum_core::canvas::edit::structure::StartTypingWitness::new(
            intent.seed(),
            &super::split_fixture::typing_updates(),
        )
        .unwrap();
        return intent
            .prepare(&source, &witness, now)
            .unwrap()
            .checkpoint()
            .unwrap();
    }
    if super::split_fixture::is_start() {
        return super_platinum_core::canvas::edit::structure::StructureIntent::insert(
            &source,
            "paragraph",
            super_platinum_core::canvas::edit::structure::Side::Before,
            &TextDraft::default(),
            now,
        )
        .unwrap()
        .prepare(&source, now)
        .unwrap()
        .checkpoint()
        .unwrap();
    }
    if super::split_fixture::is_end() {
        let intent = EndIntent::capture(&source, "paragraph", now).unwrap();
        let witness = EndWitness::new(
            intent.seed(),
            super::split_fixture::corpus().offset,
            intent.seed(),
            &TextDraft::default(),
        )
        .unwrap();
        let witness = if super::split_fixture::typing() != "none" {
            witness
                .with_child_updates(&super::split_fixture::typing_updates())
                .unwrap()
        } else {
            witness
        };
        return intent
            .prepare(&source, &witness, now)
            .unwrap()
            .checkpoint()
            .unwrap();
    }
    let intent = SplitIntent::capture(&source, super::split_fixture::parent_id(), now).unwrap();
    if super::split_fixture::second_middle::enabled() {
        let witness = super::split_fixture::second_middle::witness(intent.seed());
        return intent
            .prepare(&source, &witness, now)
            .unwrap()
            .checkpoint()
            .unwrap();
    }
    if super::split_fixture::parent_edit::enabled() {
        let witness = super::split_fixture::parent_edit::witness(intent.seed());
        return intent
            .prepare(&source, &witness, now)
            .unwrap()
            .checkpoint()
            .unwrap();
    }
    if super::split_fixture::pre_enter::enabled() {
        let witness = super::split_fixture::pre_enter::witness(intent.seed());
        return intent
            .prepare(&source, &witness, now)
            .unwrap()
            .checkpoint()
            .unwrap();
    }
    let witness = SplitWitness::new(
        intent.seed(),
        super::split_fixture::corpus().offset,
        &TextDraft::from_rtml(super::split_fixture::corpus().left_rtml).unwrap(),
        &TextDraft::from_rtml(
            &super::split_fixture::corpus()
                .right
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;"),
        )
        .unwrap(),
    )
    .unwrap();
    let witness = if super::split_fixture::is_middle_typing() {
        witness
            .with_child_updates(&super::split_fixture::typing_updates())
            .unwrap()
    } else {
        witness
    };
    intent
        .prepare(&source, &witness, now)
        .unwrap()
        .checkpoint()
        .unwrap()
}
pub(super) fn matches_typing(checkpoint: &Checkpoint) -> bool {
    if super::split_fixture::second_middle::enabled() {
        return super::split_fixture::second_middle::matches(checkpoint);
    }
    if super::split_fixture::parent_edit::enabled() {
        return super::split_fixture::parent_edit::matches(checkpoint);
    }
    if super::split_fixture::pre_enter::enabled() {
        return super::split_fixture::pre_enter::matches(checkpoint);
    }
    if super::split_fixture::is_sequence() {
        return super::split_fixture::sequence::matches(checkpoint);
    }
    if super::split_fixture::is_repeat() {
        return super::split_fixture::matches_repeat(checkpoint);
    }
    if super::split_fixture::is_empty() {
        return super::split_fixture::matches_empty(checkpoint);
    }
    let Ok(value) = serde_json::to_value(checkpoint) else {
        return false;
    };
    if super::split_fixture::is_start_typing() {
        let expected = super::split_fixture::typing_updates()
            .iter()
            .map(|d| d.rtml().unwrap())
            .collect::<Vec<_>>();
        return value["version"] == 9
            && value["action"]["StartTyping"]["original_updates"] == serde_json::json!(expected);
    }
    if super::split_fixture::is_start() {
        return value["version"] == 1
            && value["action"]["Insert"]["anchor"] == "paragraph"
            && value["action"]["Insert"]["before"] == true
            && checkpoint.draft_rtml().is_empty();
    }
    if super::split_fixture::reuse_content() != "empty" {
        let expected = super::split_fixture::typing_updates()
            .iter()
            .map(|draft| draft.rtml().unwrap())
            .collect::<Vec<_>>();
        return value["version"] == 7
            && value["action"]["EndClearing"]["child_updates"] == serde_json::json!(expected);
    }
    if super::split_fixture::is_middle_typing() {
        let expected = super::split_fixture::typing_updates()
            .iter()
            .map(|d| d.rtml().unwrap())
            .collect::<Vec<_>>();
        return value["version"] == 8
            && value["action"]["SplitTyping"]["child_updates"] == serde_json::json!(expected);
    }
    let action = &value["action"]["EndTyping"];
    if super::split_fixture::typing() == "none" {
        return action.is_null() && value["action"]["SplitTyping"].is_null();
    }
    let expected = super::split_fixture::typing_updates()
        .iter()
        .map(|draft| draft.rtml().unwrap())
        .collect::<Vec<_>>();
    value["version"] == 6
        && action["reused"] == super::split_fixture::is_reuse()
        && action["child_updates"] == serde_json::json!(expected)
}
pub(super) fn matches_reuse_base(checkpoint: &Checkpoint) -> bool {
    if super::split_fixture::is_sequence() {
        return super::split_fixture::sequence::matches(checkpoint);
    }
    if super::split_fixture::is_repeat() {
        return super::split_fixture::matches_repeat(checkpoint);
    }
    if super::split_fixture::is_empty() {
        return super::split_fixture::matches_empty(checkpoint);
    }
    if !super::split_fixture::is_reuse() {
        return !checkpoint.is_end_reuse();
    }
    let Ok(value) = serde_json::to_value(checkpoint) else {
        return false;
    };
    let expected = super::split_fixture::reuse_child();
    let action = &value["action"];
    if action["EndReuse"]["child"] != expected
        && action["EndTyping"]["child"] != expected
        && action["EndClearing"]["child"] != expected
    {
        return false;
    }
    let Some(sections) = value["sections"].as_array() else {
        return false;
    };
    sections.iter().any(|value| {
        let Ok(bytes) = serde_json::from_value::<Vec<u8>>(value.clone()) else {
            return false;
        };
        let Ok(raw) = RawMessage::decode(&bytes) else {
            return false;
        };
        if raw.string(1).ok().flatten() != Some(expected)
            || raw.uint(2).ok().flatten() != Some(1000)
        {
            return false;
        }
        let content_matches = raw
            .message(12)
            .ok()
            .flatten()
            .and_then(|content| content.message(1).ok().flatten())
            .is_some_and(|text| {
                text.string(1).ok().flatten() == Some(super::split_fixture::reuse_source_rtml())
            });
        content_matches
            && raw.message(38).ok().flatten().is_some_and(|wire| {
                wire.uint(2).ok().flatten()
                    == Some(if super::split_fixture::reuse_base() == "zero" {
                        0
                    } else {
                        1000
                    })
                    && wire.uint(3).ok().flatten() == Some(1000)
            })
    })
}
pub(in crate::canvas) async fn reload(mut state: Signal<ShellState>, mut reader: Signal<Reader>) {
    if !enabled(&state.peek()) {
        return;
    }
    let Some(read) = checklist::begin_read(&mut state.write()) else {
        return;
    };
    let snapshot = state
        .peek()
        .canvas_checklist
        .structure_fixture
        .as_ref()
        .unwrap()
        .canonical
        .clone()
        .unwrap();
    checklist::recovery::load(state, &read).await;
    let mut shell = state.write();
    checklist::finish_read(&mut shell, &read, Ok(snapshot));
    checklist::reader::update_mapping(&shell, &mut reader.write());
}
pub(super) fn set_snapshot(shell: &mut ShellState, snapshot: Arc<CanvasCollabSnapshot>) {
    shell
        .canvas_checklist
        .structure_fixture
        .as_mut()
        .unwrap()
        .canonical = Some(snapshot);
    structure_fixture::refresh(shell);
}
pub(super) fn changed_snapshot() -> Arc<CanvasCollabSnapshot> {
    let mut result = (*snapshot()).clone();
    let neighbor = result
        .sections
        .iter_mut()
        .find(|s| {
            s.id()
                == if super::split_fixture::is_empty()
                    || super::split_fixture::is_repeat()
                    || super::split_fixture::is_sequence()
                {
                    super::split_fixture::parent_id()
                } else {
                    "neighbor"
                }
        })
        .unwrap();
    *neighbor = Section::from_raw(
        neighbor
            .raw()
            .replace_fields(2, &[F::uint(2, 2000).unwrap()])
            .unwrap(),
    )
    .unwrap();
    Arc::new(result)
}

/// Restoring a receipt cannot silently select a different parent-history cohort.
pub(super) fn matches_parent_mode(checkpoint: &Checkpoint) -> bool {
    if super::split_fixture::is_end() {
        return true;
    }
    let Ok(value) = serde_json::to_value(checkpoint) else {
        return false;
    };
    let Some(sections) = value["sections"].as_array() else {
        return false;
    };
    let expected = snapshot();
    let expected = expected
        .section(super::split_fixture::parent_id())
        .unwrap()
        .raw()
        .message(38)
        .unwrap();
    sections.iter().any(|value| {
        let Ok(bytes) = serde_json::from_value::<Vec<u8>>(value.clone()) else {
            return false;
        };
        let Ok(raw) = RawMessage::decode(&bytes) else {
            return false;
        };
        raw.string(1).ok().flatten() == Some(super::split_fixture::parent_id())
            && raw.message(38).ok() == Some(expected.clone())
    })
}
