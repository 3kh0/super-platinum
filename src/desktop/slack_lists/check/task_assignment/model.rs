use super::*;
pub(super) fn bind(m: &Value, g: &Value) -> Result<(), String> {
    if g["ok"] != true
        || g["owner"]["visit"].as_str() != m["visit"].as_u64().map(|v| v.to_string()).as_deref()
        || g["editor"].is_null() != m["editor"].is_null()
    {
        return Err("Assignment DOM owner differs".into());
    }
    if m["editor"].is_object() {
        if g["editor"]["generation"].as_str()
            != m["editor"]["generation"]
                .as_u64()
                .map(|v| v.to_string())
                .as_deref()
            || g["review"].is_null() != m["editor"]["review"].is_null()
        {
            return Err("Assignment editor epoch/review differs".into());
        }
        if g["review"].is_object() {
            if g["review"]["serial"].as_str()
                != m["entry"]["serial"]
                    .as_u64()
                    .map(|v| v.to_string())
                    .as_deref()
            {
                return Err("Assignment review serial differs".into());
            }
            let people = m["editor"]["review"]["people"]
                .as_array()
                .ok_or("Model people")?;
            let actual = g["review"]["people"].as_array().ok_or("DOM people")?;
            if people.len() != actual.len()
                || people.iter().zip(actual).any(|(p, a)| {
                    p["id"] != a["id"]
                        || p["grant"] != a["value"]
                        || a["disabled"]
                            != json!(
                                !(m["editor"]["review"]["canShare"] == true
                                    && p["status"] == "Not shared")
                            )
                })
            {
                return Err("Assignment grant controls differ".into());
            }
        } else {
            if g["query"]["value"] != m["editor"]["query"] {
                return Err("Assignment query differs".into());
            }
            let choices = m["editor"]["choices"].as_array().ok_or("Model choices")?;
            let actual = g["options"].as_array().ok_or("DOM choices")?;
            if choices.len() != actual.len()
                || choices
                    .iter()
                    .zip(actual)
                    .any(|(p, a)| p["id"] != a["id"] || p["label"] != a["label"]["text"]["text"])
            {
                return Err("Assignment option inventory differs".into());
            }
        }
    }
    Ok(())
}
fn population(base: &Value, users: &[&str]) -> Value {
    let mut p = base.clone();
    let fields = p["records"][0]["fields"].as_array_mut().unwrap();
    let cell = fields.iter_mut().find(|f| f["key"] == "assignee").unwrap();
    cell["user"] = json!(users);
    p
}
fn users(index: usize) -> &'static [&'static str] {
    match index {
        0 => &["U00000001"],
        1 => &["U00000001", "U00000002", "W00000003"],
        2 => &[],
        _ => &["U00000002"],
    }
}
pub(super) fn validate(initial: &Value, proof: &[Value], last: &Value) -> Result<(), String> {
    if proof.len() != STAGES.len()
        || initial["population"]["records"].as_array().map(Vec::len) != Some(1)
        || initial["population"]["records"][0]["id"] != "R_TASK"
        || initial["population"]["response_metadata"]["next_cursor"] != "retained-page-cursor"
        || initial["ledger"]["requests"] != json!([])
        || initial["ledger"]["cells"] != json!([])
    {
        return Err("Unexpected assignment setup".into());
    }
    for (i, p) in proof.iter().enumerate() {
        let m = &p["model"];
        let written = if i < 8 {
            0
        } else if i < 12 {
            1
        } else if i < 18 {
            2
        } else {
            3
        };
        let completed = if i < 9 {
            0
        } else if i < 13 {
            1
        } else if i < 19 {
            2
        } else {
            3
        };
        let observed = if i < 10 {
            0
        } else if i < 14 {
            1
        } else if i < 20 {
            2
        } else {
            3
        };
        if p["name"] != STAGES[i]
            || m["document"] != initial["document"]
            || m["backendDocument"] != initial["document"]
            || m["population"] != population(&initial["population"], users(observed))
            || m["backend"] != population(&initial["population"], users(completed))
            || m["visit"] != initial["visit"]
            || m["selection"] != initial["selection"]
            || m["find"] != initial["find"]
            || m["ledger"]["cells"].as_array().map(Vec::len) != Some(written)
            || m["ledger"]["cellCompletions"].as_array().map(Vec::len) != Some(completed)
            || m["noSession"] != true
            || m["noTransport"] != true
        {
            return Err(format!(
                "Assignment raw page/count custody differs at {}",
                STAGES[i]
            ));
        }
        bind(m, &p["geometry"])?;
    }
    if proof.last().map(|p| &p["model"]) != Some(last)
        || !last["entry"].is_null()
        || !last["mutation"].is_null()
        || !last["ledger"]["pending"].is_null()
    {
        return Err("Assignment terminal state differs".into());
    }
    let forms = last["ledger"]["cells"].as_array().ok_or("Cell forms")?;
    for (i, form) in forms.iter().enumerate() {
        let cells = json!([{"row_id":"R_TASK","column_id":"C_TASK","user":users(i+1)}]);
        let expected = json!([
            ["token", "fixture-not-a-token"],
            ["list_id", "F_LIST_FIXTURE"],
            ["cells", cells.to_string()],
            ["_x_reason", "bulk-update-cells"]
        ]);
        if form["cells"] != cells
            || form["fields"] != expected
            || form["retrySafe"] != false
            || form["method"] != "POST"
        {
            return Err("Actual assignment cell form differs".into());
        }
    }
    let requests = last["ledger"]["requests"]
        .as_array()
        .ok_or("Request ledger")?;
    let completed = last["ledger"]["completed"]
        .as_array()
        .ok_or("Request completions")?;
    if requests.len() != 56 || completed.len() != 56 {
        return Err("Assignment access/share request count differs".into());
    }
    for (request, response) in requests.iter().zip(completed) {
        for key in ["serial", "job", "draft", "method", "fields"] {
            if request[key] != response[key] {
                return Err(format!("Assignment response {key} custody differs"));
            }
        }
    }
    let shares = requests
        .iter()
        .filter(|v| v["method"] == "files.share")
        .collect::<Vec<_>>();
    if shares.len() != 2
        || requests
            .iter()
            .filter(|v| v["method"] == "conversations.open")
            .count()
            != 2
    {
        return Err("Share/DM attempt replay or omission".into());
    }
    for (i, s) in shares.iter().enumerate() {
        let fields = s["fields"].as_array().ok_or("Share fields")?;
        if fields.iter().find(|v| v[0] == "channel").map(|v| &v[1])
            != Some(&json!(if i == 0 { "D00000002" } else { "D00000003" }))
        {
            return Err("Share recipient differs".into());
        }
    }
    Ok(())
}
