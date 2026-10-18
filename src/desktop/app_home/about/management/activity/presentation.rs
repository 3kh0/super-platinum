//! AjOk Activity fields and StepText projection. Raw receipts remain untouched.
//! No permission grant, renderer, entity acquisition or navigation lives here.
use serde_json::Value;
use super_platinum_core::block_actions::workflow_management::ActivityDetail;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Part {
    Text(String),
    Emoji(String),
    EmojiText(String),
    Channel(String),
    User(String),
    Canvas(String),
    List(String),
}

/// Exact ordinary ActivityStatus branch; friendly step names need their own
/// observed experiment. Unknown status renders no source label.
pub(super) fn status(value: &Value) -> String {
    match value.as_str() {
        Some("SUCCESS" | "completed") => "Completed",
        Some("PENDING" | "started") => "Processing",
        Some("ERROR" | "failed") => "Failed",
        Some("CANCELED") => "Canceled",
        Some("PENDING_USER_ACTION") => "Awaiting user action",
        Some("REPEATER_STARTED") => "Repeat step started",
        Some("EXPIRED") => "Expired",
        _ => "",
    }
    .into()
}

pub(super) fn list_status(row: &Value) -> String {
    status(&row["workflow_instance_status"])
}

pub(super) fn detail_status(detail: &ActivityDetail) -> String {
    status(&detail.event()["workflow_status"])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StepState {
    pub(super) error: bool,
    pub(super) success: bool,
}

pub(super) fn step_state(
    step: &Value,
    index: usize,
    total: usize,
    selected: Option<&Value>,
) -> StepState {
    let last = total > 0 && index == total - 1;
    let status = selected.and_then(|row| row["workflow_instance_status"].as_str());
    StepState {
        error: step["step_status"].as_str() == Some("ERROR") || (last && status == Some("ERROR")),
        success: last && status == Some("SUCCESS"),
    }
}

/// The non-experiment error column: trimmed last_activity, child error count,
/// then the source em dash. This does not guess friendly error messages.
pub(super) fn row_error(row: &Value) -> String {
    if let Some(text) = row["last_activity"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return text.into();
    }
    match row["child_execution_details"]["error_count"].as_u64() {
        Some(1) => "1 error".into(),
        Some(n) if n > 1 => format!("{n} errors"),
        _ => "\u{2014}".into(),
    }
}

pub(super) fn time(value: &Value, milliseconds: bool) -> String {
    let Some(mut seconds) = value.as_f64().filter(|n| n.is_finite() && *n >= 0.0) else {
        return "Date unavailable".into();
    };
    if milliseconds {
        seconds /= 1000.0;
    }
    let stamp = format!("{:.0}", seconds.floor());
    format!(
        "{} at {}",
        super_platinum_core::state::format_ts_date_label(&stamp),
        super_platinum_core::state::format_ts_hm(&stamp)
    )
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}
pub(super) fn parts(message: &Value) -> Vec<Part> {
    let Some(template) = message["template"].as_str() else {
        return vec![Part::Text("Step details unavailable".into())];
    };
    let variables = &message["variables"];
    let mut output = Vec::new();
    let mut consumed = 0;
    let mut search = 0;
    while let Some(offset) = template[search..].find('{') {
        let start = search + offset;
        let tail = &template[start + 1..];
        let length = tail
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
            .count();
        if length == 0 || tail.as_bytes().get(length) != Some(&b'}') {
            search = start + 1;
            continue;
        }
        let end = start + length + 2;
        if start > consumed {
            output.push(Part::Text(template[consumed..start].into()));
        }
        let replacement = &variables[&tail[..length]];
        let value = replacement["value"].as_str();
        let inherited = variables.is_object()
            && variables.get(&tail[..length]).is_none()
            && [
                "constructor",
                "__defineGetter__",
                "__defineSetter__",
                "hasOwnProperty",
                "__lookupGetter__",
                "__lookupSetter__",
                "isPrototypeOf",
                "propertyIsEnumerable",
                "toString",
                "valueOf",
                "__proto__",
                "toLocaleString",
            ]
            .contains(&&tail[..length]);
        let part = if inherited {
            Part::Text("replacement.value".into())
        } else if !truthy(replacement) {
            Part::Text(template[start..end].into())
        } else {
            match (replacement["type"].as_str(), value) {
                (Some("CHANNEL"), Some(v)) => Part::Channel(v.into()),
                (Some("USER"), Some(v)) => Part::User(v.into()),
                (Some("EMOJI"), Some(v)) => Part::Emoji(v.into()),
                (Some("CANVAS"), Some(v)) => Part::Canvas(v.into()),
                (Some("LIST"), Some(v)) => Part::List(v.into()),
                // This literal is the retained source's actual fallback.
                _ => Part::Text("replacement.value".into()),
            }
        };
        output.push(part);
        consumed = end;
        search = end;
    }
    if consumed < template.len() {
        output.push(Part::EmojiText(template[consumed..].into()));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use super_platinum_core::block_actions::workflow_management::ActivityDetailRequest;

    #[test]
    fn detail_header_uses_its_own_status_but_final_step_keeps_selected_list_outcome() {
        let row = json!({"workflow_instance_status":"ERROR","workflow_status":"SUCCESS"});
        let detail = ActivityDetailRequest::new("WF_REVIEW", "EXECUTION")
            .unwrap()
            .decode(json!({"ok":true,"workflow_event":{
                "workflow_status":"SUCCESS","workflow_instance_status":"ERROR","step_events":[]}}))
            .unwrap();
        assert_eq!(list_status(&row), "Failed");
        assert_eq!(detail_status(&detail), "Completed");
        let step = json!({"step_status":"SUCCESS"});
        assert_eq!(
            step_state(&step, 0, 2, Some(&row)),
            StepState {
                error: false,
                success: false
            }
        );
        assert_eq!(
            step_state(&step, 1, 2, Some(&row)),
            StepState {
                error: true,
                success: false
            }
        );
        assert_eq!(
            step_state(&step, 0, 0, Some(&row)),
            StepState {
                error: false,
                success: false
            }
        );
        assert_eq!(
            step_state(&json!({"step_status":"ERROR"}), 0, 2, None),
            StepState {
                error: true,
                success: false
            }
        );
        assert!(
            step_state(
                &step,
                1,
                2,
                Some(&json!({"workflow_instance_status":"SUCCESS"}))
            )
            .success
        );
    }

    #[test]
    fn source_status_labels_and_unknown_are_distinct_from_filter_copy() {
        for (raw, label) in [
            ("PENDING", "Processing"),
            ("started", "Processing"),
            ("PENDING_USER_ACTION", "Awaiting user action"),
            ("completed", "Completed"),
            ("failed", "Failed"),
            ("EXPIRED", "Expired"),
            ("FUTURE", ""),
        ] {
            assert_eq!(status(&json!(raw)), label);
        }
        assert_eq!(status(&Value::Null), "");
    }

    #[test]
    fn ordinary_error_column_does_not_adopt_unobserved_friendly_error_experiment() {
        assert_eq!(
            row_error(
                &json!({"last_activity":"  error_code  ","friendly_error_message":"ignored",
            "child_execution_details":{"error_count":2}})
            ),
            "error_code"
        );
        assert_eq!(
            row_error(&json!({"last_activity":" ","child_execution_details":{"error_count":1}})),
            "1 error"
        );
        assert_eq!(
            row_error(&json!({"child_execution_details":{"error_count":2}})),
            "2 errors"
        );
        assert_eq!(row_error(&json!({"friendly_error_message":"ignored"})), "—");
    }

    #[test]
    fn step_milliseconds_and_list_seconds_project_the_same_instant() {
        assert_eq!(
            time(&json!(1_700_000_000_999_u64), true),
            time(&json!(1_700_000_000), false)
        );
        assert_eq!(time(&json!("1700000000"), false), "Date unavailable");
        assert_eq!(time(&json!(-1), false), "Date unavailable");
    }

    #[test]
    fn source_step_text_is_typed_without_interpolating_markup() {
        let message = json!({"template":"雪 {u}{c}{e}{f}{l}{x} tail :wave:","variables":{
            "u":{"type":"USER","value":"U1"},"c":{"type":"CHANNEL","value":"C1"},
            "e":{"type":"EMOJI","value":":wave:"},"f":{"type":"CANVAS","value":"F1"},
            "l":{"type":"LIST","value":"F2"},"x":{"type":"FUTURE","value":"<script>"}}});
        assert_eq!(
            parts(&message),
            vec![
                Part::Text("雪 ".into()),
                Part::User("U1".into()),
                Part::Channel("C1".into()),
                Part::Emoji(":wave:".into()),
                Part::Canvas("F1".into()),
                Part::List("F2".into()),
                Part::Text("replacement.value".into()),
                Part::EmojiText(" tail :wave:".into())
            ]
        );
    }

    #[test]
    fn placeholder_word_grammar_missing_falsy_and_inherited_keys_match_source() {
        let message = json!({"template":"{a}{b}{c}{d}{missing}{é}{😀} {constructor}",
            "variables":{"a":null,"b":false,"c":0,"d":""}});
        assert_eq!(
            parts(&message),
            vec![
                Part::Text("{a}".into()),
                Part::Text("{b}".into()),
                Part::Text("{c}".into()),
                Part::Text("{d}".into()),
                Part::Text("{missing}".into()),
                Part::Text("{é}{😀} ".into()),
                Part::Text("replacement.value".into())
            ]
        );
        assert_eq!(parts(&json!({"template":"","variables":{}})), vec![]);
        assert_eq!(
            parts(&json!({"template":"plain :wave: <@U1>","variables":{}})),
            vec![Part::EmojiText("plain :wave: <@U1>".into())]
        );
        assert_eq!(
            parts(&json!({"template":"{constructor}","variables":{"constructor":false}})),
            vec![Part::Text("{constructor}".into())]
        );
    }
}
