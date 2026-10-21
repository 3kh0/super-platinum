use super::*;
const STAGES: &[&str] = &[
    "draft",
    "remove-held",
    "duplicate",
    "socket-removed",
    "first-ack",
    "second-ack",
    "third-ack",
    "read-held",
    "final-ack",
    "read-released",
];
fn expected(i: usize) -> Expected {
    let completed = match i {
        0..=3 => 0,
        4 => 1,
        5 => 2,
        6..=7 => 3,
        _ => 4,
    };
    let prepared = match i {
        0 => 0,
        1..=3 => 1,
        4 => 2,
        5 => 3,
        _ => 4,
    };
    let phases = if i == 0 {
        vec![]
    } else {
        (0..4)
            .map(|n| {
                if n < completed {
                    "confirmed"
                } else if n == completed {
                    "sent"
                } else {
                    "queued"
                }
            })
            .collect()
    };
    let removed = if i < 3 {
        vec![]
    } else if i < 5 {
        family::OWN[..1].to_vec()
    } else if i == 5 {
        family::OWN[..2].to_vec()
    } else if i < 8 {
        family::OWN[..3].to_vec()
    } else {
        family::OWN.to_vec()
    };
    Expected {
        reads: if i == 0 {
            4
        } else if i < 7 {
            8
        } else {
            12
        },
        prepared,
        completed,
        phases,
        removed,
        held: i > 0 && i < 8,
        read_held: matches!(i, 7 | 8),
        loading: i == 7,
        cancelled: u64::from(i >= 8),
        disabled: (1..8).contains(&i),
        full_reads: 0,
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
pub(super) async fn run(
    mut state: Signal<ShellState>,
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
        "remove-held",
        &[dispatch::Mode::Held; 4],
        false,
    )
    .await?;
    stage(state, m, d, p, rows, 1).await?;
    click_family(state, m, d, p, "duplicate", &[], true).await?;
    stage(state, m, d, p, rows, 2).await?;
    let frame = json!({"type":"reaction_removed","user":"U0","reaction":"thumbsup","item":{"type":"message","channel":"CCOMMENT01","ts":family::FIRST}});
    let event = super_platinum_core::slack::realtime::parse_event(&frame.to_string())
        .ok_or("Family socket parse failed")?;
    let before = snapshot(state, m)?;
    if crate::slack_lists::events::prepare(&mut state.write(), "T1", 7, &event) {
        return Err("Family socket unexpectedly requested read".into());
    }
    let after = snapshot(state, m)?;
    if before["comments"]["page"] != after["comments"]["page"]
        || before["draftStore"] != after["draftStore"]
        || before["ledger"] != after["ledger"]
    {
        return Err("Family socket changed original page/draft".into());
    }
    p["socket"] = json!({"team":"T1","generation":7,"frame":frame,"parsed":true,"before":before["familyProbe"],"after":after["familyProbe"]});
    stage(state, m, d, p, rows, 3).await?;
    for i in 4..=6 {
        release(m, p, STAGES[i])?;
        stage(state, m, d, p, rows, i).await?;
    }
    fixture::mode(m, fixture::Mode::Held)?;
    click(d, "slack-list-comments-refresh").await?;
    wait(state, m, &expected(7)).await?;
    p["heldResponse"] = fixture::held_response(&state.peek(), m)?;
    stage(state, m, d, p, rows, 7).await?;
    release(m, p, "final-ack")?;
    stage(state, m, d, p, rows, 8).await?;
    if fixture::held_response(&state.peek(), m)? != p["heldResponse"] {
        return Err("Family held original changed after local ACK".into());
    }
    p["release"] = fixture::release(m)?;
    if p["release"] != json!({"delivered":false,"cancelledBeforeReply":true}) {
        return Err("Family stale read delivered".into());
    }
    stage(state, m, d, p, rows, 9).await?;
    if snapshot(state, m)?["comments"]["page"] != p["initialResponse"] {
        return Err("Family held read overwrote original Page".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_removal_enables_a_new_no_own_invocation_without_replaying_children() {
        for i in 1..8 {
            assert!(expected(i).disabled);
        }
        for i in [8, 9] {
            let e = expected(i);
            assert!(!e.disabled);
            assert_eq!(e.prepared, 4);
            assert_eq!(e.completed, 4);
            assert_eq!(e.phases, vec!["confirmed"; 4]);
            assert_eq!(e.removed, family::OWN);
            assert_eq!(e.cancelled, 1);
        }
    }
}
