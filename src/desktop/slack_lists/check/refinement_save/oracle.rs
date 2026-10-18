//! Closed stage contract; full raw documents stay independent of the adapter reducer.
use super::*;
pub(super) const STAGES: &[&str] = &[
    "baseline", "local", "fields", "dirty", "name", "saving", "pending", "observed", "board",
];
pub(super) fn stages(mode: adapter::Case) -> Vec<&'static str> {
    STAGES
        .iter()
        .copied()
        .filter(|s| *s != "name" || mode.created())
        .collect()
}
pub(super) fn mode(mode: adapter::Case) -> &'static str {
    match mode {
        adapter::Case::Existing => "existing",
        adapter::Case::New => "new",
        adapter::Case::Lost => "lost",
    }
}
fn grouping() -> Value {
    json!({"group_by":"status","opaque":{"retained":[1,null,true]},"order":[{"select":[]},{"select":["OPT_DONE"]},{"select":["OPT_PROGRESS"]}]})
}
fn saved(original: &Value, mode: adapter::Case) -> Result<Value, String> {
    let mut raw = original.clone();
    let views = raw["list_metadata"]["views"]
        .as_array_mut()
        .ok_or("Missing original views")?;
    if mode.created() {
        views.push(json!({"id":"V_REFINED_CREATED","name":"Focused delivery","type":"board","position":"0000000009","columns":[{"id":"C_NAME","key":"NAME","visible":true},{"id":"C_NUMBER","key":"estimate","visible":false}],"grouping":grouping(),"show_completed_items":false}));
    } else {
        let v = views
            .iter_mut()
            .find(|v| v["id"] == "V_LANE")
            .ok_or("Missing original view")?;
        v["columns"][1]["visible"] = json!(false);
        v["grouping"] = grouping();
        v["filters"] = json!([]);
        v["info_column_filters"] = json!([]);
        v["sorts"] = json!([]);
        v["show_completed_items"] = json!(false);
    }
    Ok(raw)
}
fn lanes(reordered: bool) -> Value {
    let progress = json!({"key":"OPT_PROGRESS","label":"In progress","count":9,"rows":(0..9).map(|i|format!("R_LIST_{i:03}")).collect::<Vec<_>>()});
    let done = json!({"key":"OPT_DONE","label":"Done","count":0,"rows":[]});
    if reordered {
        json!([done, progress])
    } else {
        json!([progress, done])
    }
}
pub(super) fn state(
    mode: adapter::Case,
    name: &str,
    raw: &Value,
    p: &Value,
    ledger: &Value,
    original: &Value,
) -> Result<(), String> {
    if !stages(mode).contains(&name) {
        return Err("Unsupported refinement stage".into());
    }
    let saved_stage = matches!(name, "observed" | "board");
    let pending_new = name == "pending" && mode == adapter::Case::New;
    let selected = if mode.created()
        && (mode == adapter::Case::New && (name == "pending" || saved_stage) || name == "board")
    {
        "V_REFINED_CREATED"
    } else {
        "V_LANE"
    };
    let hidden = !matches!(name, "baseline" | "local" | "fields");
    let reordered = name != "baseline" && !(name == "pending" && mode == adapter::Case::Existing);
    let doc = if saved_stage {
        saved(original, mode)?
    } else {
        original.clone()
    };
    let (requests, finished) = match name {
        "saving" => (5, 0),
        "pending" if mode == adapter::Case::Lost => (5, 1),
        "pending" => (7, 1),
        "observed" | "board" if mode == adapter::Case::Lost => (8, 2),
        "observed" | "board" => (10, 2),
        _ => (0, 0),
    };
    let expected_stage = match name {
        "fields" | "name" => Some("Editing"),
        "saving" => Some("Saving"),
        "pending" if mode == adapter::Case::Lost => Some("Uncertain"),
        "pending" => Some("Checking"),
        "observed" if mode == adapter::Case::Lost => Some("Available"),
        "observed" => Some("Observed"),
        _ => None,
    };
    if raw["document"] != doc
        || raw["loading"] != false
        || raw["stale"] != false
        || raw["unavailable"] != false
        || raw["pending"]
            != if pending_new {
                json!("V_REFINED_CREATED")
            } else {
                Value::Null
            }
        || raw["selected"] != selected
        || p["selected"] != selected
        || raw["stage"].as_str() != expected_stage
        || p["pending"] != pending_new
        || p["matched"] != if pending_new { json!(0) } else { json!(9) }
        || p["lanes"]
            != if pending_new {
                json!([])
            } else {
                lanes(reordered)
            }
        || p["columns"]
            != if pending_new {
                json!([])
            } else if hidden {
                json!(["C_NAME"])
            } else {
                json!(["C_NAME", "C_NUMBER"])
            }
        || ledger["requests"].as_array().map(Vec::len) != Some(requests)
        || ledger["responses"].as_array().map(Vec::len) != Some(requests)
        || ledger["finished"].as_array().map(Vec::len) != Some(finished)
        || ledger["writePending"] != (name == "saving")
        || ledger["readPending"] != (name == "pending" && mode != adapter::Case::Lost)
        || ledger["preferencePending"] != (name == "pending" && mode != adapter::Case::Lost)
        || ledger["population"] != raw["population"]
        || ledger["server"]
            != if name == "pending" || saved_stage {
                saved(original, mode)?
            } else {
                original.clone()
            }
    {
        return Err(format!(
            "Refinement {name} raw state/projection/ledger differs"
        ));
    }
    let has_provider = !matches!(name, "baseline" | "board" | "observed")
        && !(name == "pending" && mode != adapter::Case::Lost)
        || matches!(name, "board" | "observed") && mode == adapter::Case::Lost;
    if raw["provider"]
        != if has_provider {
            grouping()["order"].clone()
        } else {
            Value::Null
        }
    {
        return Err("Provider reset/local move mismatch".into());
    }
    let local_empty = matches!(name, "baseline" | "local" | "fields")
        || mode == adapter::Case::New && (name == "pending" || saved_stage);
    if raw["localEmpty"] != local_empty {
        return Err("Local map reset/preservation mismatch".into());
    }
    let mut local_doc = doc;
    if !local_empty {
        local_doc["list_metadata"]["views"][1]["columns"][1]["visible"] = json!(false);
    }
    if raw["localDocument"] != local_doc {
        return Err("Local visibility changed unrelated metadata".into());
    }
    if name == "fields"
        && (raw["visibility"]["column"] != "C_NUMBER"
            || raw["visibility"]["original"] != true
            || raw["visibility"]["desired"] != false)
    {
        return Err("Fields captured another local visibility draft".into());
    }
    if name == "name" && raw["name"] != "Focused delivery" {
        return Err("New view name differs".into());
    }
    let prefs = if saved_stage && mode != adapter::Case::Lost {
        json!({"ok":true,"prefs":{"lists_default_view":json!({"F_OTHER":"V_OTHER","F_LIST_FIXTURE":selected,"opaque":"retained"}).to_string()}})
    } else {
        json!({"ok":true,"prefs":{"lists_default_view":"{ \"F_OTHER\": \"V_OTHER\", \"F_LIST_FIXTURE\": \"V_PRECISION\", \"opaque\": \"retained\" }"}})
    };
    if ledger["preferences"] != prefs {
        return Err("Whole preference map/encoding changed unexpectedly".into());
    }
    if matches!(name, "saving" | "pending" | "observed") {
        let preference = if name == "saving" || mode == adapter::Case::Lost {
            "NotRequested"
        } else if name == "pending" {
            "InFlight"
        } else {
            "Observed"
        };
        if raw["receipt"]["preference"] != preference || raw["receipt"]["custodyCurrent"] != true {
            return Err("Independent preference phase/custody mismatch".into());
        }
    }
    Ok(())
}
