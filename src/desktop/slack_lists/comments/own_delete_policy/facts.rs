use super::*;
pub(super) fn flag(raw: &Value, key: &str) -> Result<bool, Error> {
    // Absence in an actually acquired object follows the source's falsey
    // selector. A missing object or malformed present field is not evidence.
    if !raw.is_object() {
        return Err(missing("source metadata"));
    }
    match raw.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(v)) => Ok(*v),
        _ => Err(missing("valid source metadata")),
    }
}
pub(super) fn channel(raw: &Value, id: &str) -> Result<Value, Error> {
    if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
        return Err(missing("channel response"));
    }
    let rows = raw["channels"]
        .as_array()
        .ok_or_else(|| missing("channel response"))?;
    if rows.len() != 1
        || rows[0]["id"] != id
        || flag(&rows[0], "isUnknown")?
        || flag(&rows[0], "isNonExistent")?
        || flag(&rows[0], "is_im")?
        || flag(&rows[0], "is_mpim")?
        || !flag(&rows[0], "is_file")?
    {
        return Err(missing("ordinary List channel"));
    }
    Ok(rows[0].clone())
}
pub(super) fn root(raw: &Value, target: &Target) -> Result<(), Error> {
    let value = &raw["root"];
    if raw["channelId"] != target.channel_id()
        || raw["rootTs"].as_str() != target.root_ts()
        || value["ts"].as_str() != target.root_ts()
        || !value.is_object()
        || value["slack_list"]["list_id"] != target.list_id()
        || value["slack_list"]["list_record_id"] != target.record_id()
        || flag(value, "is_locked")?
        || flag(value, "isUnknown")?
        || flag(value, "isNonExistent")?
        || flag(value, "is_deleted")?
        || flag(value, "is_tombstoned")?
    {
        return Err(missing("unlocked original List thread root"));
    }
    Ok(())
}
pub(super) fn read_only_file_thread(channel: &Value, target: &Target) -> Result<bool, Error> {
    let file = target.document_raw();
    if !flag(channel, "is_file")?
        || channel["id"] != target.channel_id()
        || file["id"] != target.list_id()
        || target.channel_id()
            != format!(
                "C{}",
                target
                    .list_id()
                    .strip_prefix('F')
                    .ok_or_else(|| missing("virtual List identity"))?
            )
    {
        return Err(missing("original List file"));
    }
    if file["filetype"] == "thread_share"
        || file["mimetype"] == "application/vnd.slack-thread-share"
    {
        return Ok(true);
    }
    // pS2E.hG's actual List branch is sufficient independent of access and
    // file lock. Other file kinds need separate comment/write observations.
    if (file["mode"] == "list" || file["filetype"] == "list") && file["subtype"] != "poll" {
        return Ok(false);
    }
    Err(missing("ordinary List file type"))
}
