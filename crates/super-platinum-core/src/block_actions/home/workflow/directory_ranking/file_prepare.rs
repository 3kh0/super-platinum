//! Ejs0 file projection, preserving null-versus-absent DM observations.
use super::Error;
use super::features::{number, parse_float, text, truth};
use serde_json::{Value, json};
fn finite(n: f64) -> Value {
    serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number)
}
fn ctr(s: &Value, kind: &str, id: &str) -> f64 {
    s["offline"][kind][id]["user_ctr_30dd"]
        .as_f64()
        .unwrap_or_else(|| number(&s["entityCtr"][kind]))
}
fn max_ctr(s: &Value, values: &Value, kind: &str) -> f64 {
    values.as_array().map_or(0.0, |ids| {
        ids.iter()
            .map(|id| ctr(s, kind, text(id)))
            .fold(f64::NEG_INFINITY, f64::max)
    })
}
fn share_max(v: &Value) -> Result<f64, Error> {
    if v.is_null() {
        return Ok(0.0);
    }
    let map = v.as_object().ok_or(Error::InvalidInput)?;
    let mut max = f64::NEG_INFINITY;
    for shares in map.values() {
        for share in shares.as_array().ok_or(Error::InvalidInput)? {
            let n = if truth(&share["ts"]) {
                parse_float(&share["ts"])
            } else {
                f64::NEG_INFINITY
            };
            if n.is_nan() {
                return Ok(f64::NAN);
            }
            max = max.max(n);
        }
    }
    Ok(max)
}
pub(super) fn apply(
    e: &mut Value,
    s: &Value,
    offline: bool,
    affinity: bool,
    self_id: &str,
) -> Result<(), Error> {
    let file = e["item"].clone();
    e["suggestionType"] = json!("file-navigation");
    e["isNavigational"] = json!(true);
    e["fileSuggestionsFromSearcher"] = json!(true);
    e["fileId"] = file["id"].clone();
    e["logId"] = file["id"].clone();
    e["fileType"] = if truth(&file["highlight_type"]) {
        file["highlight_type"].clone()
    } else {
        file["filetype"].clone()
    };
    e["name"] = file["title"].clone();
    e["fromCurrentUser"] = json!(file["user"].as_str() == Some(self_id));
    // In JS, absent user yields undefined, whereas explicit null stays null.
    // The source compares !== null, not truthiness and not a synthesized DM ID.
    let user = file.get("user");
    let has_user = truth(&file["user"]);
    let id = text(&file["user"]);
    let local_null = if has_user {
        s["ims"][id].is_null()
    } else {
        user == Some(&Value::Null)
    };
    let remote_null = if has_user {
        !offline || s["remoteDm"].is_null() || s["remoteDm"].get(id) == Some(&Value::Null)
    } else {
        user == Some(&Value::Null)
    };
    e["hasNonNullDmsWithCreator"] = json!(!local_null);
    e["hasNonNullLocalOrRemoteDmsWithCreator"] = json!(!local_null || !remote_null);
    if let Some(v) = file.get("last_read") {
        e["lastReadTs"] = v.clone();
    }
    if !file["shares"].is_null() {
        let public = share_max(&file["shares"]["public"])?;
        let private = share_max(&file["shares"]["private"])?;
        let valid = |v: f64| v != 0.0 && !v.is_nan();
        let latest = if valid(public) && valid(private) {
            public.max(private)
        } else if valid(public) {
            public
        } else {
            private
        };
        e["latestShareTs"] = finite(latest);
    }
    if truth(&file["facts"]["latestAction"]) {
        e["latestActionTs"] = file["facts"]["latestAction"].clone();
    }
    if offline {
        let channels = max_ctr(s, &file["channels"], "channel");
        let groups = max_ctr(s, &file["groups"], "mpim");
        if channels > 0.0 || groups > 0.0 {
            e["fileChannelCtr"] = finite(channels.max(groups));
        }
        e["fileUserCtr"] = finite(if has_user {
            ctr(s, "member", id)
        } else {
            number(&s["entityCtr"]["member"])
        });
    }
    if affinity {
        e["fileIsTopEngaged"] = json!(truth(&file["isTopEngagedFile"]));
    }
    Ok(())
}
