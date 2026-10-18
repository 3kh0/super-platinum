use super::*;
use crate::slack::api::profile_sections::custom::rich;

fn captured(tokens: &[Value], labels: &[&str]) -> RichTextContext {
    assert_eq!(tokens.len(), labels.len());
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    for (token, label) in tokens.iter().zip(labels) {
        context.file(token, (*label).into()).unwrap();
    }
    context
}

#[test]
fn whole_files_require_exact_context_even_when_original_or_wire_text_is_present() {
    for kind in ["canvas", "file"] {
        let token = json!({"type":kind,"file_id":"F1","text":"Untrusted title",
            "url":"https://fixture.invalid/very-long-wire-url"});
        let input = blocks(vec![token.clone()]);
        assert!(rich::validate(&input, &[]).is_err());
        assert!(rich::validate(&input, &input).is_err());
        let empty = RichTextContext::new_ordered("T1", "U1").unwrap();
        assert!(rich::validate_with(&input, &input, Some(&empty)).is_err());
        let context = captured(&[token], &["Title"]);
        assert!(rich::validate_with(&input, &[], Some(&context)).is_ok());
        assert_eq!(
            rich::measured_length_with(&input, Some(&context)).unwrap(),
            5
        );
    }
}

#[test]
fn slug_duplicates_measure_each_label_not_file_id_or_mention_merge() {
    let first = json!({"type":"file","file_id":"F123456789","future":{"n":1}});
    let second = json!({"type":"file","file_id":"F123456789","future":{"n":2}});
    let mut styled = second.clone();
    styled["style"] = json!({"bold":true,"italic":true});
    for (right, labels, expected) in [
        (second.clone(), ["One", "One"], 6),
        (second, ["One", "Other"], 8),
        (styled, ["One", "One"], 6),
    ] {
        let tokens = vec![first.clone(), right];
        let context = captured(&tokens, &labels);
        assert_eq!(measure(tokens, &context).unwrap(), expected);
    }
}

#[test]
fn canonical_label_utf16_limit_preserves_raw_url_text_style_and_future_fields() {
    // E1L/ -> pND/ / bFOf -> 3bvJ: title text is counted, not wire URL/id.
    // Explicit escapes keep astral and combining characters unambiguous.
    let tokens = vec![
        json!({"type":"canvas","file_id":"FC","text":"https://fixture.invalid/c",
            "url":"https://fixture.invalid/c","style":{"bold":true},"future":[1,{"v":2}]}),
        json!({"type":"file","file_id":"FF","text":"FF","style":{"code":true}}),
    ];
    let context = captured(&tokens, &["A\u{1f680}", "e\u{301}"]);
    assert_eq!(measure(tokens.clone(), &context).unwrap(), 5);
    for extra in [0, 1] {
        let mut rows = vec![json!({"type":"text","text":"x".repeat(4995+extra)})];
        rows.extend(tokens.clone());
        let input = blocks(rows);
        let original = input.clone();
        assert_eq!(
            rich::measured_length_with(&input, Some(&context)).unwrap(),
            5000 + extra
        );
        assert_eq!(
            rich::validate_with(&input, &[], Some(&context)).is_ok(),
            extra == 0
        );
        assert_eq!(input, original);
    }
}

#[test]
fn exact_occurrences_reject_reorder_omission_extra_and_opaque_or_style_changes() {
    let first = json!({"type":"file","file_id":"F1","unknown":1});
    let second = json!({"type":"file","file_id":"F1","unknown":2});
    let context = captured(&[first.clone(), second.clone()], &["One", "One"]);
    for key in ["file_id", "type", "unknown", "text", "url", "style"] {
        let mut changed = second.clone();
        changed[key] = match key {
            "style" => json!({"bold":true}),
            "type" => json!("canvas"),
            _ => json!("changed"),
        };
        assert!(
            measure(vec![first.clone(), changed], &context).is_err(),
            "{key}"
        );
    }
    for rows in [
        vec![second.clone(), first.clone()],
        vec![first.clone()],
        vec![first.clone(), second.clone(), first],
    ] {
        assert!(measure(rows, &context).is_err());
    }
}

