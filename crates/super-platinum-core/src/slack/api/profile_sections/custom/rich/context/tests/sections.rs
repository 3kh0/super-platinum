use super::*;
#[test]
fn section_units_preserve_exact_surrogate_identity_and_raw_occurrence_custody() {
    let raw = json!({"type":"canvas","file_id":"F1","section_id":"temp:C:0123456789abcdef01234567","url":"https://fixture.invalid/section","style":{"bold":true},"opaque":{"keep":[1,2]}});
    let mut a = RichTextContext::new_ordered("T1", "U1").unwrap();
    let mut b = a.clone();
    a.canvas_section(&raw, &[0xd800, 0x2026]).unwrap();
    b.canvas_section(&raw, &[0xd801, 0x2026]).unwrap();
    assert_eq!(a.sequence[0].1, b.sequence[0].1); // Internal count copy only.
    assert!(a.sequence[0].2 != b.sequence[0].2); // Authoritative units not collapsed.
    assert_eq!(measure(vec![raw.clone()], &a).unwrap(), 2);
    for key in ["file_id", "section_id", "url", "opaque", "style"] {
        let mut changed = raw.clone();
        changed[key] = json!("changed");
        assert!(measure(vec![changed], &a).is_err(), "{key}");
    }
    let mut legacy = RichTextContext::new_ordered("T1", "U1").unwrap();
    assert!(legacy.file(&raw, "Name".into()).is_err());
}
#[test]
fn section_duplicate_units_measure_every_occurrence_at_5000_boundary() {
    let one = json!({"type":"canvas","file_id":"F1","section_id":"s1","opaque":1});
    let two = json!({"type":"canvas","file_id":"F1","section_id":"s2","opaque":2});
    let mut context = RichTextContext::new_ordered("T1", "U1").unwrap();
    context
        .canvas_section(&one, &[0x61, 0xd800, 0x2026])
        .unwrap();
    context
        .canvas_section(&two, &[0x61, 0xd801, 0x2026])
        .unwrap();
    assert_eq!(
        measure(vec![one.clone(), two.clone()], &context).unwrap(),
        6
    );
    assert!(measure(vec![two.clone(), one.clone()], &context).is_err());
    for extra in [0, 1] {
        let input = blocks(vec![
            json!({"type":"text","text":"x".repeat(4994+extra)}),
            one.clone(),
            two.clone(),
        ]);
        let before = input.clone();
        assert_eq!(
            super::super::super::measured_length_with(&input, Some(&context)).unwrap(),
            5000 + extra
        );
        assert_eq!(
            super::super::super::validate_with(&input, &[], Some(&context)).is_ok(),
            extra == 0
        );
        assert_eq!(input, before);
    }
    for section in [Value::Null, json!(false), json!(""), json!({})] {
        let mut raw = one.clone();
        raw["section_id"] = section;
        assert!(context.canvas_section(&raw, &[65]).is_err());
    }
    let mut skill = one;
    skill["is_skill_invocation"] = json!(true);
    assert!(context.canvas_section(&skill, &[65]).is_err());
}
