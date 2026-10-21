use super::*;
const STAGES: &[&str] = &[
    "draft",
    "opened",
    "searched",
    "cancelled",
    "add-held",
    "socket-added",
    "added",
    "exact-removed",
    "custom-added",
    "alias-removed",
];
fn expected(i: usize) -> Expected {
    let mut reactions = data::initial_reactions();
    if matches!(i, 5 | 6) {
        reactions = with(&reactions, data::TONE, true);
    }
    if i == 8 {
        reactions = with(&reactions, data::CUSTOM, true);
    }
    Expected {
        reads: [4, 4, 4, 4, 8, 8, 8, 12, 16, 20][i],
        posts: [0, 0, 0, 0, 1, 1, 1, 2, 3, 4][i],
        completed: [0, 0, 0, 0, 0, 0, 1, 2, 3, 4][i],
        phases: match i {
            0..=3 => vec![],
            4 | 5 => vec!["sent"],
            _ => vec!["confirmed"; i - 5],
        },
        picker: matches!(i, 1 | 2).then_some(true),
        reactions,
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
    mut state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    stage(state, m, d, 0, p, rows).await?;
    let input = open_picker(d).await?;
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"opened","value":input}));
    stage(state, m, d, 1, p, rows).await?;
    let input = search(d, "+1").await?;
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"searched","value":input}));
    stage(state, m, d, 2, p, rows).await?;
    let input = close_picker(d).await?;
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"cancelled","value":input}));
    stage(state, m, d, 3, p, rows).await?;
    choose(state, m, d, p, "add-held", "+1", dispatch::Mode::Held).await?;
    stage(state, m, d, 4, p, rows).await?;
    let frame = json!({"type":"reaction_added","user":"U0","reaction":data::TONE,"item":{"type":"message","channel":edits::CHANNEL,"ts":data::FIRST}});
    let event = super_platinum_core::slack::realtime::parse_event(&frame.to_string())
        .ok_or("Selection socket frame missing")?;
    let before = snapshot(state, m)?;
    if crate::slack_lists::events::prepare(&mut state.write(), "T1", 7, &event) {
        return Err("Selection socket requested List reload".into());
    }
    let after = snapshot(state, m)?;
    if before["comments"]["page"] != after["comments"]["page"]
        || before["draftStore"] != after["draftStore"]
    {
        return Err("Selection socket changed original page/draft".into());
    }
    p["socket"].as_array_mut().unwrap().push(json!({"stage":"socket-added","team":"T1","generation":7,"frame":frame,"parsed":true,"before":edits::event_state(&before),"after":edits::event_state(&after)}));
    future(
        state,
        m,
        p,
        "socket-added",
        with(&data::initial_reactions(), data::TONE, true),
    )?;
    stage(state, m, d, 5, p, rows).await?;
    release(m, p, "added")?;
    stage(state, m, d, 6, p, rows).await?;
    choose(
        state,
        m,
        d,
        p,
        "exact-removed",
        "+1",
        dispatch::Mode::Success,
    )
    .await?;
    stage(state, m, d, 7, p, rows).await?;
    future(state, m, p, "exact-removed", data::initial_reactions())?;
    choose(
        state,
        m,
        d,
        p,
        "custom-added",
        data::CUSTOM,
        dispatch::Mode::Success,
    )
    .await?;
    stage(state, m, d, 8, p, rows).await?;
    future(
        state,
        m,
        p,
        "custom-added",
        with(&data::initial_reactions(), data::CUSTOM, true),
    )?;
    choose(
        state,
        m,
        d,
        p,
        "alias-removed",
        data::ALIAS,
        dispatch::Mode::Success,
    )
    .await?;
    stage(state, m, d, 9, p, rows).await
}
