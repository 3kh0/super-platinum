use super::*;

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let ready = readiness(name).await?;
    let case = recovery::case();
    if !matches!(case.as_str(), "legacy" | "absent" | "retirement") {
        return Err("Unknown group recovery case".into());
    }
    recovery::seed(state).await?;
    let original = recovery::snapshot(&state.peek());
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    form(desktop).await?;
    retained(state)?;
    capture(
        state,
        desktop,
        name,
        "initial",
        json!({"originalDraft":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Edit description", false).await?;
    unavailable(desktop).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    if !recovery::requests().is_empty() {
        return Err("Ordinary opening forced a group read".into());
    }
    capture(
        state,
        desktop,
        name,
        "ordinary-open",
        json!({"noForcedRead":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Retry description", true).await?;
    let first = request(state, "directory", &recovery::ids()).await?;
    loading(desktop).await?;
    capture(
        state,
        desktop,
        name,
        "held",
        json!({"explicitRetryRead":true,"duplicateIdsCoalesced":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if case == "retirement" {
        click(desktop, "Cancel description edit", false).await?;
        form(desktop).await?;
        let before = finished(state);
        recovery::release(first, Ok(directory(&recovery::ids())))?;
        settled(state, before).await?;
        retained(state)?;
        form(desktop).await?;
        if recovery::snapshot(&state.peek())["groups"] != original["groups"] {
            return Err("Cancelled Retry adopted groups".into());
        }
        capture(
            state,
            desktop,
            name,
            "cancelled",
            json!({"lateResultDidNotOpen":true,"draftRetained":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        click(desktop, "Edit description", false).await?;
        unavailable(desktop).await?;
        click(desktop, "Retry description", false).await?;
        let stale = request(state, "directory", &recovery::ids()).await?;
        let mut changed = recovery::raw(recovery::GROUP);
        changed["handle"] = json!("Socket crew");
        recovery::event(&mut state.write(), changed.clone())?;
        let before = finished(state);
        recovery::release(stale, Ok(directory(&recovery::ids())))?;
        settled(state, before).await?;
        retained(state)?;
        unavailable(desktop).await?;
        let model = recovery::snapshot(&state.peek());
        if model["groups"][recovery::GROUP]["source"] != changed
            || model["groups"][recovery::OTHER]["native"] != Value::Null
        {
            return Err(format!("Original source witness was not refused: {model}"));
        }
        capture(
            state,
            desktop,
            name,
            "source-refused",
            json!({"actualGroupEvent":true,"oldSourceResultRefused":true,"draftRetained":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        click(desktop, "Retry description", false).await?;
        let account = request(state, "directory", &[recovery::OTHER.into()]).await?;
        let proof = recovery::replace_account(state);
        let before = finished(state);
        recovery::release(account, Ok(directory(&[recovery::OTHER.into()])))?;
        settled(state, before).await?;
        wait_dom(desktop, "({ok:!document.querySelector('.profile-edit')})").await?;
        let model = recovery::snapshot(&state.peek());
        if model["profile"]["enabled"] != false
            || recovery::ids()
                .iter()
                .any(|id| model["groups"][id]["native"] != Value::Null)
        {
            return Err(format!("Retired account adopted groups: {model}"));
        }
        capture(
            state,
            desktop,
            name,
            "account-replaced",
            proof,
            &mut stages,
            &mut captures,
        )
        .await?;
    } else {
        if case == "legacy" {
            let before = finished(state);
            recovery::release(first, Ok(json!({"ok":true,"results":"malformed"})))?;
            let fallback = request(state, "direct", &recovery::ids()).await?;
            recovery::release(fallback, Ok(json!({"ok":true,"subteams":"malformed"})))?;
            settled(state, before).await?;
            unchanged(state, &original)?;
            unavailable(desktop).await?;
            capture(
                state,
                desktop,
                name,
                "malformed",
                json!({"sourceRetained":true,"draftRetained":true,"actualFallback":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
            click(desktop, "Retry description", false).await?;
            let offline = request(state, "directory", &recovery::ids()).await?;
            let before = finished(state);
            recovery::release(
                offline,
                Err(Error::Offline("closed group fixture offline".into())),
            )?;
            let fallback = request(state, "direct", &recovery::ids()).await?;
            recovery::release(
                fallback,
                Err(Error::Offline("closed group direct offline".into())),
            )?;
            settled(state, before).await?;
            unchanged(state, &original)?;
            unavailable(desktop).await?;
            capture(
                state,
                desktop,
                name,
                "offline",
                json!({"sourceRetained":true,"draftRetained":true,"actualFallback":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
            click(desktop, "Retry description", false).await?;
            let good = request(state, "directory", &recovery::ids()).await?;
            let before = finished(state);
            recovery::release(good, Ok(directory(&recovery::ids())))?;
            settled(state, before).await?;
        } else {
            let before = finished(state);
            recovery::release(
                first,
                Ok(
                    json!({"ok":true,"results":[recovery::raw(recovery::GROUP)],"pending_ids":[recovery::OTHER]}),
                ),
            )?;
            settled(state, before).await?;
            retained(state)?;
            unavailable(desktop).await?;
            let partial = recovery::snapshot(&state.peek());
            if partial["groups"][recovery::GROUP]["source"] != recovery::raw(recovery::GROUP)
                || partial["groups"][recovery::OTHER]["native"] != Value::Null
            {
                return Err(format!("Wrong partial unresolved groups: {partial}"));
            }
            capture(
                state,
                desktop,
                name,
                "partial",
                json!({"actualPartialRetained":true,"noPlaceholder":true,"draftRetained":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
            click(desktop, "Retry description", false).await?;
            let remaining = vec![recovery::OTHER.into()];
            let failed = request(state, "directory", &remaining).await?;
            let before = finished(state);
            recovery::release(
                failed,
                Ok(json!({"ok":true,"results":[],"failed_ids":[recovery::OTHER]})),
            )?;
            let direct = request(state, "direct", &remaining).await?;
            loading(desktop).await?;
            capture(
                state,
                desktop,
                name,
                "fallback-held",
                json!({"onlyUnresolvedRequested":true,"failedIdsFallback":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
            recovery::release(
                direct,
                Ok(json!({"ok":true,"subteams":[recovery::raw(recovery::OTHER)]})),
            )?;
            settled(state, before).await?;
        }
        retained(state)?;
        editor(desktop).await?;
        let model = recovery::snapshot(&state.peek());
        for id in recovery::ids() {
            if model["groups"][&id]["source"] != recovery::raw(&id) {
                return Err(format!("Missing raw adopted group {id}: {model}"));
            }
        }
        if case == "legacy"
            && (model["groups"][recovery::GROUP]["native"]["users"]
                != original["groups"][recovery::GROUP]["native"]["users"]
                || model["groups"][recovery::GROUP]["native"]["members_loaded"] != true)
        {
            return Err("Group read erased membership".into());
        }
        capture(
            state,
            desktop,
            name,
            "success",
            json!({"actualReadAndAdoption":true,"originalDraft":true,"membershipPreserved":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        click(desktop, "Use description", false).await?;
        form(desktop).await?;
        retained(state)?;
        let count = recovery::requests().len();
        save(state, desktop).await?;
        capture(
            state,
            desktop,
            name,
            "saved",
            json!({"actualProfileSaveReadback":true,"rawDuplicatesPreserved":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        click(desktop, "Edit description", false).await?;
        editor(desktop).await?;
        if recovery::requests().len() != count {
            return Err("Cache-only reopen issued group request".into());
        }
        capture(
            state,
            desktop,
            name,
            "reopened",
            json!({"noRedundantRead":true,"originalDraft":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":case,"readiness":ready,"viewport":stages[0]["geometry"]["viewport"],
        "stages":stages,"captures":captures,"requests":recovery::requests(),"captureChecksPassed":true,"visualReview":"required"}),
    )
}
fn retained(state: Signal<ShellState>) -> Result<(), String> {
    let model = recovery::snapshot(&state.peek());
    if model["profile"]["blocks"] != json!(recovery::blocks(recovery::DRAFT))
        || model["profile"]["attempts"] != json!([])
        || model["profile"]["reads"] != 0
    {
        return Err(format!("Group recovery lost original draft: {model}"));
    }
    Ok(())
}
fn unchanged(state: Signal<ShellState>, original: &Value) -> Result<(), String> {
    retained(state)?;
    let model = recovery::snapshot(&state.peek());
    if model["groups"] != original["groups"] {
        return Err(format!("Failed group read changed metadata: {model}"));
    }
    Ok(())
}
fn directory(ids: &[String]) -> Value {
    json!({"ok":true,"results":ids.iter().map(|id|recovery::raw(id)).collect::<Vec<_>>()})
}
fn finished(state: Signal<ShellState>) -> u64 {
    recovery::snapshot(&state.peek())["driver"]["finished"]
        .as_u64()
        .unwrap_or(0)
}
async fn settled(state: Signal<ShellState>, before: u64) -> Result<(), String> {
    for _ in 0..100 {
        let s = recovery::snapshot(&state.peek());
        if s["driver"]["finished"].as_u64().is_some_and(|n| n > before)
            && s["driver"]["active"] == 0
            && recovery::pending().is_empty()
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "Group settlement timed out: {}",
        recovery::snapshot(&state.peek())
    ))
}
async fn loading(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:!document.getElementById('profile-section-rich')&&document.getElementById('profile-section-rich-status')?.textContent==='Loading mention names. Your description is unchanged.'&&[...document.querySelectorAll('.profile-edit button')].some(b=>b.textContent==='Retry description'&&b.disabled)})"#).await.map(|_|())
}
async fn request(state: Signal<ShellState>, kind: &str, ids: &[String]) -> Result<u64, String> {
    for _ in 0..100 {
        let rows = recovery::pending();
        if rows.len() > 1 {
            return Err(format!("Unexpected parallel group request: {rows:?}"));
        }
        if let Some(row) = rows.first() {
            let serial = row["serial"]
                .as_u64()
                .ok_or("Missing group request serial")?;
            if row["kind"] == "discovery" {
                recovery::release(
                    serial,
                    Ok(
                        json!({"ok":true,"url":format!("https://edgeapi.slack.com/cache/{}",row["team"].as_str().unwrap()),"ttl_seconds":300}),
                    ),
                )?;
            } else {
                if row["kind"] != kind {
                    return Err(format!("Expected {kind}, actual {row}"));
                }
                if kind == "directory" && row["body"] != json!({"ids":ids}) {
                    return Err(format!("Wrong group directory body: {row}"));
                }
                if kind == "direct" {
                    let body = &row["body"];
                    if body["usergroups"] != json!(ids.join(","))
                        || body["include_users"] != "false"
                        || body["include_count"] != "true"
                        || body["include_disabled"] != "true"
                        || body["_x_reason"] != "fallback:fetchRawUserGroupsById"
                    {
                        return Err(format!("Wrong direct group body: {row}"));
                    }
                }
                return Ok(serial);
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "Held group {kind} timed out: {}",
        recovery::snapshot(&state.peek())
    ))
}
async fn save(mut state: Signal<ShellState>, desktop: &DesktopContext) -> Result<(), String> {
    state
        .write()
        .profile_edit
        .sections
        .fixture
        .as_mut()
        .unwrap()
        .outcome = Outcome::Saved;
    click(desktop, "Save section", true).await?;
    let mut pending = false;
    for _ in 0..100 {
        let model = mentions::snapshot(&state.peek());
        if model["attempts"].as_array().is_some_and(|v| v.len() == 1) && model["busy"] == true {
            pending = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    if !pending {
        return Err("Profile group save did not enter held write".into());
    }
    complete(&mut state.write());
    for _ in 0..100 {
        let model = mentions::snapshot(&state.peek());
        if model["reads"] == 1 && model["busy"] == false && model["dirty"] == false {
            if model["blocks"] != json!(recovery::blocks(recovery::DRAFT)) {
                return Err("Save/readback changed group tokens".into());
            }
            let view = presentation(&state.peek()).ok_or("Missing saved section")?;
            let expected = json!({"visit":view.visit.to_string(),"serial":view.serial.to_string()});
            wait_dom(desktop,&format!(r#"(()=>{{const d=document.getElementById('shell-overlay-dialog'),e={expected};return{{ok:!!d&&d.dataset.dialogGeneration===e.visit&&d.dataset.sectionSerial===e.serial&&d.dataset.sectionLocked==='false'&&document.querySelector('.profile-edit-notice')?.textContent==='Profile fields saved.'}};}})()"#)).await?;
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    Err(format!(
        "Profile group readback timed out: {}",
        mentions::snapshot(&state.peek())
    ))
}