#[test]
fn mixed_context_keeps_mention_id_semantics_and_file_occurrence_order() {
    let user = json!({"type":"user","user_id":"U123"});
    let channel = json!({"type":"channel","channel_id":"C123"});
    let group = json!({"type":"usergroup","usergroup_id":"S123"});
    let file = json!({"type":"file","file_id":"F1"});
    let canvas = json!({"type":"canvas","file_id":"F2"});
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    context.mention(&user, "@User".into()).unwrap();
    context.file(&file, "Long file title".into()).unwrap();
    context.mention(&channel, "#Channel".into()).unwrap();
    context.file(&canvas, "Canvas".into()).unwrap();
    context.mention(&group, "@Group".into()).unwrap();
    let mut rows = vec![user, file, channel, canvas, group];
    assert_eq!(measure(rows.clone(), &context).unwrap(), 33);
    assert!(rich::validate_with(&blocks(rows.clone()), &[], Some(&context)).is_ok());
    rows.swap(1, 3);
    assert!(measure(rows, &context).is_err());
    assert!(!context.owns("T2", "U1"));
}

#[test]
fn active_canvas_variants_and_other_node_types_are_not_admitted() {
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    for token in [
        json!({"type":"list_record","file_id":"F1","record_id":"Rec1"}),
        json!({"type":"skill","skill_ref":"F1"}),
        json!({"type":"canvas","file_id":"F1","section_id":"S1"}),
        json!({"type":"canvas","file_id":"F1","section_id":{}}),
        json!({"type":"canvas","file_id":"F1","is_skill_invocation":true}),
        json!({"type":"canvas","file_id":"F1","is_skill_invocation":"false"}),
        json!({"type":"file","file_id":""}),
        json!({"type":"file","file_id":1}),
        json!({"type":"file","file_id":"F1","style":null}),
    ] {
        assert!(context.file(&token, "Title".into()).is_err(), "{token}");
    }
    assert_eq!(measure(vec![], &context).unwrap(), 0);
    for falsey in [Value::Null, json!(false), json!(0), json!("")] {
        let token = json!({"type":"canvas","file_id":"F1",
            "section_id":falsey,"is_skill_invocation":falsey});
        let context = captured(&[token.clone()], &["Title"]);
        assert_eq!(measure(vec![token], &context).unwrap(), 5);
    }
}

#[test]
fn ignored_file_fields_and_empty_labels_remain_captured_not_reinterpreted() {
    let token = json!({"type":"file","file_id":"F1","section_id":"opaque",
        "record_id":"opaque","view_id":"opaque","is_skill_invocation":true,
        "skill_ref":"opaque","text":"Must not be used as label"});
    let context = captured(&[token.clone()], &[""]);
    assert_eq!(measure(vec![token.clone()], &context).unwrap(), 0);
    assert!(rich::validate_with(&blocks(vec![token.clone()]), &[], Some(&context)).is_ok());
    assert!(measure(vec![], &context).is_err());
    let mut cursor = 0;
    let (label, kind) = context.resolve_next(&token, &mut cursor).unwrap().unwrap();
    assert!(label.is_empty());
    assert!(matches!(kind, Kind::Slug { .. }));
    context.finish_sequence(cursor).unwrap();
}

#[test]
fn whole_file_context_does_not_authorize_wrong_nesting_or_exceed_aggregate_budget() {
    let token = json!({"type":"file","file_id":"F1"});
    let mut context = captured(&[token.clone()], &["Title"]);
    assert!(rich::validate_with(&[token.clone()], &[], Some(&context)).is_err());
    assert!(context.file(&token, "x".repeat(1_048_576)).is_err());
    assert_eq!(measure(vec![token], &context).unwrap(), 5);
}
