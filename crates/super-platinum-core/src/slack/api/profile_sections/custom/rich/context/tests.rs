use super::*;
use serde_json::json;

mod files;
mod sections;

fn blocks(tokens: Vec<Value>) -> Vec<Value> {
    vec![json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":tokens}]})]
}
fn measure(tokens: Vec<Value>, context: &RichTextContext) -> Result<usize, Error> {
    super::super::measured_length_with(&blocks(tokens), Some(context))
}
#[test]
fn duplicate_channel_labels_are_attributes_not_measured_text() {
    let token = json!({"type":"channel","channel_id":"C123","unknown":{"preserve":true}});
    let mut same = RichTextContext::new_ordered("T1", "U1").unwrap();
    same.mention(&token, "#a very long canonical display label".into())
        .unwrap();
    same.mention(&token, "#a very long canonical display label".into())
        .unwrap();
    assert_eq!(
        measure(vec![token.clone(), token.clone()], &same).unwrap(),
        4
    );
    let mut different = RichTextContext::new_ordered("T1", "U1").unwrap();
    different.mention(&token, "#Cafe\u{301}".into()).unwrap();
    different.mention(&token, "#Caf\u{e9}".into()).unwrap();
    assert_eq!(
        measure(vec![token.clone(), token.clone()], &different).unwrap(),
        8
    );
    // Ordinary callers retain their existing unresolved-token measurement.
    assert_eq!(
        super::super::measured_length(&blocks(vec![token.clone(), token])).unwrap(),
        8
    );
}
#[test]
fn utf16_boundary_uses_channel_id_and_keeps_wire_tokens_unchanged() {
    let token = json!({"type":"channel","channel_id":"C123","future":{"v":[1,2]}});
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    context
        .mention(&token, format!("#{}", "long".repeat(2000)))
        .unwrap();
    context
        .mention(&token, format!("#{}", "long".repeat(2000)))
        .unwrap();
    for extra in [0, 1] {
        let input = blocks(vec![
            json!({"type":"text","text":format!("{}{}", "🚀".repeat(2498), "x".repeat(extra))}),
            token.clone(),
            token.clone(),
        ]);
        let unchanged = input.clone();
        assert_eq!(
            super::super::measured_length_with(&input, Some(&context)).unwrap(),
            5000 + extra
        );
        assert_eq!(
            super::super::validate_with(&input, &[], Some(&context)).is_ok(),
            extra == 0
        );
        assert_eq!(input, unchanged);
    }
}
#[test]
fn ordered_custody_rejects_missing_extra_reordered_and_modified_occurrences() {
    let a = json!({"type":"channel","channel_id":"C1","unknown":1});
    let b = json!({"type":"channel","channel_id":"C1","unknown":2});
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    context.mention(&a, "#A".into()).unwrap();
    context.mention(&b, "#B".into()).unwrap();
    assert_eq!(measure(vec![a.clone(), b.clone()], &context).unwrap(), 4);
    for rows in [
        vec![a.clone()],
        vec![a.clone(), b.clone(), a.clone()],
        vec![b.clone(), a.clone()],
        vec![
            a.clone(),
            json!({"type":"channel","channel_id":"C1","unknown":3}),
        ],
        vec![
            a.clone(),
            json!({"type":"channel","channel_id":"C1","unknown":2,"style":{"bold":true}}),
        ],
    ] {
        assert!(measure(rows, &context).is_err());
    }
}
#[test]
fn captured_style_separates_ops_and_other_existing_types_keep_their_semantics() {
    let a = json!({"type":"channel","channel_id":"C123"});
    let b = json!({"type":"channel","channel_id":"C123","style":{"bold":true}});
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    context.mention(&a, "#label".into()).unwrap();
    context.mention(&b, "#label".into()).unwrap();
    assert_eq!(measure(vec![a, b], &context).unwrap(), 8);
    let empty = RichTextContext::new_ordered("T1", "U1").unwrap();
    let existing = blocks(vec![
        json!({"type":"link","url":"https://example.invalid","text":"Link"}),
    ]);
    assert_eq!(
        super::super::measured_length_with(&existing, Some(&empty)).unwrap(),
        super::super::measured_length(&existing).unwrap()
    );
}
#[test]
fn ordinary_scope_ownership_and_aggregate_limits_are_explicit() {
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    assert!(context.owns("T1", "U1"));
    assert!(!context.owns("T2", "U1"));
    assert!(!context.owns("T1", "U2"));
    for token in [
        json!({"type":"usergroup","usergroup_id":""}),
        json!({"type":"user","user_id":""}),
        json!({"type":"channel","channel_id":"C1","tab_id":"tab"}),
        json!({"type":"channel","channel_id":""}),
    ] {
        assert!(context.mention(&token, "label".into()).is_err());
    }
    let token = json!({"type":"channel","channel_id":"C1"});
    context.mention(&token, "x".repeat(600_000)).unwrap();
    assert!(context.mention(&token, "x".repeat(600_000)).is_err());
    assert_eq!(measure(vec![token], &context).unwrap(), 2);
}

#[test]
fn duplicate_user_mentions_count_ids_once_but_distinct_labels_or_styles_separate() {
    let first = json!({"type":"user","user_id":"U123","unknown":{"retain":1}});
    let second = json!({"type":"user","user_id":"U123","unknown":{"retain":2}});
    let mut same = RichTextContext::new_ordered("T1", "U1").unwrap();
    for token in [&first, &second] {
        same.mention(token, "@a very long label".into()).unwrap();
    }
    assert_eq!(
        measure(vec![first.clone(), second.clone()], &same).unwrap(),
        4
    );
    assert_eq!(
        super::super::measured_length(&blocks(vec![first.clone(), second.clone()])).unwrap(),
        8
    );
    let mut different = RichTextContext::new_ordered("T1", "U1").unwrap();
    different.mention(&first, "@e\u{301}".into()).unwrap();
    different.mention(&second, "@\u{e9}".into()).unwrap();
    assert_eq!(
        measure(vec![first.clone(), second.clone()], &different).unwrap(),
        8
    );
    let mut styled = second;
    styled["style"] = json!({"bold":true});
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    context.mention(&first, "@same".into()).unwrap();
    context.mention(&styled, "@same".into()).unwrap();
    assert_eq!(measure(vec![first, styled], &context).unwrap(), 8);
}

#[test]
fn user_utf16_limit_uses_id_and_never_rewrites_unknown_tokens() {
    let first = json!({"type":"user","user_id":"U123","future":{"a":1}});
    let second = json!({"type":"user","user_id":"U123","future":{"a":2}});
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    for token in [&first, &second] {
        context.mention(token, "@".into()).unwrap();
    }
    for extra in [0, 1] {
        let input = blocks(vec![
            json!({"type":"text","text":format!("{}{}","\u{1f680}".repeat(2498),"x".repeat(extra))}),
            first.clone(),
            second.clone(),
        ]);
        let before = input.clone();
        assert_eq!(
            super::super::measured_length_with(&input, Some(&context)).unwrap(),
            5000 + extra
        );
        assert_eq!(
            super::super::validate_with(&input, &[], Some(&context)).is_ok(),
            extra == 0
        );
        assert_eq!(input, before);
    }
}

#[test]
fn mixed_user_channel_custody_requires_original_order_and_complete_occurrences() {
    let user = json!({"type":"user","user_id":"U1","future":1});
    let channel = json!({"type":"channel","channel_id":"C1"});
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    context.mention(&user, "@  preserved  ".into()).unwrap();
    context.mention(&channel, "#label".into()).unwrap();
    assert_eq!(
        measure(vec![user.clone(), channel.clone()], &context).unwrap(),
        4
    );
    let mut changed = user.clone();
    changed["future"] = json!(2);
    let mut styled = user.clone();
    styled["style"] = json!({"italic":true});
    for tokens in [
        vec![channel.clone(), user.clone()],
        vec![user.clone()],
        vec![user.clone(), channel.clone(), user.clone()],
        vec![changed, channel.clone()],
        vec![styled, channel.clone()],
    ] {
        assert!(measure(tokens, &context).is_err());
    }
    let empty = RichTextContext::new_ordered("T1", "U1").unwrap();
    assert!(measure(vec![user.clone()], &empty).is_err());
    // An old captured context cannot silently use the no-context fallback.
    assert_eq!(
        super::super::measured_length(&blocks(vec![user])).unwrap(),
        2
    );
    assert!(!context.owns("T2", "U1"));
    assert!(!context.owns("T1", "U2"));
}

#[test]
fn user_label_bytes_are_preserved_and_irrelevant_metadata_is_not_interpreted() {
    let user = json!({"type":"user","user_id":"U1","tab_id":{"future":true}});
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    context.mention(&user, "@  e\u{301}  ".into()).unwrap();
    let mut cursor = 0;
    let (label, kind) = context.resolve_next(&user, &mut cursor).unwrap().unwrap();
    assert_eq!(label, "@  e\u{301}  ");
    assert!(matches!(kind,Kind::Mention {id,label} if id=="U1" && label=="@  e\u{301}  "));
    context.finish_sequence(cursor).unwrap();
    assert!(context.mention(&user, String::new()).is_err());
}

#[test]
fn group_duplicates_merge_only_when_label_and_style_match() {
    let a = json!({"type":"usergroup","usergroup_id":"S123","future":1});
    let b = json!({"type":"usergroup","usergroup_id":"S123","future":2});
    for (label, style, expected) in [
        ("@@crew", None, 4),
        ("@other", None, 8),
        ("@@crew", Some(json!({"italic":true})), 8),
    ] {
        let mut second = b.clone();
        if let Some(style) = style {
            second["style"] = style;
        }
        let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
        context.mention(&a, "@@crew".into()).unwrap();
        context.mention(&second, label.into()).unwrap();
        assert_eq!(
            measure(vec![a.clone(), second], &context).unwrap(),
            expected
        );
    }
    assert_eq!(
        super::super::measured_length(&blocks(vec![a, b])).unwrap(),
        8
    );
}

#[test]
fn empty_group_labels_have_zero_length_but_still_require_exact_custody() {
    let group =
        json!({"type":"usergroup","usergroup_id":"S123","tab_id":{"opaque":true},"future":[1,2]});
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    context.mention(&group, String::new()).unwrap();
    assert_eq!(measure(vec![group.clone()], &context).unwrap(), 0);
    assert_eq!(
        measure(
            vec![json!({"type":"text","text":"A\u{1f680}"}), group.clone()],
            &context
        )
        .unwrap(),
        3
    );
    assert!(measure(vec![], &context).is_err());
    let mut modified = group.clone();
    modified["tab_id"] = json!(null);
    assert!(measure(vec![modified], &context).is_err());
    let mut cursor = 0;
    let (label, kind) = context.resolve_next(&group, &mut cursor).unwrap().unwrap();
    assert!(label.is_empty());
    assert!(matches!(kind,Kind::Mention{id,label} if id=="S123"&&label.is_empty()));
    context.finish_sequence(cursor).unwrap();
    for token in [
        json!({"type":"user","user_id":"U1"}),
        json!({"type":"channel","channel_id":"C1"}),
    ] {
        assert!(context.mention(&token, String::new()).is_err());
    }
}

#[test]
fn mixed_group_user_channel_context_refuses_reorder_omission_and_token_changes() {
    let user = json!({"type":"user","user_id":"U1"});
    let channel = json!({"type":"channel","channel_id":"C1"});
    let group = json!({"type":"usergroup","usergroup_id":"S1","tab_id":"opaque","future":1});
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    for (token, label) in [
        (&user, "@user"),
        (&group, "@ e\u{301} "),
        (&channel, "#channel"),
    ] {
        context.mention(token, label.into()).unwrap();
    }
    assert_eq!(
        measure(vec![user.clone(), group.clone(), channel.clone()], &context).unwrap(),
        6
    );
    let mut changed = group.clone();
    changed["future"] = json!(2);
    let mut styled = group.clone();
    styled["style"] = json!({"bold":true});
    for tokens in [
        vec![group.clone(), user.clone(), channel.clone()],
        vec![user.clone(), channel.clone()],
        vec![user.clone(), group.clone(), group.clone(), channel.clone()],
        vec![user.clone(), changed, channel.clone()],
        vec![user, styled, channel],
    ] {
        assert!(measure(tokens, &context).is_err());
    }
    let empty = RichTextContext::new_ordered("T1", "U1").unwrap();
    assert!(measure(vec![group], &empty).is_err());
}
