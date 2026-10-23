//! Presence checks distinguish an observed absent selector from unacquired data.
//! Values remain the original owner observations; no empty maps are synthesized.
use super::*;
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value, Error> {
    value
        .get(key)
        .ok_or_else(|| Error::MissingObservation(key.into()))
}
fn map<'a>(value: &'a Value, key: &str) -> Result<&'a Value, Error> {
    let v = field(value, key)?;
    if !v.is_object() {
        return Err(Error::InvalidInput);
    }
    Ok(v)
}
fn metadata(s: &Value, id: &str) -> Result<(), Error> {
    let m = field(&s["metadata"], id)?;
    if m.is_null() {
        return Ok(());
    }
    for k in [
        "latest", "lastRead", "draft", "starred", "unread", "priority",
    ] {
        field(m, k)?;
    }
    Ok(())
}
fn history(s: &Value, id: &str) -> Result<(), Error> {
    let h = field(&s["navigationHistory"], id)?;
    if !h.is_null() {
        field(h, "ts")?;
        field(h, "position")?;
    }
    Ok(())
}
pub(super) fn validate(s: &Value, rows: &Rows) -> Result<(), Error> {
    for key in ["selfId", "teamId"] {
        if field(s, key)?.as_str().is_none_or(str::is_empty) {
            return Err(Error::InvalidInput);
        }
    }
    if s["self"]["facts"]["team"]
        .as_str()
        .is_none_or(str::is_empty)
    {
        return Err(Error::MissingObservation("self team".into()));
    }
    for key in ["enterprise", "enterpriseId", "offline"] {
        field(s, key)?;
    }
    for key in ["metadata", "ims", "navigationHistory", "experiments"] {
        map(s, key)?;
    }
    for key in [
        "autocomplete_affinity_score",
        "autocomplete_affinity_ml_model",
    ] {
        field(&s["experiments"], key)?;
    }
    let offline = if s["offline"].is_null() {
        false
    } else if s["offline"].is_object() {
        true
    } else {
        return Err(Error::InvalidInput);
    };
    let affinity = s["experiments"]["autocomplete_affinity_score"] == "on"
        || s["experiments"]["autocomplete_affinity_ml_model"] == "on";
    if offline {
        for k in ["remoteDm", "remoteMpim", "entityCtr"] {
            map(s, k)?;
        }
        map(&s["offline"], "channel")?;
        if affinity {
            map(s, "affinity")?;
            map(s, "channelPriority")?;
        }
    }
    for row in rows.values() {
        let item = &row["item"];
        let id = item["id"].as_str().ok_or(Error::InvalidInput)?;
        if item["facts"]["member"] == true {
            for key in ["_real_name_normalized_lc", "_display_name_normalized_lc"] {
                if field(item, key)?.as_str().is_none() {
                    return Err(Error::InvalidInput);
                }
            }
            let im = field(&s["ims"], id)?;
            if !im.is_null() {
                let imid = im["id"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or(Error::InvalidInput)?;
                metadata(s, imid)?;
                history(s, imid)?;
            }
            if !s["enterpriseId"].is_null() && s["enterpriseId"] == item["enterprise_id"] {
                field(s, "defaultTeam")?;
                if !field(s, "searcherTeams")?.is_array() {
                    return Err(Error::InvalidInput);
                }
            }
            if offline {
                field(&s["remoteDm"], id)?;
                field(&s["remoteMpim"], id)?;
                field(&s["entityCtr"], "member")?;
                if affinity {
                    field(&s["affinity"], id)?;
                }
            }
        } else {
            metadata(s, id)?;
            history(s, id)?;
            if offline {
                field(&s["offline"]["channel"], id)?;
                field(&s["entityCtr"], "channel")?;
                if affinity {
                    field(&s["channelPriority"], id)?;
                }
            }
        }
    }
    Ok(())
}
