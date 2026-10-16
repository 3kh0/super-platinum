//! Three intentional multi-cell saves through the same worker and observers.
use super::*;
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
) -> Result<Value, String> {
    proof.stable(state, 1)?;
    let creation = rename_fixture::snapshot(&state.peek())?;
    let options = proof
        .select_options
        .clone()
        .ok_or("Missing independent Multi options")?;
    if options["format"] != "multi_select" {
        return Err("Wrong Multi configuration".into());
    }
    let sync = cells::arm(
        &mut state.write(),
        &proof.docs[0],
        &proof.docs[1],
        &proof.population,
        proof.id,
        &options,
    )?;
    proof.stable(state, 1)?;
    let values = oracle::values(&options)?;
    let pages = oracle::pages_for(&proof.population, &values)?;
    checked(d, include_str!("select.js")).await?;
    checked(
        d,
        &format!("window.__listSelectCreate.configure({options})"),
    )
    .await?;
    let initial = checked(d, "window.__listSelectCreate.initial()").await?;
    let first = open(state, d, &json!([])).await?;
    let missing_noop = noop(state, d, first).await?;
    // Event order is deliberately the reverse of schema order. The DOM inventory
    // is schema-ordered; only the actual typed attempt proves submission order.
    let mut edits = vec![];
    for value in &values[0] {
        edits.push(
            checked(
                d,
                &format!(
                    "window.__listSelectCreate.toggle({},true,{first})",
                    json!(value)
                ),
            )
            .await?,
        );
    }
    wait_dom(
        d,
        &format!(
            "window.__listSelectCreate.selection({}, {first})",
            json!([values[0][1], values[0][0]])
        ),
    )
    .await?;
    stage(state, d, proof, &pages, &sync, 0, &creation).await?;
    let mut generations = vec![first];
    let mut disabled = vec![];
    let mut returned = vec![];
    for index in 0..3 {
        let generation = if index == 0 {
            first
        } else {
            let generation = open(state, d, &json!(values[index - 1])).await?;
            if generations.contains(&generation) {
                return Err("Multi editor generation reused".into());
            }
            generations.push(generation);
            let removed = if index == 1 {
                &values[0][1]
            } else {
                &values[1][0]
            };
            edits.push(
                checked(
                    d,
                    &format!(
                        "window.__listSelectCreate.toggle({},false,{generation})",
                        json!(removed)
                    ),
                )
                .await?,
            );
            wait_dom(
                d,
                &format!(
                    "window.__listSelectCreate.selection({},{generation})",
                    json!(values[index])
                ),
            )
            .await?;
            generation
        };
        click(d, "slack-list-editor-save", generation).await?;
        stage(state, d, proof, &pages, &sync, index * 3 + 1, &creation).await?;
        let held = cells::snapshot(&state.peek())?;
        disabled.push(
            checked(
                d,
                &format!("window.__listSelectCreate.disabledNoReplay({generation})"),
            )
            .await?,
        );
        if cells::snapshot(&state.peek())? != held {
            return Err("Disabled Multi controls replayed".into());
        }
        fixture::complete_pending(&mut state.write());
        stage(state, d, proof, &pages, &sync, index * 3 + 2, &creation).await?;
        click(d, "slack-list-editor-refresh", generation).await?;
        returned.push(wait_dom(d, "window.__listSelectCreate.returned()").await?);
        if index < 2 {
            stage(state, d, proof, &pages, &sync, index * 3 + 3, &creation).await?;
        }
    }
    let last = open(state, d, &json!([])).await?;
    if generations.contains(&last) {
        return Err("Final Multi generation reused".into());
    }
    generations.push(last);
    let empty_noop = noop(state, d, last).await?;
    click(d, "slack-list-editor-cancel", last).await?;
    let canceled = wait_dom(d, "window.__listSelectCreate.returned()").await?;
    stage(state, d, proof, &pages, &sync, 9, &creation).await?;
    let geometry = checked(d, "window.__listSelectCreate.geometry('cleared',null)").await?;
    let final_state = stable(state, proof, &pages, &sync, 9, &creation)?;
    Ok(
        json!({"format":"multi_select","sync":sync,"initial":initial,"pages":pages,"values":values,
        "generations":generations,"missingNoop":missing_noop,"edits":edits,"disabledNoReplay":disabled,
        "returned":returned,"emptyNoop":empty_noop,"canceled":canceled,"ledger":final_state["ledger"],
        "geometry":geometry,"actualForms":14,"cellLocalPreflights":3,"cellLocalReadCompletions":3,"httpExecuted":0}),
    )
}
