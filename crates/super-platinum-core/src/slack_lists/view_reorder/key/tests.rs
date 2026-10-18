use super::*;

#[test]
fn exact_captured_integer_and_fractional_vectors() {
    for (left, right, expected) in [
        (None, None, "5000000000"),
        (None, Some("5000000000"), "4999999999"),
        (Some("5000000000"), None, "5000000001"),
        (Some("1000000000"), Some("9000000000"), "5000000000"),
        (Some("5000000000"), Some("5000000001"), "5000000000.V"),
        (Some("5000000000.V"), Some("5000000001"), "5000000000.l"),
        (Some("5000000000"), Some("5000000000.V"), "5000000000.G"),
        (Some("5000000000.V"), Some("5000000000.W"), "5000000000.VV"),
        (
            Some("5000000000.V"),
            Some("5000000000.V1"),
            "5000000000.V0V",
        ),
        (
            Some("5000000000"),
            Some("5000000000.001"),
            "5000000000.000V",
        ),
        (Some("5000000000.z"), Some("5000000001"), "5000000000.zV"),
        (None, Some("5000000000.V"), "5000000000"),
        (None, Some("0000000000.V"), "0000000000"),
        // The source compares an unpadded integer string, not its numeric value.
        (None, Some("0000000001.V"), "0000000000"),
        (Some("0000000001.V"), None, "0000000002"),
    ] {
        assert_eq!(
            between(left, right).unwrap(),
            expected,
            "{left:?} {right:?}"
        );
    }
}
#[test]
fn integer_limits_and_ordering_fail_closed() {
    for (left, right) in [
        (None, Some("0000000000")),
        (Some("9999999999"), None),
        (Some("9999999999.V"), None),
        (Some("5000000000"), Some("5000000000")),
        (Some("5000000001"), Some("5000000000")),
    ] {
        assert!(between(left, right).is_err());
    }
}
#[test]
fn malformed_unicode_and_coercible_legacy_keys_are_not_normalized() {
    for value in [
        "",
        "1",
        "000000000",
        "00000000000",
        "5000000000.",
        "5000000000.0",
        "5000000000.V0",
        " 500000000",
        "-000000001",
        "5000000000.+",
        "5000000000.é",
        "５000000000",
        "5000000000.😀",
        "1e00000000",
        "5000000000.\0",
        "5000000000.1.1",
    ] {
        assert!(!valid(value), "{value:?}");
        assert!(between(Some(value), None).is_err());
        assert!(between(None, Some(value)).is_err());
    }
}
#[test]
fn dense_keys_are_bounded_without_truncation() {
    let at_bound = format!("5000000000.{}", "z".repeat(MAX_KEY_BYTES - 11));
    assert!(valid(&at_bound));
    assert!(between(Some(&at_bound), Some("5000000001")).is_err());
    assert!(!valid(&(at_bound + "V")));
}
#[test]
fn repeated_midpoints_remain_strictly_between_original_bounds() {
    let mut left = "5000000000".to_owned();
    let right = "5000000001";
    for _ in 0..100 {
        let next = between(Some(&left), Some(right)).unwrap();
        assert!(left < next && next.as_str() < right && valid(&next));
        left = next;
    }
}

#[test]
fn retained_official_helper_oracle_vectors() {
    for (left, right, expected) in [
        (None, None, "5000000000"),
        (None, Some("0000000001"), "0000000000"),
        (Some("0000000001"), None, "0000000002"),
        (Some("0000000001"), Some("0000000009"), "0000000005"),
        (Some("0000000001"), Some("0000000002"), "0000000001.V"),
        (Some("0000000001.V"), Some("0000000002"), "0000000001.l"),
        (Some("0000000001.V"), Some("0000000001.W"), "0000000001.VV"),
        (Some("0000000001.z"), Some("0000000002"), "0000000001.zV"),
        (None, Some("0000000001.V"), "0000000000"),
        (Some("9999999998"), None, "9999999999"),
    ] {
        assert_eq!(between(left, right).unwrap(), expected);
    }
}
