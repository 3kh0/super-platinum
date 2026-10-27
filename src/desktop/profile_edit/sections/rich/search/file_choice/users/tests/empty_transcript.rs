use super::*;
use crate::profile_edit::sections::rich::title_locale::CanvasTitle;
use std::sync::atomic::{AtomicBool, Ordering};

fn empty_source(blocks: bool) -> Value {
    let mut raw = file(FILE);
    raw["title"] = json!(" \t\n");
    raw["title_blocks"] = if blocks {
        json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":" \t\n","opaque":{"keep":true}}
        ]}]}])
    } else {
        json!([])
    };
    raw
}
fn token() -> Value {
    json!({"type":"canvas","file_id":FILE,"text":"original wire", "url":"https://fixture.invalid/canvas",
        "style":{"bold":true},"opaque":{"keep":[1,2]}})
}
fn setup(blocks: bool) -> ShellState {
    let mut s = shell();
    s.self_account.user_id = "USELF0001".into();
    s.canvas_titles = crate::canvas::quip_titles::State::fresh();
    file_source::fixture_target_source(&mut s, TEAM, vec![empty_source(blocks)]).unwrap();
    s
}
fn capture(s: &ShellState, owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>) -> FileTitles {
    FileTitles::capture_with_context(s, TEAM, vec![token()], owner, Some(CanvasTitle::english()))
        .unwrap()
}

#[test]
fn explicit_transcript_context_uses_exact_wire_without_member_or_policy_dependency() {
    for blocks in [false, true] {
        let s = setup(blocks);
        let raw = token();
        assert!(
            FileTitles::capture(&s, TEAM, vec![raw.clone()], Arc::new(|_| true))
                .unwrap()
                .complete(&s, None, None, Arc::new(|_| true))
                .is_err()
        );
        assert!(FileTarget::capture(&s, TEAM, &raw).is_err());
        let titles = Arc::new(
            capture(&s, Arc::new(|_| true))
                .complete(&s, None, None, Arc::new(|_| true))
                .unwrap(),
        );
        assert!(titles.required().is_empty());
        assert_eq!(titles.label(&s, &raw).unwrap(), "Untitled");
        let proof = titles.snapshot(&s);
        assert_eq!(proof["users"], Value::Null);
        assert_eq!(proof["files"][0]["source"], empty_source(blocks));
        assert_eq!(
            proof["canvasFallback"][FILE]["descriptor"],
            CanvasTitle::english().descriptor()
        );
        assert_eq!(
            proof["canvasFallback"][FILE]["quipTitle"],
            if blocks {
                json!({"fileId":FILE,"current":true,"value":"nullish"})
            } else {
                Value::Null
            }
        );
        let target = FileTarget::capture_with_titles(&s, TEAM, &raw, Some(titles.clone())).unwrap();
        assert_eq!(target.raw(), &raw);
        assert_eq!(target.label(), "Untitled");
        let mut duplicate = raw.clone();
        duplicate["style"] = json!({"italic":true});
        duplicate["opaque"] = json!({"second":2});
        let target =
            FileTarget::capture_with_titles(&s, TEAM, &duplicate, Some(titles.clone())).unwrap();
        assert_eq!(target.raw(), &duplicate);
        assert_eq!(target.label(), "Untitled");
        duplicate["record_id"] = json!("unowned");
        assert!(FileTarget::capture_with_titles(&s, TEAM, &duplicate, Some(titles)).is_err());
    }
}

#[test]
fn retained_quip_is_used_by_display_and_proof_and_never_recaptured_after_replacement() {
    let mut s = setup(true);
    let titles = capture(&s, Arc::new(|_| true))
        .complete(&s, None, None, Arc::new(|_| true))
        .unwrap();
    assert!(titles.current(&s));
    s.canvas_titles = crate::canvas::quip_titles::State::fresh();
    assert!(!titles.current(&s));
    assert!(titles.label(&s, &token()).is_err());
    let proof = titles.snapshot(&s);
    assert_eq!(proof["canvasFallback"][FILE]["current"], false);
    assert_eq!(
        proof["canvasFallback"][FILE]["quipTitle"],
        json!({"fileId":FILE,"current":false,"value":"nullish"})
    );
    assert_eq!(proof["labels"], Value::Null);
    // Only a fresh independently captured receipt may use the successor runtime.
    assert_eq!(
        capture(&s, Arc::new(|_| true))
            .complete(&s, None, None, Arc::new(|_| true))
            .unwrap()
            .label(&s, &token())
            .unwrap(),
        "Untitled"
    );
}

#[test]
fn retired_owner_or_changed_file_refuses_late_completion_and_existing_receipt() {
    for before_complete in [false, true] {
        for file_change in [false, true] {
            let mut s = setup(false);
            let live = Arc::new(AtomicBool::new(true));
            let admitted = live.clone();
            let mut titles = capture(&s, Arc::new(move |_| admitted.load(Ordering::SeqCst)));
            if !before_complete {
                titles = titles.complete(&s, None, None, Arc::new(|_| true)).unwrap();
            }
            if file_change {
                let mut changed = empty_source(false);
                changed["title"] = json!("successor");
                file_source::fixture_target_source(&mut s, TEAM, vec![changed]).unwrap();
            } else {
                live.store(false, Ordering::SeqCst);
            }
            assert!(!titles.current(&s));
            assert!(titles.label(&s, &token()).is_err());
            if before_complete {
                assert!(titles.complete(&s, None, None, Arc::new(|_| true)).is_err());
            }
        }
    }
}

