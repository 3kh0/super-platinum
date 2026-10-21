use super::*;
const STAGES: [&str; 8] = [
    "draft",
    "cold-loading",
    "wave-added",
    "single-added",
    "multi-added",
    "paired-added",
    "raw-added",
    "alias-added",
];
fn expected(i: usize) -> Expected {
    let done = i.saturating_sub(1);
    Expected {
        reads: 4 + 4 * done,
        posts: done,
        phases: vec!["confirmed"; done],
        addition: (i == 1).then_some("loading"),
        prefs: usize::from(i > 0),
        pref_completed: u64::from(i > 1),
        pref_held: i == 1,
        preflights: done,
        preflight_held: false,
        reactions: if i > 1 {
            own(&data::initial_reactions(), data::RESOLVED[i - 2])
        } else {
            data::initial_reactions()
        },
        target: if i > 1 { data::RESOLVED[i - 2] } else { "wave" },
    }
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    capture(state, m, d, STAGES[0], p, rows, expected(0)).await?;
    click(d, p, STAGES[1], "wave").await?;
    capture(state, m, d, STAGES[1], p, rows, expected(1)).await?;
    let release = data::release_preferences(m)?;
    if release["delivered"] != true {
        return Err("Original cold reply lost".into());
    }
    p["releases"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"wave-added","kind":"preferences","value":release}));
    capture(state, m, d, STAGES[2], p, rows, expected(2)).await?;
    for i in 3..8 {
        let previous = data::RESOLVED[i - 3];
        let frame = json!({"type":"reaction_removed","user":"U0","reaction":previous,"item":{"type":"message","channel":edits::CHANNEL,"ts":data::FIRST}});
        let event = super_platinum_core::slack::realtime::parse_event(&frame.to_string())
            .ok_or("Addition removal event missing")?;
        let before = snapshot(state, m)?;
        if crate::slack_lists::events::prepare(&mut state.write(), "T1", 7, &event) {
            return Err("Addition removal requested List reload".into());
        }
        let after = snapshot(state, m)?;
        if before["comments"]["page"] != after["comments"]["page"]
            || before["draftStore"] != after["draftStore"]
        {
            return Err("Socket changed original page/draft".into());
        }
        p["socket"].as_array_mut().unwrap().push(json!({"stage":STAGES[i],"team":"T1","generation":7,"frame":frame,"parsed":true,"before":edits::event_state(&before),"after":edits::event_state(&after)}));
        future(state, m, p, STAGES[i], data::initial_reactions())?;
        click(d, p, STAGES[i], data::RAW[i - 2]).await?;
        capture(state, m, d, STAGES[i], p, rows, expected(i)).await?;
    }
    Ok(())
}
