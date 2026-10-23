use super::*;

#[test]
fn exact_cost_boundary_and_overflow_refuse_without_large_allocations() {
    assert_eq!(
        charge(0, ATTRIBUTE_COST_LIMIT - 1, 1).unwrap(),
        ATTRIBUTE_COST_LIMIT - 1
    );
    assert_eq!(
        charge(0, ATTRIBUTE_COST_LIMIT, 1).unwrap(),
        ATTRIBUTE_COST_LIMIT
    );
    assert!(charge(0, ATTRIBUTE_COST_LIMIT + 1, 1).is_err());
    assert!(charge(0, usize::MAX, 2).is_err());
    assert!(charge(usize::MAX, 1, 1).is_err());
    assert_eq!(
        charge(ATTRIBUTE_COST_LIMIT - 2, 1, 2).unwrap(),
        ATTRIBUTE_COST_LIMIT
    );
    assert!(charge(ATTRIBUTE_COST_LIMIT - 1, 1, 2).is_err());
}

#[test]
fn serialization_cost_is_checked_once_per_op_and_preserves_raw_values() {
    let attributes = json!({"opaque":{"unknown":[null,false,1,"??"]},"bold":true});
    let size = serde_json::to_vec(attributes.as_object().unwrap())
        .unwrap()
        .len();
    let mut total = 7;
    reserve(&mut total, Some(&attributes), 3).unwrap();
    assert_eq!(total, 7 + size * 3);
    let original = vec![json!({"insert":"a??","attributes":attributes})];
    let document = Document::capture(&original).unwrap();
    assert_eq!(document.len(), 3);
    assert_eq!(document.ops().unwrap(), original);
    let mut exhausted = ATTRIBUTE_COST_LIMIT;
    assert!(reserve(&mut exhausted, Some(&attributes), 1).is_err());
    assert_eq!(exhausted, ATTRIBUTE_COST_LIMIT);
    for value in [None, Some(&Value::Null), Some(&json!({}))] {
        reserve(&mut exhausted, value, LIMIT).unwrap();
    }
}

#[test]
fn oversized_input_refuses_before_per_unit_attribute_expansion() {
    // Only about 1 KiB of source attributes; its per-unit copies would exceed
    // the cap. No expanded document is allocated before this refusal.
    let original =
        vec![json!({"insert":"x".repeat(9000),"attributes":{"unknown":"v".repeat(1024)}})];
    assert!(matches!(
        Document::capture(&original),
        Err(SeedUnavailable::Limit)
    ));
    let over_units = vec![json!({"insert":"x".repeat(LIMIT + 1)})];
    assert!(matches!(
        Document::capture(&over_units),
        Err(SeedUnavailable::Limit)
    ));
    assert!(Document::capture(&[json!({"insert":"x","attributes":false})]).is_err());
}
