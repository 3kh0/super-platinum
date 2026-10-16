//! Exact real-builder oracle with only the three generated query UUIDs varying.
use super::{ORIGIN, PreparedRequest};

pub(super) fn matches(actual: &PreparedRequest, expected: &PreparedRequest) -> bool {
    if actual.method != expected.method
        || actual.headers != expected.headers
        || actual.body != expected.body
    {
        return false;
    }
    let Some((actual_base, actual_query)) = actual.url.split_once('?') else {
        return false;
    };
    let Some((expected_base, expected_query)) = expected.url.split_once('?') else {
        return false;
    };
    if expected_base != format!("{ORIGIN}/api/files.share") || actual_base != expected_base {
        return false;
    }
    let actual: Vec<_> = actual_query.split('&').collect();
    let expected: Vec<_> = expected_query.split('&').collect();
    if actual.len() != expected.len() {
        return false;
    }
    let mut generated = Vec::new();
    for (actual, expected) in actual.iter().zip(&expected) {
        let Some((key, value)) = actual.split_once('=') else {
            return false;
        };
        let Some((expected_key, expected_value)) = expected.split_once('=') else {
            return false;
        };
        if key != expected_key {
            return false;
        }
        if matches!(key, "_x_id" | "_x_csid" | "fp") {
            if !simple_v4(value) || !simple_v4(expected_value) || generated.contains(&value) {
                return false;
            }
            generated.push(value);
        } else if value != expected_value {
            return false;
        }
    }
    generated.len() == 3
}

fn simple_v4(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 32
        && bytes
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        && bytes[12] == b'4'
        && matches!(bytes[16], b'8' | b'9' | b'a' | b'b')
}
