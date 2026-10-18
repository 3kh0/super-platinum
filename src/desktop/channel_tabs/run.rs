use super::*;
use serde_json::Value;
use std::time::Duration;
use super_platinum_core::{
    channel_tabs::Observation,
    slack::{Error, api},
};

pub(super) async fn read(owner: &Owner) -> Result<(Value, Inventory), Error> {
    let raw = owner
        .execute(api::conversations_info(
            &owner.client,
            &owner.session,
            owner.key.channel.clone(),
        ))
        .await?;
    envelope(&raw)?;
    let channel = raw
        .get("channel")
        .ok_or_else(|| Error::Api("channel_missing".into()))?;
    let inventory =
        Inventory::from_channel(channel).map_err(|_| Error::Api("tabs_unavailable".into()))?;
    if inventory.channel() != owner.key.channel {
        return Err(Error::Api("channel_changed".into()));
    }
    Ok((channel.clone(), inventory))
}
pub(super) fn envelope(raw: &Value) -> Result<(), Error> {
    if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
        Err(Error::Api("tabs_response_unconfirmed".into()))
    } else {
        Ok(())
    }
}
pub(super) async fn execute(
    mut state: Signal<ShellState>,
    owner: Owner,
    serial: u64,
    intent: Option<Intent>,
) {
    if !owner.current(&state.peek()) {
        finish(
            state,
            &owner,
            serial,
            "The conversation changed. Refresh its tabs.",
        );
        return;
    }
    let (raw, current) = match read(&owner).await {
        Ok(v) => v,
        Err(e) => {
            failure(state, &owner, serial, &e, false);
            return;
        }
    };
    if !owner.current(&state.peek()) {
        finish(
            state,
            &owner,
            serial,
            "The conversation changed. Refresh its tabs.",
        );
        return;
    }
    let permission = match owner.read_permission(state, &raw).await {
        Ok(permission) => permission,
        Err(e) => {
            failure(state, &owner, serial, &e, false);
            return;
        }
    };
    if let Some(mut intent) = intent {
        if permission != Permission::Allowed || intent.revalidate(&current).is_err() {
            if let Some(e) = current_entry(&mut state.write(), &owner, serial) {
                e.inventory = Some(current);
                e.permission = Some(permission);
            }
            finish(
                state,
                &owner,
                serial,
                "Tabs or permissions changed. Review the current tabs before trying again.",
            );
            return;
        }
        if let Action::AddCanvas { file, access, .. } = intent.action() {
            // Read-only sharing is the currently exposed add operation. Write
            // permission needs the complete file-role selector before UI exposure.
            if *access != super_platinum_core::channel_tabs::Access::Read {
                finish(
                    state,
                    &owner,
                    serial,
                    "That sharing permission is unavailable.",
                );
                return;
            }
            let result = owner
                .execute(api::file_info(&owner.client, &owner.session, file))
                .await
                .and_then(|v| {
                    envelope(&v)?;
                    let f: super_platinum_core::slack::models::File =
                        serde_json::from_value(v["file"].clone())
                            .map_err(|_| Error::Api("file_missing".into()))?;
                    if f.id.as_deref() != Some(file.as_str()) {
                        return Err(Error::Api("file_changed".into()));
                    }
                    Ok(f)
                });
            if !owner.current(&state.peek()) {
                finish(
                    state,
                    &owner,
                    serial,
                    "The conversation changed. Nothing was sent.",
                );
                return;
            }
            match result {
                Ok(f)
                    if matches!(f.filetype.as_deref(), Some("quip" | "canvas"))
                        && !f.extra.get("is_tombstoned").is_some_and(|v| v == true) => {}
                Err(e) => {
                    failure(state, &owner, serial, &e, false);
                    return;
                }
                _ => {
                    finish(state, &owner, serial, "Choose an available Canvas.");
                    return;
                }
            }
            // Actual client read contract; positive server eligibility, never a
            // fabricated local grant. Always checking is a conservative extra read.
            let Ok(request) = super_platinum_core::channel_tabs::can_share_request(
                &owner.client,
                &owner.session,
                &owner.key.channel,
                file,
            ) else {
                finish(
                    state,
                    &owner,
                    serial,
                    "The Canvas destination is unavailable.",
                );
                return;
            };
            let result = owner.execute(request).await;
            if !owner.current(&state.peek()) {
                finish(
                    state,
                    &owner,
                    serial,
                    "The conversation changed. Nothing was sent.",
                );
                return;
            }
            match result {
                Ok(v) if envelope(&v).is_ok() && v["can_share"] == true => {}
                Err(e) => {
                    failure(state, &owner, serial, &e, false);
                    return;
                }
                _ => {
                    finish(state, &owner, serial, "This Canvas cannot be shared here.");
                    return;
                }
            }
            // File checks may take time; reread membership/order/permissions
            // before the mutation rather than trusting the initial channel read.
            let (fresh, new) = match read(&owner).await {
                Ok(v) => v,
                Err(e) => {
                    failure(state, &owner, serial, &e, false);
                    return;
                }
            };
            let fresh_permission = match owner.read_permission(state, &fresh).await {
                Ok(permission) => permission,
                Err(e) => {
                    failure(state, &owner, serial, &e, false);
                    return;
                }
            };
            if !owner.current(&state.peek())
                || intent.revalidate(&new).is_err()
                || fresh_permission != Permission::Allowed
            {
                finish(
                    state,
                    &owner,
                    serial,
                    "Tabs or permissions changed. Nothing was sent.",
                );
                return;
            }
        }
        if let Action::Remove { id } = intent.action() {
            // oatk also recognizes file-only knowledge metadata. A missing
            // channel flag must not send the ordinary one-service removal.
            let file = intent
                .base()
                .canvas_record(id)
                .ok()
                .and_then(super_platinum_core::channel_tabs::file_id)
                .unwrap_or_default();
            let response = owner
                .execute(api::file_info(&owner.client, &owner.session, file))
                .await;
            if !owner.current(&state.peek()) {
                finish(
                    state,
                    &owner,
                    serial,
                    "The conversation changed. Nothing was sent.",
                );
                return;
            }
            let raw = match response {
                Ok(v) if envelope(&v).is_ok() && v["file"]["id"].as_str() == Some(file) => v,
                Err(e) => {
                    failure(state, &owner, serial, &e, false);
                    return;
                }
                _ => {
                    finish(
                        state,
                        &owner,
                        serial,
                        "Current Canvas information is required. Nothing was sent.",
                    );
                    return;
                }
            };
            if raw["file"]["knowledge_canvas_channel_id"].as_str()
                == Some(owner.key.channel.as_str())
                && !(current
                    .properties()
                    .pointer("/channel_knowledge_canvas/file_id")
                    .and_then(Value::as_str)
                    == Some(file)
                    && current
                        .properties()
                        .pointer("/channel_knowledge_canvas/is_enabled")
                        == Some(&Value::Bool(false)))
            {
                finish(
                    state,
                    &owner,
                    serial,
                    "This is a Channel Knowledge Canvas. Refresh its channel metadata before removing it.",
                );
                return;
            }
            let (fresh, new) = match read(&owner).await {
                Ok(v) => v,
                Err(e) => {
                    failure(state, &owner, serial, &e, false);
                    return;
                }
            };
            if !owner.current(&state.peek()) || intent.revalidate(&new).is_err() {
                finish(state, &owner, serial, "Tabs changed. Nothing was sent.");
                return;
            }
            match owner.read_permission(state, &fresh).await {
                Ok(Permission::Allowed) => {}
                Err(e) => {
                    failure(state, &owner, serial, &e, false);
                    return;
                }
                _ => {
                    finish(
                        state,
                        &owner,
                        serial,
                        "Permission changed. Nothing was sent.",
                    );
                    return;
                }
            }
        }
        // Publish immutable custody before handing anything to Transport. There
        // is no mutation retry path, including generic Offline after this point.
        let request = intent.request(&owner.client, &owner.session);
        {
            let mut shell = state.write();
            if !owner.current(&shell) || !selection::can_mutate(&shell, intent.action()) {
                drop(shell);
                finish(
                    state,
                    &owner,
                    serial,
                    "The conversation or Canvas edit changed. Nothing was sent.",
                );
                return;
            }
            let Some(e) = current_entry(&mut shell, &owner, serial) else {
                return;
            };
            e.pending = Some(intent.clone());
        }
        match owner.execute(request).await {
            Ok(v) if envelope(&v).is_ok() => {
                if intent.acknowledge(&v).is_err() {
                    finish(
                        state,
                        &owner,
                        serial,
                        "The folder response could not be confirmed. Review current tabs; creation will not be sent again.",
                    );
                    return;
                }
                // ACK custody belongs to the original operation even when its
                // account or dialog is no longer the visible destination.
                if let Some(e) = current_entry(&mut state.write(), &owner, serial) {
                    e.pending = Some(intent.clone());
                }
            }
            Ok(_) => {
                finish(
                    state,
                    &owner,
                    serial,
                    "The tab change could not be confirmed. Review current tabs; it will not be sent again.",
                );
                return;
            }
            Err(e) => {
                failure(state, &owner, serial, &e, true);
                return;
            }
        }
        if !owner.account_current(&state.peek()) {
            finish(
                state,
                &owner,
                serial,
                "The response is retained. Return to this account to review current tabs.",
            );
            return;
        }
        match read(&owner).await {
            // Post-dispatch settlement is account-owned, even after UI retirement.
            // A manager proof is not required to confirm the exact outcome;
            // its next management action requires another fresh permission read.
            Ok((raw, current)) => {
                adopt(state, &owner, serial, owner.permission(&raw), current, true)
            }
            Err(e) => failure(state, &owner, serial, &e, true),
        }
    } else {
        adopt(state, &owner, serial, permission, current, false);
    }
}
fn adopt(
    mut state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
    permission: Permission,
    current: Inventory,
    just_dispatched: bool,
) {
    if !owner.account_current(&state.peek()) {
        finish(
            state,
            owner,
            serial,
            "Return to the original account to review tabs.",
        );
        return;
    }
    let mut shell = state.write();
    let originating_visit_current = owner.current(&shell);
    let Some(e) = current_entry(&mut shell, owner, serial) else {
        return;
    };
    e.busy = false;
    e.permission = Some(permission);
    let mut added = None;
    if let Some(pending) = e.pending.as_ref() {
        if pending.observe(&current) == Observation::Satisfied {
            added = selection::confirmed_added(
                pending,
                &current,
                just_dispatched,
                originating_visit_current,
            );
            e.pending = None;
            e.notice = "The tab change is confirmed.".into();
        } else {
            e.notice="The outcome is still unconfirmed. Current tabs are shown; the change will not be sent again.".into();
        }
    } else {
        e.notice.clear();
    }
    e.inventory = Some(current.clone());
    drop(shell);
    if let Some(id) = added {
        super::open_canvas(state, &Visit(Rc::new(owner.clone())), current, id);
    }
}
pub(super) fn finish(mut state: Signal<ShellState>, owner: &Owner, serial: u64, message: &str) {
    if let Some(e) = current_entry(&mut state.write(), owner, serial) {
        e.busy = false;
        e.notice = message.into();
    }
}
pub(super) fn failure(
    mut state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
    error: &Error,
    sent: bool,
) {
    let wait = match error {
        Error::RateLimited { retry_after_secs } => {
            Some(Duration::from_secs(retry_after_secs.unwrap_or(30).max(1)))
        }
        _ => None,
    };
    if let Some(e) = current_entry(&mut state.write(), owner, serial) {
        e.busy = false;
        e.cooling = wait.is_some();
        e.retry_at = wait.and_then(|d| Instant::now().checked_add(d));
        e.notice=if wait.is_some(){"Slack asked us to wait. Refresh becomes available after the wait; no change is retried."}
            else if sent{"The tab change is unconfirmed. Review current tabs; it will not be sent again."}
            else{"Tabs could not load. Nothing was sent. Refresh when connected."}.into();
    }
    if let Some(wait) = wait.filter(|d| Instant::now().checked_add(*d).is_some()) {
        let key = owner.key.clone();
        dioxus::core::spawn_forever(async move {
            tokio::time::sleep(wait).await;
            if let Some(e) = state
                .write()
                .channel_tabs
                .entries
                .get_mut(&key)
                .filter(|e| e.serial == serial)
            {
                e.retry_at = None;
                e.cooling = false;
            }
        });
    }
    if owner.account_current(&state.peek()) {
        state
            .write()
            .report_failure(error, "Tabs will be available when connected.", || {
                "The tab request could not finish.".into()
            });
    }
}
