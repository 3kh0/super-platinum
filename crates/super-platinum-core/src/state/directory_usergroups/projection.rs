use super::*;
use serde_json::json;

pub(super) fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
pub(super) fn known(v: &Value) -> bool {
    !truth(&v["isUnknown"]) && !truth(&v["isNonExistent"])
}
pub(super) fn id(v: &Value) -> Result<&str> {
    v.get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or(Error::Shape)
}
pub(super) fn strings(v: &Value) -> Result<Vec<String>> {
    v.as_array()
        .ok_or(Error::Shape)?
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .ok_or(Error::Shape)
        })
        .collect()
}
fn row<'a>(rows: &'a [Current], identifier: &str) -> Option<&'a Current> {
    rows.iter().find(|r| r.effective["id"] == identifier)
}
fn version(v: &Value) -> &Value {
    if known(v) {
        &v["date_update"]
    } else {
        &Value::Null
    }
}
fn strict_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        _ => a == b,
    }
}
fn skip(origin: Origin, incoming: &Value, old: Option<&Current>) -> bool {
    origin == Origin::Fetch
        && truth(version(incoming))
        && old.is_some_and(|old| strict_equal(version(&old.effective), version(incoming)))
}
pub(super) fn validate(op: &Operation) -> Result<()> {
    match op {
        Operation::Metadata {
            rows,
            native_members,
            ..
        } => {
            if rows.len() > MAX_ROWS {
                return Err(Error::Limit);
            }
            for raw in rows {
                id(raw)?;
                if !raw.is_object() {
                    return Err(Error::Shape);
                }
            }
            if let NativeMembers::Replace(ids) = native_members {
                if ids.iter().any(String::is_empty) {
                    return Err(Error::Shape);
                }
            }
        }
        Operation::Membership { ids } => {
            if ids.iter().any(String::is_empty) {
                return Err(Error::Shape);
            }
        }
        Operation::Members { id, users, .. } => {
            if id.is_empty() || users.iter().any(String::is_empty) {
                return Err(Error::Shape);
            }
        }
        Operation::Event { kind, raw, .. } => {
            if !raw.is_object() {
                return Err(Error::Shape);
            }
            match kind.as_str() {
                "subteam_created" | "subteam_updated" => {
                    id(&raw["subteam"])?;
                }
                "subteam_members_changed" | "subteam_self_added" | "subteam_self_removed" => {
                    if raw["subteam_id"].as_str().is_none_or(str::is_empty) {
                        return Err(Error::Shape);
                    }
                    for key in ["added_users", "removed_users"] {
                        if let Some(v) = raw.get(key) {
                            strings(v)?;
                        }
                    }
                }
                _ => return Err(Error::Shape),
            }
        }
    }
    Ok(())
}
fn event_metadata(rows: &[Current], kind: &str, raw: &Value) -> Result<Vec<Value>> {
    if kind == "subteam_created" {
        return Ok(vec![raw["subteam"].clone()]);
    }
    if kind != "subteam_updated" {
        return Ok(vec![]);
    }
    let incoming = &raw["subteam"];
    let Some(old) = row(rows, id(incoming)?).filter(|r| known(&r.effective)) else {
        return Ok(vec![]);
    };
    let mut incoming = incoming.clone();
    incoming["users"] = if truth(&old.effective["users"]) {
        old.effective["users"].clone()
    } else {
        json!([])
    };
    Ok(vec![incoming])
}
pub(super) fn fields(rows: &[Current], op: &Operation) -> Result<Vec<String>> {
    let (origin, input) = match op {
        Operation::Metadata { origin, rows, .. } => (*origin, rows.clone()),
        Operation::Event { kind, raw, .. } => (Origin::Upsert, event_metadata(rows, kind, raw)?),
        _ => return Ok(vec![]),
    };
    let mut fields = vec![];
    for raw in input {
        if skip(origin, &raw, row(rows, id(&raw)?)) {
            continue;
        }
        match raw.get("name") {
            Some(Value::String(name)) if !name.is_empty() => {
                if !fields.contains(name) {
                    fields.push(name.clone())
                }
            }
            None | Some(Value::Null | Value::String(_)) => {}
            _ => return Err(Error::Shape),
        }
    }
    Ok(fields)
}
pub(super) fn apply(
    saved: &mut Saved,
    native: &mut Native,
    op: &Operation,
    lowered: &BTreeMap<String, String>,
) -> Result<Vec<Current>> {
    validate(op)?;
    match op {
        Operation::Metadata {
            origin,
            rows,
            native_members,
        } => metadata(saved, native, *origin, rows, native_members, lowered),
        Operation::Membership { ids } => {
            saved.membership = Some(ids.clone());
            saved.membership_flags = ids.iter().map(|id| (id.clone(), true)).collect();
            for group in native.values_mut() {
                group.users.retain(|u| u != &saved.user)
            }
            for id in ids {
                let group = native.entry(id.clone()).or_insert_with(|| UserGroup {
                    id: id.clone(),
                    ..Default::default()
                });
                if !group.users.contains(&saved.user) {
                    group.users.push(saved.user.clone())
                }
            }
            Ok(vec![])
        }
        Operation::Members {
            id,
            users,
            append,
            skip_org_count,
        } => {
            if let Some(current) = saved
                .rows
                .iter_mut()
                .find(|r| r.effective["id"] == id.as_str())
            {
                member_list(&mut current.effective, users, *append, *skip_org_count)?;
            }
            if let Some(group) = native.get_mut(id) {
                if *append {
                    group.users.extend(users.iter().cloned())
                } else {
                    group.users = users.clone();
                    group.members_loaded = true
                }
            }
            Ok(vec![])
        }
        Operation::Event { kind, raw, sonic } => event(saved, native, kind, raw, *sonic, lowered),
    }
}
fn metadata(
    saved: &mut Saved,
    native: &mut Native,
    origin: Origin,
    input: &[Value],
    native_members: &NativeMembers,
    lowered: &BTreeMap<String, String>,
) -> Result<Vec<Current>> {
    let before = saved.rows.clone();
    let native_before = native.clone();
    let mut projected = vec![];
    for raw in input {
        let identifier = id(raw)?;
        let previous = row(&before, identifier);
        if skip(origin, raw, previous) {
            continue;
        }
        let Some(mut effective) =
            super::effective::effective(raw, previous.map(|r| &r.effective), &|name| {
                lowered.get(name).cloned()
            })?
        else {
            continue;
        };
        if !truth(&effective["users"]) {
            effective["users"] = json!([])
        }
        strings(&effective["users"])?;
        effective
            .as_object_mut()
            .ok_or(Error::Shape)?
            .remove("hasFetchedMembers");
        // Native membership loading is an existing host projection contract,
        // independent of the canonical GQ users/fetched fields above.
        let mut group = UserGroup::from_observed(&effective).map_err(|_| Error::Shape)?;
        group.label_source = Some(raw.clone());
        let old = native_before.get(identifier);
        match native_members {
            NativeMembers::Preserve => {
                group.users = old.map(|v| v.users.clone()).unwrap_or_default();
                group.members_loaded = old.is_some_and(|v| v.members_loaded)
            }
            NativeMembers::IncomingIfPresent => {
                if let Some(users) = raw.get("users") {
                    group.users = strings(users)?;
                    group.members_loaded = true
                } else {
                    group.users = old.map(|v| v.users.clone()).unwrap_or_default();
                    group.members_loaded = old.is_some_and(|v| v.members_loaded)
                }
            }
            NativeMembers::Replace(users) => {
                group.users = users.clone();
                group.members_loaded = true
            }
        }
        projected.push((
            Current {
                source: raw.clone(),
                effective,
            },
            group,
        ));
    }
    for (current, group) in projected {
        if let Some(old) = saved
            .rows
            .iter_mut()
            .find(|r| r.effective["id"] == group.id)
        {
            *old = current
        } else {
            saved.rows.push(current)
        }
        native.insert(group.id.clone(), group);
    }
    // D8Wa reselects every original occurrence, including skipped inputs.
    Ok(input
        .iter()
        .filter_map(|raw| {
            row(&saved.rows, raw["id"].as_str()?).map(|current| Current {
                source: raw.clone(),
                effective: current.effective.clone(),
            })
        })
        .collect())
}
fn member_list(
    effective: &mut Value,
    users: &[String],
    append: bool,
    skip_org_count: bool,
) -> Result<()> {
    if append {
        if users.is_empty() {
            return Ok(());
        }
        let mut combined = if truth(&effective["users"]) {
            strings(&effective["users"])?
        } else {
            vec![]
        };
        combined.extend_from_slice(users);
        effective["users"] = json!(combined);
    } else {
        effective["users"] = json!(users);
        effective["hasFetchedMembers"] = json!(true);
        let org = effective["team_id"].as_str().is_some_and(|id| {
            id.len() >= 9
                && id.starts_with('E')
                && id[1..]
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        });
        if !(skip_org_count && org) {
            effective["user_count"] = json!(users.len())
        }
    }
    Ok(())
}
fn event(
    saved: &mut Saved,
    native: &mut Native,
    kind: &str,
    raw: &Value,
    sonic: bool,
    lowered: &BTreeMap<String, String>,
) -> Result<Vec<Current>> {
    if matches!(kind, "subteam_created" | "subteam_updated") {
        let rows = event_metadata(&saved.rows, kind, raw)?;
        let identifier = raw["subteam"]["id"].as_str().ok_or(Error::Shape)?;
        let prior = native.get(identifier).cloned();
        let result = metadata(
            saved,
            native,
            Origin::Upsert,
            &rows,
            &NativeMembers::Preserve,
            lowered,
        )?;
        if !rows.is_empty()
            && let Some(group) = native.get_mut(identifier)
        {
            if let Some(users) = raw["subteam"].get("users") {
                group.users = strings(users)?;
                group.members_loaded = true;
            } else if let Some(prior) = prior {
                group.users = prior.users;
                group.members_loaded = prior.members_loaded;
            }
        }
        return Ok(result);
    }
    if kind == "subteam_self_removed" && saved.user.is_empty() {
        return Ok(vec![]);
    }
    let mut event = raw.clone();
    event["type"] = json!(kind);
    let event = serde_json::from_value(event).map_err(|_| Error::Shape)?;
    crate::slack::models::apply_group_event(native, &saved.user, &event);
    if kind == "subteam_members_changed" && sonic {
        return Ok(vec![]);
    }
    let id = raw["subteam_id"].as_str().ok_or(Error::Shape)?;
    let mut added = raw
        .get("added_users")
        .map(strings)
        .transpose()?
        .unwrap_or_default();
    let mut removed = raw
        .get("removed_users")
        .map(strings)
        .transpose()?
        .unwrap_or_default();
    match kind {
        "subteam_self_added" => {
            saved.membership_flags.insert(id.into(), true);
            added = vec![saved.user.clone()];
            removed.clear();
            let ids = saved.membership.get_or_insert_with(Vec::new);
            if !ids.iter().any(|s| s == id) {
                ids.push(id.into())
            }
        }
        "subteam_self_removed" => {
            saved.membership_flags.insert(id.into(), false);
            added.clear();
            removed = vec![saved.user.clone()];
            if let Some(ids) = &mut saved.membership {
                ids.retain(|s| s != id)
            }
        }
        _ => {}
    }
    if let Some(current) = saved.rows.iter_mut().find(|r| r.effective["id"] == id) {
        if kind == "subteam_members_changed" && !known(&current.effective) {
            return Ok(vec![]);
        }
        let mut users = if truth(&current.effective["users"]) {
            strings(&current.effective["users"])?
        } else {
            vec![]
        };
        let original = users.clone();
        for user in &added {
            if !users.contains(user) {
                users.push(user.clone())
            }
        }
        users.retain(|u| !removed.contains(u));
        if kind == "subteam_members_changed" || original != users {
            current.effective["users"] = json!(users);
            if kind == "subteam_members_changed" {
                current.effective["user_count"] = json!(users.len())
            }
            if kind != "subteam_self_added" {
                current
                    .effective
                    .as_object_mut()
                    .ok_or(Error::Shape)?
                    .remove("hasFetchedMembers");
            }
        }
    }
    Ok(vec![])
}
