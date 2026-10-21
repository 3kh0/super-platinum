use super::*;

pub(super) fn opening(state: Signal<ShellState>) -> Value {
    recovery::snapshot(&state.peek())["mixedFile"]["opening"].clone()
}
pub(super) fn same_opening(state: Signal<ShellState>, original: &Value) -> Result<(), String> {
    let current = opening(state);
    if current != *original || current["mounted"] != false || current["reading"] != true {
        return Err(format!(
            "Original mixed opening changed: {original}; {current}"
        ));
    }
    retained(state)
}
pub(super) fn absent(state: Signal<ShellState>) -> Result<(), String> {
    let model = recovery::snapshot(&state.peek());
    let value = &model["mixedAbsentMember"];
    if value["member"]["sourceObserved"] != true
        || value["member"]["sourcePresent"] != false
        || !value["member"]["native"].is_null()
        || !value["observation"].is_null()
    {
        return Err(format!("Mixed member is not truly absent: {value}"));
    }
    retained(state)
}
pub(super) fn canonical_member(state: Signal<ShellState>, response: &Value) -> Result<(), String> {
    let model = recovery::snapshot(&state.peek());
    let value = &model["mixedAbsentMember"];
    // Read projects actual can_interact, then the private hydration step adds
    // derived source fields. Keep the original response separate from this
    // explicitly expected canonical source; the visible label is not deburred.
    let mut expected = response["results"][0].clone();
    expected["is_stranger"] = json!(false);
    for (key, value) in [
        ("first_name", ""),
        ("last_name", ""),
        ("display_name_normalized", "  Cafe Harbor  "),
        ("real_name_normalized", "Profile fallback"),
    ] {
        expected["profile"][key] = json!(value);
    }
    for (key, value) in [
        ("_name_lc", "username-must-not-win"),
        ("_first_name_lc", ""),
        ("_last_name_lc", ""),
        ("_real_name_lc", "profile fallback"),
        ("_display_name_lc", "  cafe\u{0301} harbor  "),
        ("_real_name_normalized_lc", "profile fallback"),
        ("_display_name_normalized_lc", "  cafe harbor  "),
        ("_email_normalized_lc", ""),
    ] {
        expected[key] = json!(value);
    }
    if value["member"]["sourcePresent"] != true
        || value["observation"]["current"] != true
        || value["observation"]["source"] != expected
        || value["observation"]["native"] != value["member"]["native"]
        || value["member"]["native"]["id"] != member::MEMBER
        || value["member"]["native"]["profile"]["display_name"] != member::LABEL
    {
        return Err(format!(
            "Mixed member original source/private adoption changed: {value}"
        ));
    }
    Ok(())
}
pub(super) fn no_groups(state: Signal<ShellState>) -> Result<(), String> {
    let model = recovery::snapshot(&state.peek());
    if !recovery::requests().is_empty()
        || recovery::ids()
            .iter()
            .any(|id| !model["groups"][id]["native"].is_null())
    {
        return Err(format!(
            "Group admission preceded original member adoption: {model}"
        ));
    }
    Ok(())
}
pub(super) fn release_file() -> Result<Value, String> {
    let release = files::release();
    if release != json!({"delivered":true,"cancelledBeforeReply":false}) {
        return Err(format!(
            "Original mixed file response was not delivered: {release}"
        ));
    }
    Ok(release)
}
pub(super) fn require_delivery(release: &Value) -> Result<(), String> {
    if release["delivered"] != true || release["receiverRetired"] != false {
        return Err(format!(
            "Account-owned member response delivery changed: {release}"
        ));
    }
    Ok(())
}
pub(super) fn member_finished(state: Signal<ShellState>) -> u64 {
    member::snapshot(&state.peek())["driver"]["finished"]
        .as_u64()
        .unwrap_or(0)
}
pub(super) async fn member_settled(state: Signal<ShellState>, before: u64) -> Result<(), String> {
    for _ in 0..100 {
        let model = member::snapshot(&state.peek());
        if model["driver"]["finished"]
            .as_u64()
            .is_some_and(|v| v > before)
            && model["driver"]["active"] == 0
            && member::pending().is_empty()
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Mixed shared member task did not settle: {}",
        member::snapshot(&state.peek())
    ))
}
pub(super) async fn member_request(state: Signal<ShellState>) -> Result<u64, String> {
    for _ in 0..100 {
        let pending = member::pending();
        if pending.len() > 1 {
            return Err(format!("Duplicate mixed member acquisition: {pending:?}"));
        }
        if let Some(row) = pending.first() {
            let serial = row["serial"]
                .as_u64()
                .ok_or("Missing mixed member serial")?;
            if row["kind"] == "discovery" {
                let release = member::release_observed(
                    serial,
                    Ok((
                        json!({"ok":true,
                    "url":format!("https://edgeapi.slack.com/cache/{}",row["team"].as_str().unwrap()),
                    "ttl_seconds":300}),
                        None,
                    )),
                )?;
                require_delivery(&release)?;
            } else {
                if row["kind"] != "member"
                    || row["body"]
                        != json!({
                    "updated_ids":{(member::MEMBER):0},"check_interaction":true,
                    "include_profile_only_users":true})
                {
                    return Err(format!("Wrong original mixed member request: {row}"));
                }
                return Ok(serial);
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Mixed held member did not arrive: {}",
        member::snapshot(&state.peek())
    ))
}
pub(super) async fn quiet(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    loading: Option<bool>,
) -> Result<(), String> {
    let before = recovery::snapshot(&state.peek());
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(loading) = loading {
            message(desktop, loading).await?;
        } else {
            form(desktop).await?;
        }
        retained(state)?;
        let after = recovery::snapshot(&state.peek());
        if before != after {
            return Err(format!(
                "Mixed quiet observation changed: before={before}; after={after}"
            ));
        }
    }
    Ok(())
}
pub(super) async fn shot(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    capture(state, desktop, name, stage, proof, stages, captures).await?;
    let geometry = &stages.last().ok_or("Missing mixed absent stage")?["geometry"];
    if geometry["richEditable"] == true {
        let tokens = geometry["tokens"]
            .as_array()
            .ok_or("Missing actual mixed tokens")?;
        if tokens.len() != 7
            || [3, 5]
                .iter()
                .any(|i| tokens[*i]["fontWeight"] != "700" || tokens[*i]["fontStyle"] != "normal")
            || [4, 6]
                .iter()
                .any(|i| tokens[*i]["fontWeight"] != "400" || tokens[*i]["fontStyle"] != "italic")
        {
            return Err(format!(
                "Original mixed member/file styles changed: {tokens:?}"
            ));
        }
    }
    Ok(())
}
