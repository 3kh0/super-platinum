//! Actual UI handlers and independently held metadata/preference responses.
use super::*;
pub(super) async fn fields(
    proof: &mut Proof,
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let before = proof.stable(state, "local")?;
    let open = click(d, "slack-list-fields").await?;
    let first = serial(state)?;
    let first_binding = bind(state, d, "fields").await?;
    let cancel_choice = checked(
        d,
        "window.__listRefinementSave.choose('#slack-list-field-choice','C_NUMBER')",
    )
    .await?;
    let cancel_hidden = checked(
        d,
        "window.__listRefinementSave.choose('#slack-list-field-visible','hidden')",
    )
    .await?;
    wait_state(state, "discardable Fields draft", |s, _| {
        s["visibility"]["column"] == "C_NUMBER" && s["visibility"]["desired"] == false
    })
    .await?;
    let cancel = click(d, "slack-list-rename-close").await?;
    let cancel_return = wait_dom(
        d,
        "window.__listRefinementSave.returned('#slack-list-fields')",
    )
    .await?;
    if proof.stable(state, "local")? != before {
        return Err("Cancel changed local state or dispatched a request".into());
    }
    let reopen = click(d, "slack-list-fields").await?;
    let second_binding = bind(state, d, "fields").await?;
    let current = serial(state)?;
    if current == first {
        return Err("Reopened Fields retained old serial".into());
    }
    let choose = checked(
        d,
        "window.__listRefinementSave.choose('#slack-list-field-choice','C_NUMBER')",
    )
    .await?;
    let hidden = checked(
        d,
        "window.__listRefinementSave.choose('#slack-list-field-visible','hidden')",
    )
    .await?;
    wait_state(state, "Fields desired visibility", |s, _| {
        s["visibility"]["column"] == "C_NUMBER" && s["visibility"]["desired"] == false
    })
    .await?;
    let stale = adapter::refuse_old_confirmation(&mut state.write(), first)?;
    proof.stage(state, d, "fields").await?;
    let apply = click(d, "slack-list-rename-confirm").await?;
    wait_state(state, "local Apply", |s, l| {
        s["stage"] == Value::Null && s["localEmpty"] == false && l["requests"] == json!([])
    })
    .await?;
    let apply_return = wait_dom(
        d,
        "window.__listRefinementSave.returned('#slack-list-fields')",
    )
    .await?;
    proof.stage(state, d, "dirty").await?;
    Ok(
        json!({"open":open,"firstSerial":first,"firstBinding":first_binding,"cancelChoice":cancel_choice,"cancelHidden":cancel_hidden,"cancel":cancel,"cancelReturn":cancel_return,"cancelUnchanged":true,"reopen":reopen,"secondSerial":current,"secondBinding":second_binding,"choose":choose,"hidden":hidden,"stale":stale,"apply":apply,"applyReturn":apply_return,"requestsAfterApply":0}),
    )
}
pub(super) async fn settle(
    proof: &mut Proof,
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    attempt: u64,
) -> Result<Value, String> {
    if proof.mode == adapter::Case::Lost {
        return lost(proof, state, d, attempt).await;
    }
    // Hold the first actual metadata read before releasing the ACK. New starts
    // that read automatically; Existing exposes its ordinary explicit Check.
    rename_fixture::hold_read(&state.peek())?;
    rename_fixture::release(&state.peek(), false)?;
    let check = if proof.mode == adapter::Case::Existing {
        wait_state(
            state,
            "Existing parent completed with held child",
            |s, l| {
                s["stage"] == "Uncertain"
                    && l["preferencePending"] == true
                    && l["finished"].as_array().map(Vec::len) == Some(1)
            },
        )
        .await?;
        click(d, "slack-list-rename-check").await?
    } else {
        Value::Null
    };
    wait_state(state, "independent child and metadata holds", |s, l| {
        s["stage"] == "Checking"
            && l["readPending"] == true
            && l["preferencePending"] == true
            && l["finished"].as_array().map(Vec::len) == Some(1)
    })
    .await?;
    let selection = if proof.mode == adapter::Case::New {
        accept(state, d, attempt).await?
    } else {
        Value::Null
    };
    let parent = adapter::parent_completed_while_child_held(&state.peek())?;
    proof.stage(state, d, "pending").await?;
    rename_fixture::release(&state.peek(), true)?;
    let metadata = wait_state(
        state,
        "metadata observed before child settlement",
        |s, l| {
            s["stage"] == "Observed"
                && s["receipt"]["preference"] == "InFlight"
                && l["preferencePending"] == true
                && l["finished"].as_array().map(Vec::len) == Some(2)
        },
    )
    .await?;
    if metadata["ledger"]["requests"].as_array().map(Vec::len) != Some(9)
        || metadata["state"]["receipt"]["custodyCurrent"] != true
    {
        return Err("Metadata did not retain independent child custody".into());
    }
    adapter::release_preference(&state.peek())?;
    let preference = wait_state(state, "independent preference read observed", |s, l| {
        s["stage"] == "Observed"
            && s["receipt"]["preference"] == "Observed"
            && l["preferencePending"] == false
            && l["requests"].as_array().map(Vec::len) == Some(10)
    })
    .await?;
    let remount = if proof.mode == adapter::Case::New {
        wait_dom(d, "window.__listRefinementSave.bindFind({remount:true})").await?
    } else {
        Value::Null
    };
    proof.stage(state, d, "observed").await?;
    let close = click(
        d,
        if proof.mode == adapter::Case::New {
            "slack-list-refinement-status-close"
        } else {
            "slack-list-rename-close"
        },
    )
    .await?;
    wait_state(state, "current receipt dismissed", |s, _| {
        s["stage"] == Value::Null
    })
    .await?;
    let returned = if proof.mode == adapter::Case::Existing {
        wait_dom(
            d,
            "window.__listRefinementSave.returned('#slack-list-view-select')",
        )
        .await?
    } else {
        Value::Null
    };
    proof.stage(state, d, "board").await?;
    Ok(
        json!({"kind":"positive-ack","check":check,"selection":selection,"parentWhileChildHeld":parent,"metadataBeforePreference":metadata,"preferenceObserved":preference,"remountedFind":remount,"close":close,"returned":returned}),
    )
}
async fn lost(
    proof: &mut Proof,
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    attempt: u64,
) -> Result<Value, String> {
    rename_fixture::release(&state.peek(), false)?;
    wait_state(state, "New response lost", |s, l| {
        s["stage"] == "Uncertain" && l["finished"].as_array().map(Vec::len) == Some(1)
    })
    .await?;
    let retained = adapter::lost_reply_kept_local_state(&state.peek())?;
    proof.stage(state, d, "pending").await?;
    let check = click(d, "slack-list-rename-check").await?;
    wait_state(state, "unselected created view available", |s, l| {
        s["stage"] == "Available"
            && s["selected"] == "V_LANE"
            && s["receipt"]["preference"] == "NotRequested"
            && l["finished"].as_array().map(Vec::len) == Some(2)
    })
    .await?;
    proof.stage(state, d, "observed").await?;
    let open = click(d, "slack-list-new-view-open").await?;
    wait_state(state, "explicit Open available view", |s, _| {
        s["stage"] == Value::Null && s["selected"] == "V_REFINED_CREATED"
    })
    .await?;
    let selected = accept(state, d, attempt).await?;
    let find = if selected["find"]["oldDisconnected"] == true {
        wait_dom(d, "window.__listRefinementSave.bindFind({remount:true})").await?
    } else {
        selected["find"].clone()
    };
    proof.stage(state, d, "board").await?;
    Ok(
        json!({"kind":"lost-reply","retained":retained,"check":check,"open":open,"selection":selected,"find":find,"childRequests":0}),
    )
}
