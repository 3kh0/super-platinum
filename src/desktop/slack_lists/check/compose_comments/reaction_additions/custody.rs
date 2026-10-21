use super::*;
const STAGES: [&str; 10] = [
    "draft",
    "acquisition-failed",
    "retry-loading",
    "preferences-retired",
    "catalog-held",
    "catalog-retired",
    "prefs-held",
    "prefs-retired",
    "unknown",
    "readback",
];
fn expected(i: usize) -> Expected {
    Expected {
        reads: [4, 4, 4, 4, 8, 8, 12, 12, 16, 20][i],
        posts: usize::from(i >= 8),
        phases: match i {
            4 => vec!["preparing"],
            5 => vec!["not-sent"],
            6 => vec!["not-sent", "preparing"],
            7 => vec!["not-sent"; 2],
            8 => vec!["not-sent", "not-sent", "unknown"],
            9 => vec!["not-sent", "not-sent", "observed"],
            _ => vec![],
        },
        addition: match i {
            1 | 3 => Some("failed"),
            2 => Some("loading"),
            _ => None,
        },
        prefs: [0, 1, 2, 2, 3, 3, 3, 3, 3, 3][i],
        pref_completed: [0, 1, 1, 2, 3, 3, 3, 3, 3, 3][i],
        pref_held: i == 2,
        preflights: [0, 0, 0, 0, 1, 1, 2, 2, 3, 3][i],
        preflight_held: matches!(i, 4 | 6),
        reactions: if i == 9 {
            own(&data::initial_reactions(), data::RAW[1])
        } else {
            data::initial_reactions()
        },
        target: data::RAW[1],
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
    data::preference_mode(&state.peek(), m, data::PrefMode::Failure)?;
    click(d, p, STAGES[1], data::RAW[1]).await?;
    stage(state, m, d, p, rows, 1).await?;
    let started = std::time::Instant::now();
    for _ in 0..900 {
        if crate::emoji_preferences::fixture_read_retry_ready(&state.peek()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    if !crate::emoji_preferences::fixture_read_retry_ready(&state.peek()) {
        return Err("Original preference retry deadline did not expire".into());
    }
    p["throttleWait"] = json!({"ready":true,"elapsedMillis":started.elapsed().as_millis()});
    data::preference_mode(&state.peek(), m, data::PrefMode::Held)?;
    click(d, p, STAGES[2], data::RAW[1]).await?;
    stage(state, m, d, p, rows, 2).await?;
    prefs(state, m, p, STAGES[3], 5)?;
    let release = data::release_preferences(m)?;
    p["releases"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":STAGES[3],"kind":"preferences","value":release}));
    stage(state, m, d, p, rows, 3).await?;
    data::preference_mode(&state.peek(), m, data::PrefMode::Success)?;
    compose::mode(m, compose::Mode::HeldPreflight)?;
    click(d, p, STAGES[4], data::RAW[1]).await?;
    stage(state, m, d, p, rows, 4).await?;
    let before = data::observed(&state.peek(), m)?;
    {
        let mut s = state.write();
        let w = s.core.workspaces.get_mut("T1").unwrap();
        w.custom_emoji.insert(
            "fixture_addition_changed".into(),
            super_platinum_core::slack::models::Emoji {
                name: "fixture_addition_changed".into(),
                value: "alias:eyes".into(),
                ..Default::default()
            },
        );
        w.custom_emoji_revision += 1;
    }
    let after = data::observed(&state.peek(), m)?;
    p["sourceChanges"].as_array_mut().unwrap().push(json!({"stage":STAGES[5],"kind":"accepted-custom-metadata","name":"fixture_addition_changed","value":"alias:eyes","before":before,"after":after}));
    let release = compose::release(m, "T1")?;
    p["releases"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":STAGES[5],"kind":"preflight","value":release}));
    stage(state, m, d, p, rows, 5).await?;
    compose::mode(m, compose::Mode::HeldPreflight)?;
    click(d, p, STAGES[6], data::RAW[1]).await?;
    stage(state, m, d, p, rows, 6).await?;
    prefs(state, m, p, STAGES[7], 5)?;
    let release = compose::release(m, "T1")?;
    p["releases"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":STAGES[7],"kind":"preflight","value":release}));
    stage(state, m, d, p, rows, 7).await?;
    dispatch::mode(&state.peek(), m, dispatch::Mode::Unconfirmed)?;
    click(d, p, STAGES[8], data::RAW[1]).await?;
    stage(state, m, d, p, rows, 8).await?;
    future(
        state,
        m,
        p,
        STAGES[9],
        own(&data::initial_reactions(), data::RAW[1]),
    )?;
    checked(d,"(()=>{const b=document.getElementById('slack-list-comments-refresh');if(!b||b.disabled)return {ok:false};b.click();return {ok:true};})()").await?;
    stage(state, m, d, p, rows, 9).await
}
