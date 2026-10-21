use super::*;
const STAGES: &[&str] = &[
    "draft",
    "add-held",
    "duplicate",
    "socket-added",
    "added",
    "remove-held",
    "socket-removed",
    "removed",
    "add-again-held",
    "socket-add-again",
    "socket-opposite",
    "acknowledged",
    "local-add-held",
    "read-held",
    "local-acknowledged",
    "read-released",
];
fn expected(i: usize) -> Expected {
    let prepared = match i {
        0 => 0,
        1..=4 => 1,
        5..=7 => 2,
        8..=11 => 3,
        _ => 4,
    };
    let completed = match i {
        0..=3 => 0,
        4..=6 => 1,
        7..=10 => 2,
        11..=13 => 3,
        _ => 4,
    };
    let own = matches!(i, 3 | 4 | 5 | 9 | 14 | 15);
    Expected {
        reads: match i {
            0 => 4,
            1..=4 => 8,
            5..=7 => 12,
            8..=11 => 16,
            12 => 20,
            _ => 24,
        },
        prepared,
        issued: prepared,
        completed,
        attempts: prepared,
        held: prepared > completed,
        own,
        count: if own { 3 } else { 2 },
        disabled: prepared > completed,
        phase: if i == 0 {
            None
        } else if prepared > completed {
            Some("sent")
        } else {
            Some("confirmed")
        },
        read_held: matches!(i, 13 | 14),
        loading: i == 13,
        cancelled: u64::from(i >= 14),
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
    capture(state, m, d, STAGES[i], p, rows, expected(i)).await
}
pub(super) async fn run(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    rows: &mut Vec<Value>,
    events: &mut Vec<Value>,
) -> Result<(), String> {
    stage(state, m, d, 0, p, rows).await?;
    dispatch::mode(&state.peek(), m, dispatch::Mode::Held)?;
    let input = click_pill(d, false).await?;
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"add-held","value":input}));
    stage(state, m, d, 1, p, rows).await?;
    let before = snapshot(state, m);
    let input = click_pill(d, true).await?;
    settled(d).await?;
    if snapshot(state, m) != before {
        return Err("Disabled duplicate reaction dispatched".into());
    }
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"duplicate","value":input}));
    stage(state, m, d, 2, p, rows).await?;
    events.push(socket(state, m, "socket-added", true)?);
    server(state, m, p, "socket-added", true)?;
    stage(state, m, d, 3, p, rows).await?;
    release(m, p, "added")?;
    stage(state, m, d, 4, p, rows).await?;
    dispatch::mode(&state.peek(), m, dispatch::Mode::Held)?;
    let input = click_pill(d, false).await?;
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"remove-held","value":input}));
    stage(state, m, d, 5, p, rows).await?;
    events.push(socket(state, m, "socket-removed", false)?);
    server(state, m, p, "socket-removed", false)?;
    stage(state, m, d, 6, p, rows).await?;
    release(m, p, "removed")?;
    stage(state, m, d, 7, p, rows).await?;
    dispatch::mode(&state.peek(), m, dispatch::Mode::Held)?;
    let input = click_pill(d, false).await?;
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"add-again-held","value":input}));
    stage(state, m, d, 8, p, rows).await?;
    events.push(socket(state, m, "socket-add-again", true)?);
    server(state, m, p, "socket-add-again", true)?;
    stage(state, m, d, 9, p, rows).await?;
    events.push(socket(state, m, "socket-opposite", false)?);
    server(state, m, p, "socket-opposite", false)?;
    stage(state, m, d, 10, p, rows).await?;
    release(m, p, "acknowledged")?;
    stage(state, m, d, 11, p, rows).await?;
    dispatch::mode(&state.peek(), m, dispatch::Mode::Held)?;
    let input = click_pill(d, false).await?;
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"local-add-held","value":input}));
    stage(state, m, d, 12, p, rows).await?;
    fixture::mode(m, fixture::Mode::Held)?;
    click(d, "slack-list-comments-refresh").await?;
    wait(state, m, |v| expected(13).ready(v)).await?;
    p["heldResponse"] = fixture::held_response(&state.peek(), m)?;
    stage(state, m, d, 13, p, rows).await?;
    release(m, p, "local-acknowledged")?;
    stage(state, m, d, 14, p, rows).await?;
    if fixture::held_response(&state.peek(), m)? != p["heldResponse"] {
        return Err("Old held read was rewritten after local ACK".into());
    }
    p["release"] = fixture::release(m)?;
    if p["release"] != json!({"delivered":false,"cancelledBeforeReply":true}) {
        return Err("Old read survived local ACK".into());
    }
    stage(state, m, d, 15, p, rows).await
}
