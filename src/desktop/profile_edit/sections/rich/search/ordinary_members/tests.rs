use super::*;
use crate::media::MediaRegistry;
use serde_json::json;

#[test]
fn private_mixed_choices_preserve_kind_and_same_label_distinct_group_ids() {
    let labels = BTreeMap::from([
        ("ULISTMEM1".into(), "@alex-team".into()),
        ("SLISTGRP1".into(), "@alex-team".into()),
        ("SLISTGRP2".into(), "@alex-team".into()),
    ]);
    let member = choice(false, "ULISTMEM1", &labels).unwrap();
    let first = choice(true, "SLISTGRP1", &labels).unwrap();
    let second = choice(true, "SLISTGRP2", &labels).unwrap();
    assert_eq!(member.0, json!({"type":"user","user_id":"ULISTMEM1"}));
    assert_eq!(
        first.0,
        json!({"type":"usergroup","usergroup_id":"SLISTGRP1"})
    );
    assert_eq!(
        second.0,
        json!({"type":"usergroup","usergroup_id":"SLISTGRP2"})
    );
    assert_eq!(member.1, first.1);
    assert_eq!(first.1, second.1);
    assert_ne!(first.0, second.0);
    assert!(choice(true, "SUNOBSERVED", &labels).is_err());
}

#[test]
fn private_group_choice_uses_renderer_nfc_label_without_trimming_or_reprefixing() {
    // The existing group renderer owns NFC('@' + effective.handle), covered by
    // its actual-JS tests. This boundary must preserve that receipt verbatim.
    for label in ["@@Caf\u{e9}", "@  Caf\u{e9}  ", "@", "@@@team"] {
        let labels = BTreeMap::from([("SLISTGRP1".into(), label.into())]);
        let (_, selected) = choice(true, "SLISTGRP1", &labels).unwrap();
        assert_eq!(selected, label);
    }
}

#[test]
fn retained_mixed_choice_is_unavailable_after_original_owner_retires() {
    let mut shell = ShellState::login(MediaRegistry::default());
    shell.core.active_account = Some("original".into());
    let values = vec![
        (json!({"type":"user","user_id":"ULISTMEM1"}), "Alex".into()),
        (
            json!({"type":"usergroup","usergroup_id":"SLISTGRP1"}),
            "@alex-team".into(),
        ),
    ];
    let batch = MemberBatch {
        values: values.clone(),
        settled: true,
        diagnostic: Arc::new(|_| Value::Null),
        canvas_fallback: Arc::new(|_| json!({})),
        witness: Arc::new(|s| s.core.active_account.as_deref() == Some("original")),
    };
    assert_eq!(batch.choices(&shell), Some(values));
    shell.core.active_account = Some("replacement".into());
    assert!(!batch.current(&shell));
    assert!(batch.choices(&shell).is_none());
}