#[test]
fn transcript_context_does_not_admit_unsupported_or_missing_title_facts() {
    let mut s = setup(true);
    s.canvas_titles = Default::default();
    assert!(
        capture(&s, Arc::new(|_| true))
            .complete(&s, None, None, Arc::new(|_| true))
            .is_err()
    );
    s.canvas_titles = crate::canvas::quip_titles::State::fresh();
    let mut bad = empty_source(true);
    bad["title_blocks"][0]["elements"][0]["elements"][0] = json!({"type":"future"});
    file_source::fixture_target_source(&mut s, TEAM, vec![bad]).unwrap();
    assert!(
        FileTitles::capture_with_context(
            &s,
            TEAM,
            vec![token()],
            Arc::new(|_| true),
            Some(CanvasTitle::english())
        )
        .is_err()
    );
}

#[test]
fn original_transcript_row_survives_unrelated_singleton_but_rejects_same_id_replacement() {
    use super_platinum_core::slack::api::custom::texty::file_title_source::OrderedFileInfo;
    for blocks in [false, true] {
        let mut s = setup(blocks);
        let whole_collection = file_source::Snapshot::capture(&s, TEAM).unwrap();
        let titles = Arc::new(
            capture(&s, Arc::new(|_| true))
                .complete(&s, None, None, Arc::new(|_| true))
                .unwrap(),
        );
        let original = titles.snapshot(&s);
        let mut other = empty_source(false);
        other["id"] = json!("FUNRELATED01");
        other["title"] = json!("Unrelated accepted title");
        let response =
            OrderedFileInfo::decode(&serde_json::to_vec(&json!({"ok":true,"file":other})).unwrap())
                .unwrap();
        file_source::FileInfoRead::capture(&mut s, TEAM, "FUNRELATED01")
            .unwrap()
            .publish(&mut s, &response)
            .unwrap();
        assert!(
            !whole_collection.current(&s),
            "query collection remains conservatively retired"
        );
        assert!(titles.current(&s));
        assert_eq!(titles.label(&s, &token()).unwrap(), "Untitled");
        assert_eq!(
            titles.snapshot(&s),
            original,
            "no source/Quip recapture or receipt rewriting"
        );
        assert_eq!(
            FileTarget::capture_with_titles(&s, TEAM, &token(), Some(titles.clone()))
                .unwrap()
                .label(),
            "Untitled"
        );
        let mut replacement = empty_source(blocks);
        replacement["updated"] = json!(999);
        let response = OrderedFileInfo::decode(
            &serde_json::to_vec(&json!({"ok":true,"file":replacement})).unwrap(),
        )
        .unwrap();
        file_source::FileInfoRead::capture(&mut s, TEAM, FILE)
            .unwrap()
            .publish(&mut s, &response)
            .unwrap();
        assert!(!titles.current(&s));
        assert!(titles.label(&s, &token()).is_err());
        assert!(FileTarget::capture_with_titles(&s, TEAM, &token(), Some(titles)).is_err());
    }
}

#[test]
fn original_transcript_row_still_rejects_socket_account_and_file_invalidation() {
    for mutation in 0..3 {
        let mut s = setup(false);
        let titles = capture(&s, Arc::new(|_| true))
            .complete(&s, None, None, Arc::new(|_| true))
            .unwrap();
        match mutation {
            0 => s.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1,
            1 => s.core.account_epoch += 1,
            _ => crate::file_events::invalidate(&mut s, TEAM, FILE),
        }
        assert!(!titles.current(&s));
        assert!(titles.label(&s, &token()).is_err());
    }
}

#[test]
fn nonempty_list_transcript_receipt_remains_current_after_canvas_publication() {
    use super_platinum_core::slack::api::custom::texty::file_title_source::OrderedFileInfo;
    let mut s = setup(false);
    let mut list = empty_source(false);
    list["mode"] = json!("list");
    list["filetype"] = json!("list");
    list["title"] = json!("Original List title");
    file_source::fixture_target_source(&mut s, TEAM, vec![list]).unwrap();
    let mut raw = token();
    raw["type"] = json!("file");
    let titles = FileTitles::capture_with_context(
        &s,
        TEAM,
        vec![raw.clone()],
        Arc::new(|_| true),
        Some(CanvasTitle::english()),
    )
    .unwrap()
    .complete(&s, None, None, Arc::new(|_| true))
    .unwrap();
    let before = titles.snapshot(&s);
    let mut canvas = empty_source(false);
    canvas["id"] = json!("FOTHER01");
    let response =
        OrderedFileInfo::decode(&serde_json::to_vec(&json!({"ok":true,"file":canvas})).unwrap())
            .unwrap();
    file_source::FileInfoRead::capture(&mut s, TEAM, "FOTHER01")
        .unwrap()
        .publish(&mut s, &response)
        .unwrap();
    assert!(titles.current(&s));
    assert_eq!(titles.label(&s, &raw).unwrap(), "Original List title");
    assert_eq!(titles.snapshot(&s), before);
    assert_eq!(
        before["canvasFallback"],
        json!({}),
        "List receives no Canvas fallback grant"
    );
}
