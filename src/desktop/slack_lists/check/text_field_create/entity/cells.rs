//! Actual cell open/local choice/Cancel, plus typed cardinality without dispatch.
use super::*;
use crate::slack_lists::{CellEdit, CellEditValue, editor};
fn empty_editor(state: Signal<ShellState>, case: EntityCase, column: &str) -> Result<u64, String> {
    let shell = state.peek();
    let p = editor::presentation(&shell).ok_or("Missing entity cell editor")?;
    let correct = match p.original.as_ref() {
        CellEditValue::Users(v) => !case.channel() && v.is_empty(),
        CellEditValue::Channels(v) => case.channel() && v.is_empty(),
        _ => false,
    };
    let kind = match (case.channel(), case.multiple()) {
        (false, false) => editor::EditorKind::User,
        (false, true) => editor::EditorKind::MultiUser,
        (true, false) => editor::EditorKind::Channel,
        (true, true) => editor::EditorKind::MultiChannel,
    };
    if !correct
        || p.kind != kind
        || p.column_id != column
        || p.row_id != "R_LIST_003"
        || p.disabled
        || p.saving
        || p.stale
    {
        return Err(
            "Configured creation leaked defaults into existing cell or changed cell format".into(),
        );
    }
    Ok(p.generation)
}
fn unchanged(state: Signal<ShellState>, proof: &Proof, ledger: &Value) -> Result<(), String> {
    proof.stable(state, 1)?;
    let shell = state.peek();
    if rename_fixture::snapshot(&shell)? != *ledger
        || !shell.slack_lists.operations.is_empty()
        || crate::slack_lists::fixture::attempt_count(&shell) != 0
    {
        return Err("Local cell compatibility dispatched a mutation".into());
    }
    Ok(())
}
pub(super) async fn run(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
    case: EntityCase,
) -> Result<Value, String> {
    let ledger = rename_fixture::snapshot(&state.peek())?;
    let (column, key) = proof.mode.observed();
    let typed = {
        let shell = state.peek();
        let loaded = shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .ok_or("Missing observed document")?;
        let make = |values: Vec<String>| {
            if case.channel() {
                CellEditValue::Channels(values)
            } else {
                CellEditValue::Users(values)
            }
        };
        let single = CellEdit::capture(
            &loaded.document,
            &loaded.records,
            "R_LIST_003",
            column,
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
            make(vec![case.ids()[0].into()]),
        );
        let multiple = CellEdit::capture(
            &loaded.document,
            &loaded.records,
            "R_LIST_003",
            column,
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
            make(case.ids().into_iter().map(str::to_owned).collect()),
        );
        if single.is_err() || multiple.is_ok() != case.multiple() {
            return Err("Creation defaults changed existing cell cardinality rules".into());
        }
        if loaded.records.raw["records"].as_array().is_none_or(|rows| {
            rows.iter().any(|r| {
                r["fields"].as_array().is_none_or(|fields| {
                    fields
                        .iter()
                        .any(|f| f["key"] == key || f["column_id"] == column)
                })
            })
        }) {
            return Err("Created defaults unexpectedly backfilled existing raw records".into());
        }
        json!({"singleAdmitted":true,"multipleAdmitted":multiple.is_ok(),"createDefaultIds":case.options()["default_value_typed"],"rawRowsUnchanged":loaded.records.raw==proof.population,"notExecuted":true})
    };
    checked(
        d,
        &format!("window.__listEntityCreate.cellOpen({})", json!(column)),
    )
    .await?;
    let mut generation = None;
    for _ in 0..150 {
        if let Ok(g) = empty_editor(state, case, column) {
            generation = Some(g);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    let first = generation.ok_or("Created cell editor did not open")?;
    let initial = wait_dom(
        d,
        &format!(
            "window.__listEntityCreate.cellReady({first},{},{})",
            case.channel(),
            case.multiple()
        ),
    )
    .await?;
    checked(
        d,
        &format!(
            "window.__listEntityCreate.cellSelect({first},{})",
            json!(case.ids()[0])
        ),
    )
    .await?;
    let local = wait_dom(
        d,
        &format!(
            "window.__listEntityCreate.cellLocal({first},{})",
            json!(case.ids()[0])
        ),
    )
    .await?;
    unchanged(state, proof, &ledger)?;
    proof.captures.insert(
        "entity-cell".into(),
        harness::capture(d, "entity-cell").await?,
    );
    unchanged(state, proof, &ledger)?;
    proof.stages.push(json!({"stage":"entity-cell","geometry":local,"initial":initial,"typed":typed,"ledger":ledger,"noSession":true,"noTransport":true}));
    checked(d, &format!("window.__listEntityCreate.cellCancel({first})")).await?;
    let returned = wait_dom(d, "window.__listEntityCreate.cellReturned()").await?;
    unchanged(state, proof, &ledger)?;
    checked(
        d,
        &format!("window.__listEntityCreate.cellOpen({})", json!(column)),
    )
    .await?;
    let mut second = None;
    for _ in 0..150 {
        if let Ok(g) = empty_editor(state, case, column) {
            if g != first {
                second = Some(g);
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    let second = second.ok_or("Reopened entity cell did not retain empty canonical seed")?;
    let reopen = wait_dom(
        d,
        &format!(
            "window.__listEntityCreate.cellReady({second},{},{})",
            case.channel(),
            case.multiple()
        ),
    )
    .await?;
    checked(
        d,
        &format!("window.__listEntityCreate.cellCancel({second})"),
    )
    .await?;
    let final_dom = wait_dom(d, "window.__listEntityCreate.cellReturned()").await?;
    unchanged(state, proof, &ledger)?;
    proof.captures.insert(
        "entity-canceled".into(),
        harness::capture(d, "entity-canceled").await?,
    );
    unchanged(state, proof, &ledger)?;
    proof.stages.push(json!({"stage":"entity-canceled","geometry":final_dom,"reopen":reopen,"ledger":ledger,"noSession":true,"noTransport":true}));
    Ok(
        json!({"typed":typed,"initial":initial,"local":local,"returned":returned,"reopen":reopen,"final":final_dom,"cellAttempts":0,"noBackfill":true,"ledgerUnchanged":true}),
    )
}
