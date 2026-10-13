use super::Record;
use serde_json::Value;
use std::cmp::Ordering;

// Captured view-position selection. Comparison below preserves homogeneous
// numeric/lexical order but deliberately uses a total native order for mixed
// types: JavaScript's pair-dependent coercion is not a valid Rust sort contract.
fn position<'a>(record: &'a Record, view: Option<&str>) -> Option<&'a Value> {
    let override_position = view.and_then(|view| {
        record
            .raw
            .get("view_positions")?
            .as_object()?
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(view))
            .map(|(_, value)| value)
    });
    override_position
        .filter(|value| match value {
            Value::String(v) => !v.is_empty(),
            Value::Number(v) => v.as_f64() != Some(0.0),
            _ => false,
        })
        .or_else(|| record.raw.get("position").filter(|v| !v.is_null()))
}
fn scalar(value: Option<&Value>, missing_last: bool) -> (u8, Option<f64>, Option<&str>) {
    match value {
        None | Some(Value::Null) => (if missing_last { 4 } else { 0 }, None, None),
        Some(Value::Number(n)) if n.as_f64().is_some_and(f64::is_finite) => (1, n.as_f64(), None),
        Some(Value::String(s)) => (2, None, Some(s)),
        _ => (3, None, None),
    }
}
fn compare_scalar(a: Option<&Value>, b: Option<&Value>, missing_last: bool) -> Ordering {
    let a = scalar(a, missing_last);
    let b = scalar(b, missing_last);
    a.0.cmp(&b.0).then_with(|| match (a.1, b.1, a.2, b.2) {
        (Some(a), Some(b), _, _) => a.total_cmp(&b),
        (_, _, Some(a), Some(b)) => a.cmp(b),
        _ => Ordering::Equal,
    })
}
pub(super) fn compare(a: &Record, b: &Record, view: Option<&str>) -> Ordering {
    // Missing positions first; malformed positions last. Creation-time ties use
    // number < string < malformed < missing. No coercion across representations.
    compare_scalar(position(a, view), position(b, view), false)
        .then_with(|| compare_scalar(a.raw.get("date_created"), b.raw.get("date_created"), true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn row(position: Value, date: Value) -> Record {
        Record {
            id: "R".into(),
            fields: Default::default(),
            raw: json!({"position":position,"date_created":date}),
        }
    }
    #[test]
    fn heterogeneous_positions_and_creation_ties_obey_total_order_laws() {
        let values = [
            json!("2"),
            json!("10"),
            json!(3),
            Value::Null,
            json!(0),
            json!(-1),
            json!(""),
            json!(false),
            json!({"malformed":true}),
        ];
        for tied_positions in [false, true] {
            let rows = values
                .iter()
                .map(|v| {
                    if tied_positions {
                        row(json!(0), v.clone())
                    } else {
                        row(v.clone(), json!(0))
                    }
                })
                .collect::<Vec<_>>();
            for a in &rows {
                for b in &rows {
                    assert_eq!(compare(a, b, None), compare(b, a, None).reverse());
                    for c in &rows {
                        if compare(a, b, None).is_le() && compare(b, c, None).is_le() {
                            assert!(compare(a, c, None).is_le());
                        }
                    }
                }
            }
            let mut ordered = rows.clone();
            ordered.sort_by(|a, b| compare(a, b, None));
            assert!(
                ordered
                    .windows(2)
                    .all(|r| compare(&r[0], &r[1], None).is_le())
            );
        }
    }
    #[test]
    fn normal_positions_and_case_insensitive_truthy_view_overrides_are_preserved() {
        assert!(
            compare(
                &row(json!("10"), json!(0)),
                &row(json!("2"), json!(0)),
                None
            )
            .is_lt()
        );
        assert!(compare(&row(json!(2), json!(0)), &row(json!(10), json!(0)), None).is_lt());
        let mut a = row(json!(10), json!(1));
        let b = row(json!(2), json!(2));
        a.raw["view_positions"] = json!({"VIEW":1});
        assert!(compare(&a, &b, Some("view")).is_lt());
        a.raw["view_positions"] = json!({"VIEW":0});
        assert!(compare(&a, &b, Some("view")).is_gt());
        a.raw["view_positions"] = json!({"VIEW":""});
        assert!(compare(&a, &b, Some("view")).is_gt());
    }
}
