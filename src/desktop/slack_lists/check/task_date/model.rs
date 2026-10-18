use super::*;
pub(super) fn initial(m: &Value) -> Result<(), String> {
    if m["document"]["id"] != "F_LIST_FIXTURE"
        || m["population"]["records"].as_array().map(Vec::len) != Some(1)
        || m["population"]["records"][0]["id"] != "R_DATE"
        || m["population"]["response_metadata"]["next_cursor"] != "retained-date-page-cursor"
        || m["selected"] != "V_TASK_DATE"
        || m["find"]["applied"] != "Review"
        || m["forms"] != json!([])
        || m["outcomes"] != json!([])
        || m["population"] != m["backend"]
        || m["todos"] != false
    {
        return Err("Unexpected initial date seed".into());
    }
    Ok(())
}
fn page(seed: &Value, date: &[&str]) -> Value {
    let mut p = seed.clone();
    p["records"][0]["fields"][1]["date"] = json!(date);
    p
}
pub(super) fn stage(initial: &Value, m: &Value, name: &str) -> Result<(), String> {
    let i = STAGES.iter().position(|s| *s == name).ok_or("Date stage")?;
    for key in ["document", "visit", "selection", "selected", "find"] {
        if m[key] != initial[key] {
            return Err(format!("Date changed retained {key}"));
        }
    }
    let counts = [0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2];
    let done = [0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2];
    let forms = m["forms"].as_array().ok_or("Forms shape")?;
    if forms.len() != counts[i]
        || m["outcomes"].as_array().map(Vec::len) != Some(done[i])
        || m["attempts"] != json!(counts[i])
        || m["pending"] != json!(counts[i] > done[i])
    {
        return Err("Unexpected date write/outcome count".into());
    }
    for (n, form) in forms.iter().enumerate() {
        let cells = if n == 0 {
            json!([{"row_id":"R_DATE","column_id":"C_DATE","date":["2026-10-18"]}])
        } else {
            json!([{"row_id":"R_DATE","column_id":"C_DATE","date":[]}])
        };
        let fields = json!([
            ["token", "fixture-not-a-token"],
            ["list_id", "F_LIST_FIXTURE"],
            ["cells", cells.to_string()],
            ["_x_reason", "bulk-update-cells"]
        ]);
        if form["method"] != "POST"
            || !form["url"].as_str().is_some_and(fixture::request_url)
            || form["retrySafe"] != false
            || form["cells"] != cells
            || form["fields"] != fields
        {
            return Err("Actual date form differs".into());
        }
        if let Some(outcome) = m["outcomes"].get(n) {
            if outcome != &json!({"serial":form["serial"],"lost":n==0}) {
                return Err("Date outcome custody differs".into());
            }
        }
    }
    let shown = if i >= 11 {
        page(&initial["population"], &[])
    } else if i >= 8 {
        page(&initial["population"], &["2026-10-18"])
    } else {
        initial["population"].clone()
    };
    let backend = if i >= 11 {
        page(&initial["population"], &[])
    } else if i >= 7 {
        page(&initial["population"], &["2026-10-18"])
    } else {
        initial["population"].clone()
    };
    if m["population"] != shown || m["backend"] != backend || m["todos"] != json!(i == 12) {
        return Err("Date raw page/cursor or policy differs".into());
    }
    let phase = match i {
        5 | 6 | 10 => Some("Saving"),
        7 => Some("Uncertain"),
        _ => None,
    };
    if m["mutation"]["phase"].as_str() != phase
        || m["editor"].is_object() != matches!(i, 1 | 2 | 4 | 5 | 9 | 10 | 12)
    {
        return Err("Date phase/editor custody differs".into());
    }
    if phase.is_some() {
        let last = forms.last().unwrap();
        if m["mutation"]["serial"] != last["serial"]
            || m["mutation"]["cells"] != last["cells"]
            || m["mutation"]["visit"] != initial["visit"]
            || m["mutation"]["selection"] != initial["selection"]
        {
            return Err("Retained date attempt differs".into());
        }
    }
    Ok(())
}
pub(super) fn bind(m: &Value, g: &Value) -> Result<(), String> {
    if g["ok"] != true
        || g["owner"]["visit"].as_str() != m["visit"].as_u64().map(|n| n.to_string()).as_deref()
        || g["owner"]["selection"].as_str()
            != m["selection"].as_u64().map(|n| n.to_string()).as_deref()
        || g["selected"] != m["selected"]
        || g["find"]["value"] != m["find"]["input"]
        || g["find"]["sameNode"] != true
    {
        return Err("DOM date owner/projection differs".into());
    }
    if g["modal"].is_object() != m["editor"].is_object() {
        return Err("DOM date modal differs".into());
    }
    if m["editor"].is_object()
        && (g["modal"]["serial"].as_str()
            != m["editor"]["generation"]
                .as_u64()
                .map(|n| n.to_string())
                .as_deref()
            || g["modal"]["disabled"] != m["editor"]["disabled"])
    {
        return Err("DOM date editor authority differs".into());
    }
    Ok(())
}

pub(super) fn continued(prior: &Value, current: &Value) -> Result<(), String> {
    for key in ["forms", "outcomes"] {
        let old = prior[key].as_array().ok_or("Prior date ledger shape")?;
        let new = current[key].as_array().ok_or("Current date ledger shape")?;
        if !new.starts_with(old) {
            return Err(format!("Retained date {key} changed while appending"));
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn append_cannot_rewrite_an_earlier_form_or_settled_outcome() {
        let prior = json!({"forms":[{"serial":301}],"outcomes":[{"serial":301,"lost":true}]});
        let desired = json!({"forms":[{"serial":301},{"serial":302}],"outcomes":[{"serial":301,"lost":true}]});
        assert!(continued(&prior, &desired).is_ok());
        let mut changed = desired.clone();
        changed["forms"][0]["serial"] = json!(999);
        changed["outcomes"][0]["serial"] = json!(999);
        assert!(continued(&prior, &changed).is_err());
        changed = desired;
        changed["outcomes"][0]["lost"] = json!(false);
        assert!(continued(&prior, &changed).is_err());
    }
}
