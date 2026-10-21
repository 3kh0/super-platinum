use super::*;
const STAGES: &[&str] = &[
    "draft",
    "rejected",
    "offline",
    "rate-limited",
    "rate-expired",
    "unconfirmed",
    "readback",
    "lone-pill",
    "malformed",
    "resolved",
];
fn expected(i: usize) -> Expected {
    let prepared = match i {
        0 => 0,
        1 => 1,
        2..=4 => 2,
        5..=7 => 3,
        _ => 4,
    };
    let attempts = match i {
        0 => 0,
        1 => 1,
        2 => 2,
        3..=4 => 3,
        5..=7 => 4,
        _ => 5,
    };
    let own = matches!(i, 6..=8);
    let count = if i == 9 {
        0
    } else if matches!(i, 7 | 8) {
        1
    } else if own {
        3
    } else {
        2
    };
    Expected {
        reads: [4, 8, 12, 16, 16, 20, 24, 24, 28, 32][i],
        prepared,
        issued: prepared.saturating_sub(u64::from(i >= 2)),
        completed: prepared,
        attempts,
        held: false,
        own,
        count,
        disabled: matches!(i, 3 | 5 | 8 | 9),
        phase: match i {
            0 => None,
            1 => Some("rejected"),
            2..=4 => Some("not-sent"),
            5 | 8 => Some("unknown"),
            _ => Some("observed"),
        },
        read_held: false,
        loading: false,
        cancelled: 0,
    }
}
async fn stage(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    i: usize,
    p: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    if i == 4 {
        // available() uses the wall clock; the owner's scheduled wake separately
        // commits retry_at=0. Capture only after that actual mutation retires.
        wait(state, m, |v| {
            expected(i).ready(v) && v["comments"]["retryAt"] == 0
        })
        .await?;
    }
    capture(state, m, d, STAGES[i], p, rows, expected(i)).await
}
async fn click_mode(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    name: &str,
    mode: dispatch::Mode,
) -> Result<(), String> {
    dispatch::mode(&state.peek(), m, mode)?;
    let input = click_pill(d, false).await?;
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":name,"value":input}));
    Ok(())
}
pub(super) async fn run(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    stage(state, m, d, 0, p, rows).await?;
    click_mode(state, m, d, p, "rejected", dispatch::Mode::Rejected).await?;
    stage(state, m, d, 1, p, rows).await?;
    click_mode(state, m, d, p, "offline", dispatch::Mode::Offline).await?;
    stage(state, m, d, 2, p, rows).await?;
    fixture::mode(m, fixture::Mode::RateLimited)?;
    let input = click_pill(d, false).await?;
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"rate-limited","value":input}));
    stage(state, m, d, 3, p, rows).await?;
    // Actual account-scoped retry deadline and renderer wake, never clear it directly.
    stage(state, m, d, 4, p, rows).await?;
    click_mode(state, m, d, p, "unconfirmed", dispatch::Mode::Unconfirmed).await?;
    stage(state, m, d, 5, p, rows).await?;
    server(state, m, p, "readback", true)?;
    click(d, "slack-list-comments-refresh").await?;
    stage(state, m, d, 6, p, rows).await?;
    let mut events = vec![];
    for user in ["U1", "U2"] {
        events.push(socket_user(state, m, "lone-pill", false, user)?);
    }
    p["loneEvents"] = json!(events);
    let user = compose::session(&state.peek())
        .map_err(|_| "Self missing")?
        .user_id;
    let raw = dispatch::server_members(&state.peek(), m, vec![user])?;
    p["serverUpdates"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"lone-pill","value":raw}));
    stage(state, m, d, 7, p, rows).await?;
    click_mode(state, m, d, p, "malformed", dispatch::Mode::Malformed).await?;
    stage(state, m, d, 8, p, rows).await?;
    let raw = dispatch::server_members(&state.peek(), m, vec![])?;
    p["serverUpdates"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"resolved","value":raw}));
    click(d, "slack-list-comments-refresh").await?;
    stage(state, m, d, 9, p, rows).await
}
