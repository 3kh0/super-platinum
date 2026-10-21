use super::*;
use serde_json::json;
fn shell() -> ShellState {
    ShellState::fixture(crate::media::MediaRegistry::default())
}
#[test]
fn exact_url_is_retained_only_for_safe_current_team_links() {
    let shell = shell();
    let raw = json!({"type":"file","file_id":"F1","url":"https://example.test/exact?x=1&y=2"});
    assert_eq!(
        destination(&shell, "T1", &raw).as_deref(),
        raw["url"].as_str()
    );
    assert!(destination(&shell, "TOTHER", &raw).is_none());
    for url in [
        "javascript:alert(1)",
        "data:text/html,test",
        "file:///secret",
        "//example.test",
        " https://example.test",
        "https://example.test\n",
        "https://user:pass@example.test",
        "slack://open",
    ] {
        assert!(
            destination(&shell, "T1", &json!({"url":url})).is_none(),
            "unsafe URL admitted"
        );
    }
    assert!(
        destination(
            &shell,
            "T1",
            &json!({"text":"https://example.test","file_id":"F1"})
        )
        .is_none()
    );
}
#[test]
fn missing_or_foreign_source_keeps_visible_wire_fallback_not_raw_title() {
    let mut shell = shell();
    let raw =
        json!({"type":"canvas","file_id":"F1","text":"Original fallback","title":"Forged title"});
    assert_eq!(
        label(&shell, "T1", &raw, "Original fallback"),
        "Original fallback"
    );
    shell.core.active_team = Some("T2".into());
    assert_eq!(
        label(&shell, "T1", &raw, "Original fallback"),
        "Original fallback"
    );
    assert!(destination(&shell, "T1", &json!({"url":"https://example.test"})).is_none());
}
#[test]
fn style_uses_existing_classes_and_requires_actual_boolean_flags() {
    assert_eq!(
        style_class(&json!({"style":{"bold":true,"italic":true,"strike":true,"code":true}})),
        "rich-file-mention bold italic strike inline-code"
    );
    assert_eq!(
        style_class(&json!({"style":{"bold":"true","italic":1}})),
        "rich-file-mention"
    );
}

#[test]
fn list_occurrences_keep_depth_first_identity_for_equal_raw_duplicates() {
    let file = || RichNode::FileMention {
        team: "T1".into(),
        raw: json!({"type":"file","file_id":"F1","style":{"bold":true}}),
        fallback: "F1".into(),
    };
    let nodes = vec![
        RichNode::Paragraph(vec![RichNode::Text("before".into()), file()]),
        RichNode::Section {
            text: vec![file()],
            fields: vec![vec![file()]],
            accessory: Some(Box::new(RichNode::List {
                ordered: false,
                indent: 0,
                offset: 0,
                items: vec![vec![file()]],
            })),
        },
        RichNode::Quote(vec![RichNode::Context(vec![RichNode::Header(vec![
            RichNode::Actions(vec![file()]),
        ])])]),
    ];
    let indexed = occurrences(&nodes);
    assert_eq!(indexed.len(), 5);
    let RichNode::Paragraph(first) = &nodes[0] else {
        panic!("paragraph")
    };
    assert_eq!(
        indexed.get(&(&first[1] as *const RichNode as usize)),
        Some(&0)
    );
    let RichNode::Section {
        text,
        fields,
        accessory,
    } = &nodes[1]
    else {
        panic!("section")
    };
    assert_eq!(
        indexed.get(&(&text[0] as *const RichNode as usize)),
        Some(&1)
    );
    assert_eq!(
        indexed.get(&(&fields[0][0] as *const RichNode as usize)),
        Some(&2)
    );
    let RichNode::List { items, .. } = accessory.as_deref().unwrap() else {
        panic!("list")
    };
    assert_eq!(
        indexed.get(&(&items[0][0] as *const RichNode as usize)),
        Some(&3)
    );
    // Equal bytes from another render node cannot borrow an original ordinal.
    assert!(!indexed.contains_key(&(&file() as *const RichNode as usize)));
    let foreign_attachment = file();
    let source = ListSource {
        source: None,
        ordinals: indexed,
    };
    assert!(
        source
            .token(
                &shell(),
                &foreign_attachment,
                "T1",
                &json!({"type":"file","file_id":"F1"})
            )
            .is_none()
    );
}

#[test]
fn failed_list_capture_cannot_become_ownerless_url_authority() {
    let shell = shell();
    let raw = json!({"type":"file","file_id":"F1","url":"https://example.test/exact"});
    let nodes = vec![RichNode::FileMention {
        team: "T1".into(),
        raw: raw.clone(),
        fallback: "F1".into(),
    }];
    let source = ListSource {
        source: None,
        ordinals: occurrences(&nodes),
    };
    assert!(destination(&shell, "T1", &raw).is_some());
    assert!(source.token(&shell, &nodes[0], "T1", &raw).is_none());
    assert!(source.demand(&shell, &nodes[0], "T1", &raw).is_none());
}

#[test]
fn metadata_ready_cannot_enable_an_unowned_or_unsupported_occurrence() {
    let view = inline_state(
        false,
        Some(file_acquisition::Presentation {
            state: "ready",
            notice: None,
            retryable: false,
        }),
    );
    assert_eq!(view.state, "unavailable");
    assert_eq!(view.notice.as_deref(), Some("File unavailable"));
    assert!(!view.retryable);
    let absent = inline_state(false, None);
    assert_eq!(absent.state, "unavailable");
    assert!(!absent.retryable);
}

#[test]
fn pending_acquisition_cannot_offer_duplicate_retry_but_failed_receipt_can() {
    let loading = inline_state(
        false,
        Some(file_acquisition::Presentation {
            state: "loading",
            notice: None,
            retryable: true,
        }),
    );
    assert_eq!(loading.state, "loading");
    assert_eq!(loading.notice.as_deref(), Some("Loading file…"));
    assert!(!loading.retryable);
    let failed = inline_state(
        false,
        Some(file_acquisition::Presentation {
            state: "unavailable",
            notice: Some("File details could not load. Try again.".into()),
            retryable: true,
        }),
    );
    assert_eq!(failed.state, "unavailable");
    assert_eq!(
        failed.notice.as_deref(),
        Some("File details could not load. Try again.")
    );
    assert!(failed.retryable);
}

#[test]
fn current_original_target_does_not_paint_obsolete_acquisition_feedback() {
    let ready = inline_state(
        true,
        Some(file_acquisition::Presentation {
            state: "unavailable",
            notice: Some("Previous request failed".into()),
            retryable: true,
        }),
    );
    assert_eq!(ready.state, "ready");
    assert!(ready.notice.is_none());
    assert!(!ready.retryable);
}
