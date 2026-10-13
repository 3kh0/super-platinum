use super::*;
use serde_json::json;

#[test]
fn captured_browser_intl_oracle_all_supported_locales() {
    // Generated through root's live browser Intl using captured H0Nw transforms.
    // Exact cases use BigInt as formatter input so the oracle also preserves u64.
    let oracle: Value = serde_json::from_str(include_str!("intl-oracle.json")).unwrap();
    for case in oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(oracle["exact"].as_array().unwrap())
    {
        let input = case["input"].as_str().unwrap();
        let value: Number = input.parse().unwrap();
        let options = json!({"precision":case["precision"]});
        assert_eq!(
            format(&value, Some(&options), case["locale"].as_str()),
            case["expected"].as_str().unwrap(),
            "{case}"
        );
    }
}

#[test]
fn defaults_unknown_schema_and_exact_value_are_preserved() {
    let value: Number = "12.5".parse().unwrap();
    assert_eq!(format(&value, None, None), "13");
    assert_eq!(
        format(
            &value,
            Some(&json!({"precision":null})),
            Some("not-a-locale")
        ),
        "13"
    );
    for precision in [json!(-1), json!(6), json!(100000), json!("2"), json!(1.5)] {
        assert_eq!(
            format(&value, Some(&json!({"precision":precision})), Some("de-DE")),
            "12.5"
        );
    }
    assert_eq!(
        format(
            &value,
            Some(&json!({"format":"currency","currency":"JPY","precision":2})),
            None
        ),
        "12.50"
    );
    assert_eq!(value.to_string(), "12.5");
    assert_eq!(
        format(&Number::from(u64::MAX), None, None),
        "18,446,744,073,709,551,615"
    );
    assert_eq!(
        format(&Number::from(i64::MIN), None, None),
        "-9,223,372,036,854,775,808"
    );
}

#[test]
fn decimal_carry_and_extreme_exponents_are_bounded() {
    let options = json!({"precision":2});
    for (raw, expected) in [
        ("9.999", "10.00"),
        ("-9.995", "-10.00"),
        ("1e-300", "0.00"),
        ("-1e-300", "-0.00"),
    ] {
        assert_eq!(
            format(&raw.parse().unwrap(), Some(&options), None),
            expected
        );
    }
    let huge: Number = "1e300".parse().unwrap();
    let output = format(&huge, None, None);
    assert_eq!(output.replace(',', ""), format!("1{}", "0".repeat(300)));
    assert!(output.len() < 512);
}
