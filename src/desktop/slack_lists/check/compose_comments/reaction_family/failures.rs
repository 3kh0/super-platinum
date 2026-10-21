use super::*;
const STAGES: &[&str] = &[
    "draft",
    "mixed",
    "blocked",
    "readback",
    "rate-limited",
    "rate-expired",
    "fresh",
    "retry-held",
    "retry-first-ack",
    "retry-done",
];
fn expected(i: usize) -> Expected {
    let phases = if i == 0 {
        vec![]
    } else {
        let mut p = vec![
            "confirmed",
            "rejected",
            if i < 3 { "unknown" } else { "observed" },
            "not-sent",
        ];
        if i >= 4 {
            p.extend(["rejected", "not-sent"]);
        }
        if i >= 7 {
            p.extend(if i == 7 {
                ["sent", "queued"]
            } else if i == 8 {
                ["confirmed", "sent"]
            } else {
                ["confirmed", "confirmed"]
            });
        }
        p
    };
    let removed = match i {
        0 => vec![],
        1..=2 => vec![family::OWN[0]],
        3..=7 => vec![family::OWN[0], family::OWN[2]],
        8 => vec![family::OWN[0], family::OWN[1], family::OWN[2]],
        _ => family::OWN.to_vec(),
    };
    Expected {
        reads: [4, 8, 8, 12, 16, 16, 20, 24, 24, 24][i],
        prepared: [0, 3, 3, 3, 4, 4, 4, 5, 6, 6][i],
        completed: [0, 3, 3, 3, 4, 4, 4, 4, 5, 6][i],
        phases,
        removed,
        held: matches!(i, 7 | 8),
        read_held: false,
        loading: false,
        cancelled: 0,
        disabled: matches!(i, 1 | 2 | 4 | 7 | 8),
        full_reads: usize::from(i >= 3),
    }
}
async fn stage(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &Value,
    rows: &mut Vec<Value>,
    i: usize,
) -> Result<(), String> {
    capture(state, m, d, STAGES[i], p, rows, expected(i)).await
}
fn server(state: Signal<ShellState>, m: &Model, p: &mut Value, stage: &str) -> Result<(), String> {
    let value = family::server(&state.peek(), m, &[family::OWN[0], family::OWN[2]])?;
    p["serverUpdates"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":stage,"value":value}));
    Ok(())
}
pub(super) async fn run(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    stage(state, m, d, p, rows, 0).await?;
    click_family(
        state,
        m,
        d,
        p,
        "mixed",
        &[
            dispatch::Mode::Success,
            dispatch::Mode::Rejected,
            dispatch::Mode::Unconfirmed,
        ],
        false,
    )
    .await?;
    stage(state, m, d, p, rows, 1).await?;
    click_family(state, m, d, p, "blocked", &[], true).await?;
    stage(state, m, d, p, rows, 2).await?;
    server(state, m, p, "readback")?;
    click(d, "slack-list-comments-refresh").await?;
    stage(state, m, d, p, rows, 3).await?;
    click_family(
        state,
        m,
        d,
        p,
        "rate-limited",
        &[dispatch::Mode::RateLimited],
        false,
    )
    .await?;
    stage(state, m, d, p, rows, 4).await?;
    for _ in 0..150 {
        if snapshot(state, m)?["comments"]["retryAt"] == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    if snapshot(state, m)?["comments"]["retryAt"] != 0 {
        return Err("Family actual cooldown did not retire".into());
    }
    stage(state, m, d, p, rows, 5).await?;
    server(state, m, p, "fresh")?;
    click(d, "slack-list-comments-refresh").await?;
    stage(state, m, d, p, rows, 6).await?;
    click_family(
        state,
        m,
        d,
        p,
        "retry-held",
        &[dispatch::Mode::Held; 2],
        false,
    )
    .await?;
    stage(state, m, d, p, rows, 7).await?;
    release(m, p, "retry-first-ack")?;
    stage(state, m, d, p, rows, 8).await?;
    release(m, p, "retry-done")?;
    stage(state, m, d, p, rows, 9).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completed_explicit_retry_enables_addition_but_unknown_rate_and_pending_stay_blocked() {
        for i in [1, 2, 4, 7, 8] {
            assert!(expected(i).disabled);
        }
        let e = expected(9);
        assert!(!e.disabled);
        assert_eq!(e.prepared, 6);
        assert_eq!(e.completed, 6);
        assert_eq!(e.removed, family::OWN);
        assert_eq!(
            e.phases,
            vec![
                "confirmed",
                "rejected",
                "observed",
                "not-sent",
                "rejected",
                "not-sent",
                "confirmed",
                "confirmed"
            ]
        );
        assert_eq!(e.full_reads, 1);
    }
}
