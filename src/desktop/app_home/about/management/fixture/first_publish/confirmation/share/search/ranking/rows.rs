use super::*;

fn person(n: usize, remote: bool) -> Value {
    let mut row = super::super::super::super::super::picker::person(
        &format!("URANK{}{n:03}", if remote { "R" } else { "L" }),
        &format!("Review {} {n:02}", if remote { "Remote" } else { "Local" }),
    );
    row["is_app_user"] = json!(false);
    row["is_workflow_bot"] = json!(false);
    row["is_profile_only"] = json!(false);
    row["ranking_fixture_opaque"] = json!({"family":"member","remote":remote,"ordinal":n});
    row
}
pub(super) fn local_people() -> Vec<Value> {
    (0..42).map(|n| person(n, false)).collect()
}
pub(crate) fn seed(users: &mut Vec<Value>) {
    users.extend(local_people());
}
fn channel(n: usize) -> Value {
    let mut row = raw::channel(raw::DEST).unwrap();
    row["id"] = json!(format!("CRANKR{n:03}"));
    row["name"] = json!(format!("review-remote-{n:02}"));
    // Preserve source membership companion separately from the original row.
    row["is_member"] = json!(false);
    row["ranking_fixture_opaque"] = json!({"family":"channel","ordinal":n});
    row
}
pub(super) fn destination_channel() -> Value {
    posting_channel(0)
}
pub(super) fn posting_channel(n: usize) -> Value {
    let mut row = channel(n);
    row["is_member"] = json!(true);
    row
}
pub(crate) fn search_reply(
    kind: wire::Kind,
    query: &str,
    marker: Option<&str>,
) -> Result<(Value, Option<Vec<String>>), Error> {
    if !matches!(query, "review" | "Review" | "oldreview" | "newreview")
        || marker.is_some_and(|m| m != "ranking-page-2")
    {
        return Err(closed());
    }
    let (start, end) = if marker.is_none() { (0, 13) } else { (13, 25) };
    let mut values = serde_json::Map::new();
    let mut keys = vec![];
    let mut members = vec![];
    for n in start..end {
        let value = match kind {
            wire::Kind::Members => person(n, true),
            wire::Kind::Channels => channel(n),
        };
        if kind == wire::Kind::Channels {
            members.push(value["id"].clone());
        }
        let key = format!("z{:02}", 99 - n);
        keys.push(key.clone());
        values.insert(key, value);
    }
    let mut raw = json!({"ok":true,"results":values,"next_marker":if marker.is_none(){"ranking-page-2"}else{""},
        "opaque_receipt":{"query":query,"family":kind.method(),"page":if marker.is_none(){1}else{2}}});
    if kind == wire::Kind::Channels {
        raw["member_channels"] = json!(members);
    }
    Ok((raw, Some(keys)))
}
