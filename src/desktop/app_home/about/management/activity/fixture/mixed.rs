//! Exact mixed navigation vectors; existing standalone fixtures remain unchanged.
use super::*;

pub(crate) fn is_case(value: &str) -> bool {
    matches!(
        value,
        "mixed-navigation" | "mixed-navigation-reverse" | "mixed-navigation-success"
    )
}
pub(super) fn messages(row: &mut Value) {
    row["trigger_message"] = json!({"template":"Open {list} then {list}",
        "variables":{"list":{"type":"LIST","value":file::LIST}}});
    row["step_events"][0]["outcome_message"] = json!({"template":"Check {missing}",
        "variables":{"missing":{"type":"CHANNEL","value":channel::MISSING}}});
    let mut canvas = row["step_events"][0].clone();
    canvas["step_execution_id"] = json!("STEP_MIXED_CANVAS");
    canvas["outcome_message"] = json!({"template":"Read {canvas}",
        "variables":{"canvas":{"type":"CANVAS","value":file::CANVAS}}});
    row["step_events"].as_array_mut().unwrap().push(canvas);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_mixed_vectors_keep_original_missing_and_duplicate_list_occurrences() {
        let mut row = json!({"step_events":[{"step_execution_id":"STEP_REVIEW",
            "future":{"preserved":17},"step_status":"SUCCESS"}]});
        messages(&mut row);
        assert_eq!(
            row["trigger_message"]["template"],
            "Open {list} then {list}"
        );
        assert_eq!(row["step_events"][0]["step_execution_id"], "STEP_REVIEW");
        assert_eq!(
            row["step_events"][0]["outcome_message"]["variables"]["missing"]["value"],
            channel::MISSING
        );
        assert_eq!(
            row["step_events"][1]["outcome_message"]["variables"]["canvas"]["value"],
            file::CANVAS
        );
        assert_eq!(row["step_events"][1]["future"], json!({"preserved":17}));
        assert!(!is_case("channel"));
        assert!(!is_case("file"));
        assert!(!is_case("mixed-navigation-foreign"));
    }
}
