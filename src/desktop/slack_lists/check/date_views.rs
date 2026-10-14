//! Local-calendar saved views exercised through the native picker and grid.
use super::*;
use crate::slack_lists::fixture;

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let attempts = fixture::attempt_count(&state.peek());
    fixture::add_date_views(&mut state.write());
    wait_dom(desktop, r#"({ok:!!document.querySelector('#slack-list-view-select option[value="V_DATE_TODAY"]')})"#).await?;
    choose(desktop, "V_DATE_TODAY").await?;
    rows(desktop, &["R_LIST_000"]).await?;
    timestamp(
        desktop,
        "R_LIST_000",
        "1 day ago",
        "Yesterday at 12:00:00 PM",
    )
    .await?;
    let today = capture_stage(desktop, "saved-date-today").await?;

    choose(desktop, "V_DATE_WEEK").await?;
    rows(desktop, &["R_LIST_001", "R_LIST_000", "R_LIST_003"]).await?;
    let week = capture_stage(desktop, "saved-date-week").await?;
    choose(desktop, "V_DATE_LATER").await?;
    rows(desktop, &["R_LIST_005"]).await?;

    // Captured negation is OR across choices, not a set complement. Missing
    // cells remain excluded and numeric zero is a real, nonempty Later value.
    let ordered = [
        "R_LIST_002",
        "R_LIST_003",
        "R_LIST_000",
        "R_LIST_001",
        "R_LIST_004",
        "R_LIST_005",
    ];
    choose(desktop, "V_DATE_NEGATED").await?;
    rows(desktop, &ordered).await?;
    choose(desktop, "V_DATE_SORT").await?;
    rows(desktop, &ordered).await?;

    choose(desktop, "V_DATE_SINCE").await?;
    rows(desktop, &["R_LIST_001", "R_LIST_002"]).await?;
    // Advance the production deadline path using a fixed fixture sample, with
    // no wall-clock wait or canonical read/write. DOM text and tooltip update.
    fixture::set_date_clock(
        &mut state.write(),
        "2026-10-12T12:02:00",
        "2026-10-12T12:00:00",
    );
    timestamp(
        desktop,
        "R_LIST_001",
        "2 minutes ago",
        "Today at 12:00:01 PM",
    )
    .await?;
    if !fixture::tick_date_clock(
        &mut state.write(),
        "2026-10-12T12:03:00",
        "2026-10-12T12:00:00",
    ) {
        return Err("Timestamp deadline did not refresh the visible List".into());
    }
    timestamp(
        desktop,
        "R_LIST_001",
        "3 minutes ago",
        "Today at 12:00:01 PM",
    )
    .await?;
    fixture::set_date_clock(
        &mut state.write(),
        "2026-10-13T12:00:00",
        "2026-10-12T12:00:01",
    );
    rows(desktop, &["R_LIST_002"]).await?;

    // Reproject the already selected layout at a local day boundary. No picker
    // event, request, account switch, or UTC-midnight assumption is involved.
    choose(desktop, "V_DATE_TODAY").await?;
    rows(desktop, &["R_LIST_000"]).await?;
    fixture::set_date_clock(
        &mut state.write(),
        "2026-10-14T00:00:00",
        "2026-10-12T12:00:01",
    );
    rows(desktop, &["R_LIST_001"]).await?;

    choose(desktop, "").await?;
    fixture::remove_date_views(&mut state.write());
    wait_dom(desktop, r#"({ok:document.querySelector('.slack-list-grid tbody tr[data-row-id]')?.cells.length===11&&!document.querySelector('#slack-list-view-select option[value="V_DATE_TODAY"]')})"#).await?;
    assert_attempts(state, attempts)?;
    Ok(
        json!({"today":true,"isoWeekSort":true,"laterNotEmpty":true,"negatedChoicesOr":true,
        "strictPriorRead":true,"priorReadReprojection":true,"localDayReprojection":true,
        "timestampRelativeLabel":true,"timestampTooltip":true,"timestampMinuteReprojection":true,"timestampReadOnly":true,
        "noWrites":true,"captures":{"today":today,"week":week}}),
    )
}

async fn timestamp(
    desktop: &dioxus::desktop::DesktopContext,
    row: &str,
    label: &str,
    title: &str,
) -> Result<(), String> {
    wait_dom(desktop, &format!(r#"(()=>{{const cell=document.querySelector('[data-row-id="{}"] [data-column-id="C_STAMP"]'),text=cell?.querySelector('.slack-list-timestamp');return {{ok:text?.textContent==={}&&text?.title==={}&&!cell.querySelector('button,input'),label:text?.textContent,title:text?.title}};}})()"#,row,json!(label),json!(title))).await?;
    Ok(())
}

async fn rows(desktop: &dioxus::desktop::DesktopContext, expected: &[&str]) -> Result<(), String> {
    wait_dom(desktop, &format!(r#"(()=>{{const rows=[...document.querySelectorAll('.slack-list-grid tbody tr[data-row-id]')].map(row=>row.dataset.rowId);return {{ok:JSON.stringify(rows)===JSON.stringify({}),rows,notices:document.querySelector('.slack-list-notices')?.textContent}};}})()"#, json!(expected))).await?;
    Ok(())
}

async fn choose(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    let before = checked(desktop, &format!(r#"(()=>{{const select=document.getElementById('slack-list-view-select'),dialog=select?.closest('.slack-list-dialog');if(!select||!dialog)return {{ok:false}};const previousValue=select.value,previousLayout=dialog.dataset.layoutGeneration;select.value={};select.dispatchEvent(new Event('change',{{bubbles:true}}));return {{ok:true,previousValue,previousLayout}};}})()"#, json!(id))).await?;
    wait_dom(desktop, &format!(r#"(()=>{{const select=document.getElementById('slack-list-view-select'),dialog=select?.closest('.slack-list-dialog');return {{ok:select?.value==={}&&!!dialog&&({}==={}||dialog.dataset.layoutGeneration!=={})}};}})()"#, json!(id), before["previousValue"], json!(id), before["previousLayout"])).await?;
    Ok(())
}
