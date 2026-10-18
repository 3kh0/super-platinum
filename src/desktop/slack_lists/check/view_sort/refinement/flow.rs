use super::*;
pub(super) async fn prelude(
    proof: &mut CurrentProof,
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let opened = rows::open(state, d).await?;
    let mut actions = vec![];
    for column in ["C_NUMBER", "C_CHECK", "C_NAME"] {
        actions.push(rows::add(state, d, column).await?);
    }
    proof
        .stage(state, d, "structure-added", 0, 0, 0, 0, 0, false)
        .await?;
    actions.push(rows::action(state, d, Some("C_CHECK"), "remove", None).await?);
    proof
        .stage(state, d, "structure-removed", 0, 0, 0, 0, 0, false)
        .await?;
    for (column, kind, value) in [
        (Some("C_NAME"), "field", Some("C_SELECT")),
        (Some("C_SELECT"), "field", Some("C_NAME")),
        (Some("C_NAME"), "direction", Some("descending")),
        (Some("C_NAME"), "up", None),
        (Some("C_NAME"), "down", None),
        (None, "clear", None),
    ] {
        actions.push(rows::action(state, d, column, kind, value).await?);
    }
    let model = draft(state)?;
    if model["desired"] != model["original"] || model["rows"] != json!([]) {
        return Err("Sort structural prelude changed original".into());
    }
    let disabled = wait_dom(
        d,
        "({ok:document.getElementById('slack-list-rename-confirm')?.disabled===true})",
    )
    .await?;
    proof.exact(state, 0, 0, 0, false, false, false)?;
    Ok(json!({"model":model,"opened":opened,"actions":actions,"disabled":disabled}))
}
pub(super) async fn retire(
    proof: &mut CurrentProof,
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    count: usize,
    finished: usize,
) -> Result<Value, String> {
    rename_fixture::hold_read(&state.peek())?;
    act(d, "slack-list-rename-check").await?;
    phase(state, Stage::Checking).await?;
    pending(state, "readPending").await?;
    let held = proof.exact(state, count + 1, finished, 1, false, true, false)?;
    let original = adapter::state(&state.peek())?;
    let (loaded, find) = {
        let shell = state.peek();
        let v = shell.slack_lists.view.as_ref().unwrap();
        (
            v.loaded.clone().ok_or("Missing canonical Sort")?,
            v.layout.find.clone(),
        )
    };
    let old_epoch = state.peek().core.account_epoch;
    let close = act(d, "slack-list-rename-leave").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    state.write().core.account_epoch = old_epoch.checked_add(1).ok_or("Fixture epoch exhausted")?;
    let visit = rename_fixture::reopen(&mut state.write())?;
    adapter::seed_reopen(&mut state.write(), visit, loaded, find)?;
    let replacement = proof.model(state, 0, 0)?;
    if replacement["state"]["visit"] == original["visit"]
        || replacement["state"]["selection"] == original["selection"]
        || replacement["state"]["localEmpty"] != true
    {
        return Err("Replacement reused local owner/draft".into());
    }
    let rebound = wait_dom(
        d,
        &format!(
            "window.__listSortRefinement.rebind({},{})",
            replacement["state"]["visit"], replacement["state"]["selection"]
        ),
    )
    .await?;
    if adapter::snapshot(&state.peek())? != held {
        return Err("Held read changed before release".into());
    }
    rename_fixture::release(&state.peek(), true)?;
    completed(state, finished + 1).await?;
    phase(state, Stage::Uncertain).await?;
    let retired = proof.exact(state, count + 1, finished + 1, 1, false, false, false)?;
    let after = proof.model(state, 0, 0)?;
    if after["state"]["original"] != original["original"]
        || after["state"]["serial"] != original["serial"]
        || after["state"]["document"] != replacement["state"]["document"]
        || after["state"]["population"] != replacement["state"]["population"]
    {
        return Err("Retired response mutated replacement or retained intent".into());
    }
    Ok(
        json!({"held":held,"old":original,"close":close,"replacement":replacement,"rebound":rebound,"completed":retired,"after":after}),
    )
}
