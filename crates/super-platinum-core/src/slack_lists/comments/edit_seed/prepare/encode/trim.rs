use super::*;
pub(super) fn token(op: &Value) -> bool {
    ["slackmention", "slackslug", "slacksearchfilter"]
        .iter()
        .any(|k| truthy(op.get("attributes").and_then(|a| a.get(*k))))
}
fn protected(attributes: &Map<String, Value>) -> bool {
    ["code-block", "blockquote", "list"]
        .iter()
        .any(|k| attributes.contains_key(*k))
}
pub(super) fn start(ops: &[Value]) -> Result<Vec<Value>> {
    let mut ops = ops.to_vec();
    let first = ops
        .iter()
        .position(|op| {
            let text = op["insert"].as_str();
            token(op)
                || text.is_none_or(|s| !s.chars().all(chunks::text::js_space))
                || op
                    .get("attributes")
                    .and_then(Value::as_object)
                    .is_some_and(protected)
        })
        .unwrap_or(ops.len());
    ops.drain(..first);
    if ops
        .first()
        .is_some_and(|op| op["insert"].is_string() && !token(op))
    {
        // 0523 merges the reached first op attributes with its newline attributes.
        let document = Document::capture(&ops)?;
        let mut attributes = ops[0]
            .get("attributes")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some((_, _, newline)) = document.lines().first() {
            attributes.extend(newline.clone());
        }
        if !protected(&attributes) {
            let trimmed = ops[0]["insert"]
                .as_str()
                .unwrap()
                .trim_start_matches(chunks::text::js_space)
                .to_owned();
            ops[0]["insert"] = json!(trimmed);
            if ops[0]["insert"] == "" {
                ops.remove(0);
            }
        }
    }
    Ok(ops)
}
