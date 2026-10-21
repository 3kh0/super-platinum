use super::*;
const STAGES: &[&str] = &[
    "draft",
    "rejected",
    "unknown",
    "readback",
    "rate-limited",
    "rate-expired",
    "retry-held",
    "retried",
];
fn expected(i: usize) -> Expected {
    let phases = match i {
        0 => vec![],
        1 => vec!["rejected"],
        2 => vec!["rejected", "unknown"],
        3 => vec!["rejected", "observed"],
        4 | 5 => vec!["rejected", "observed", "rejected"],
        6 => vec!["rejected", "observed", "rejected", "sent"],
        _ => vec!["rejected", "observed", "rejected", "confirmed"],
    };
    Expected {
        reads: [4, 8, 12, 16, 20, 20, 24, 24][i],
        posts: [0, 1, 2, 2, 3, 3, 4, 4][i],
        completed: [0, 1, 2, 2, 3, 3, 3, 4][i],
        phases,
        picker: None,
        reactions: if matches!(i, 3..=6) {
            with(&data::initial_reactions(), "eyes", false)
        } else if i == 7 {
            with(
                &with(&data::initial_reactions(), "eyes", false),
                "eyes",
                true,
            )
        } else {
            data::initial_reactions()
        },
    }
}
async fn stage(
    s: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    i: usize,
    p: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    capture(s, m, d, STAGES[i], p, rows, expected(i)).await
}
pub(super) async fn run(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    stage(state, m, d, 0, p, rows).await?;
    choose(state, m, d, p, "rejected", "eyes", dispatch::Mode::Rejected).await?;
    stage(state, m, d, 1, p, rows).await?;
    choose(
        state,
        m,
        d,
        p,
        "unknown",
        "eyes",
        dispatch::Mode::Unconfirmed,
    )
    .await?;
    stage(state, m, d, 2, p, rows).await?;
    future(
        state,
        m,
        p,
        "readback",
        with(&data::initial_reactions(), "eyes", false),
    )?;
    checked(
        d,
        "window.__listCompose.click('slack-list-comments-refresh')",
    )
    .await?;
    stage(state, m, d, 3, p, rows).await?;
    let v = snapshot(state, m)?;
    if v["reactionLedger"]["fullReads"].as_array().map(Vec::len) != Some(1) {
        return Err("Unknown exact removal needs full reaction readback".into());
    }
    choose(
        state,
        m,
        d,
        p,
        "rate-limited",
        "eyes",
        dispatch::Mode::RateLimited,
    )
    .await?;
    stage(state, m, d, 4, p, rows).await?;
    wait(state, m, |v| {
        v["comments"]["retryAt"] == 0
            && v["comments"]["retryAfter"] == 0
            && v["actionProbe"]["available"] == true
    })
    .await?;
    stage(state, m, d, 5, p, rows).await?;
    choose(state, m, d, p, "retry-held", "eyes", dispatch::Mode::Held).await?;
    stage(state, m, d, 6, p, rows).await?;
    release(m, p, "retried")?;
    stage(state, m, d, 7, p, rows).await
}
