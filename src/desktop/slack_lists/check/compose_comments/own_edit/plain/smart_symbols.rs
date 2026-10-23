//! Original smart-symbol keys, deferred quote input, and independent Save custody.
use super::*;

fn script(action: &str, arg: Value) -> String {
    format!(
        "({})({},{})",
        include_str!("smart_symbols.js"),
        json!(action),
        arg
    )
}
async fn event(
    d: &dioxus::desktop::DesktopContext,
    action: &str,
    arg: Value,
) -> Result<Value, String> {
    checked(d, &script(action, arg)).await
}
fn ops(text: &str) -> Value {
    json!([{"insert":format!("{text}\n")}])
}
async fn settle(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    text: &str,
) -> Result<Value, String> {
    let expected = ops(text);
    wait(state, m, |v| {
        v["capture"]["canSave"] == true
            && v["edit"]["active"]["plainDelta"]["ops"] == expected
            && v["edit"]["active"]["composing"] == false
            && v["edit"]["active"]["nativePending"] == false
    })
    .await?;
    let native = wait_dom(d, &script("state", json!({"ops":expected}))).await?;
    let inventory = durable(state, &expected).await?;
    Ok(json!({"native":native,"inventory":inventory}))
}
async fn key(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    text: &str,
) -> Result<Value, String> {
    let observed = event(d, "key", json!({"key":name})).await?;
    let settled = settle(state, m, d, text).await?;
    Ok(json!({"event":observed,"settled":settled}))
}
async fn insert(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    text: &str,
    expected: &str,
) -> Result<Value, String> {
    let observed = event(d, "insert", json!({"text":text,"composing":false})).await?;
    let settled = settle(state, m, d, expected).await?;
    Ok(json!({"event":observed,"settled":settled}))
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let readiness = harness::wait_runner(state).await?;
    let m = fixture::begin(&mut state.write())?;
    fixture::completion::seed(state).await?;
    let seed_source = data::seed(state)?;
    checked(d, include_str!("../../../comments.js")).await?;
    checked(d, include_str!("../../../compose_comments.js")).await?;
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    let seed = compose::seed_draft(state).await?;
    write(d).await?;
    type_text(d, "Independent reply A \u{4e2d}\u{1f642}").await?;
    settled(d).await?;
    let remembered = command(d, "remember", Value::Null).await?;
    let disk = wire::Disk::capture(&state.peek())?;
    let enabled = source::smart_symbols::enabled();
    let disabled = source::smart_symbols::disabled();
    let mut p = json!({"source":seed_source,"replySeed":seed,"replyBlocks":remembered["blocks"],"opens":[],"inputs":[],"actions":[],"storageControls":[],"originalDisk":disk.snapshot().await?,"smartSymbolsEnabled":enabled,"smartSymbolsDisabled":disabled});
    let mut rows = Vec::new();
    take(state, &m, d, "draft", &p, &mut rows, false).await?;
    open_plain(state, &m, d, &mut p).await?;
    replace(state, &m, d, &mut p, "Marks --").await?;
    settle(state, &m, d, "Marks --").await?;
    take(state, &m, d, "editing", &p, &mut rows, false).await?;
    let mut symbols = Vec::new();
    for (i, (literal, converted)) in [
        ("Marks --", "Marks \u{2014} "),
        ("Right ->", "Right \u{2192} "),
        ("Left <-", "Left \u{2190} "),
        ("Both <->", "Both \u{2194} "),
    ]
    .into_iter()
    .enumerate()
    {
        if i > 0 {
            replace(state, &m, d, &mut p, literal).await?;
        }
        let expected = if enabled {
            converted.to_string()
        } else {
            format!("{literal} ")
        };
        let space = key(state, &m, d, "Space", &expected).await?;
        let mut receipt = json!({"literal":literal,"space":space});
        if enabled {
            receipt["undo"] = key(state, &m, d, "Undo", &format!("{literal} ")).await?;
            receipt["redo"] = key(state, &m, d, "Redo", converted).await?;
        }
        symbols.push(receipt);
    }
    p["symbols"] = json!(symbols);
    if enabled {
        take(state, &m, d, "space-symbols", &p, &mut rows, false).await?;
    }
    replace(state, &m, d, &mut p, "Wait ..").await?;
    p["period"] = key(
        state,
        &m,
        d,
        "Period",
        if enabled { "Wait \u{2026}" } else { "Wait ..." },
    )
    .await?;
    if enabled {
        p["periodUndo"] = key(state, &m, d, "Undo", "Wait ...").await?;
        p["periodRedo"] = key(state, &m, d, "Redo", "Wait \u{2026}").await?;
        take(state, &m, d, "period", &p, &mut rows, false).await?;
        let mut guards = Vec::new();
        for (name, text, key_name, expected) in [
            ("code", "`Code --", "Space", "`Code -- "),
            ("repeated-dash", "Repeated ---", "Space", "Repeated --- "),
            ("repeated-period", "Wait ...", "Period", "Wait ...."),
        ] {
            replace(state, &m, d, &mut p, text).await?;
            guards.push(json!({"kind":name,"key":key(state,&m,d,key_name,expected).await?}));
        }
        replace(state, &m, d, &mut p, "Pick --").await?;
        let selection = event(d, "select", json!({"start":5,"end":7})).await?;
        guards.push(json!({"kind":"selection","selection":selection,"key":key(state,&m,d,"Space","Pick  ").await?}));
        replace(state, &m, d, &mut p, "Compose --").await?;
        let start = event(d, "composition", json!({"active":true})).await?;
        let observed = event(d, "key", json!({"key":"Space"})).await?;
        let end = event(d, "composition", json!({"active":false})).await?;
        let settled = settle(state, &m, d, "Compose --").await?;
        guards.push(
            json!({"kind":"composition","start":start,"key":observed,"end":end,"settled":settled}),
        );
        p["guards"] = json!(guards);
        take(state, &m, d, "guards", &p, &mut rows, false).await?;
    }
    replace(state, &m, d, &mut p, "He said ").await?;
    p["quotes"] = insert(
        state,
        &m,
        d,
        "\"hello\"; can't; '90; 6'",
        if enabled {
            "He said \u{201c}hello\u{201d}; can\u{2019}t; \u{2018}90; 6\u{2019}"
        } else {
            "He said \"hello\"; can't; '90; 6'"
        },
    )
    .await?;
    if enabled {
        replace(state, &m, d, &mut p, "Compose ").await?;
        let start = event(d, "composition", json!({"active":true})).await?;
        let input = event(d, "insert", json!({"text":"\"yes\"","composing":true})).await?;
        let end = event(d, "composition", json!({"active":false})).await?;
        p["compositionQuotes"] = json!({"start":start,"input":input,"end":end,"settled":settle(state,&m,d,"Compose \u{201c}yes\u{201d}").await?});
        take(state, &m, d, "quotes", &p, &mut rows, false).await?;
    } else {
        take(state, &m, d, "literal", &p, &mut rows, false).await?;
    }
    p["cancel"] = button(d, "slack-list-comment-edit-cancel").await?;
    closed(state, &m, d).await?;
    take(state, &m, d, "cancelled", &p, &mut rows, false).await?;
    if enabled {
        open_plain(state, &m, d, &mut p).await?;
        replace(state, &m, d, &mut p, "Both <->").await?;
        p["saveArrow"] = key(state, &m, d, "Space", "Both \u{2194} ").await?;
        p["saveInput"] = insert(
            state,
            &m,
            d,
            "\"done\" ..",
            "Both \u{2194} \u{201c}done\u{201d} ..",
        )
        .await?;
        p["savePeriod"] = key(
            state,
            &m,
            d,
            "Period",
            "Both \u{2194} \u{201c}done\u{201d} \u{2026}",
        )
        .await?;
        data::mode(&state.peek(), data::Mode::HeldSuccess)?;
        p["enter"] = event(d, "key", json!({"key":"Enter"})).await?;
        wait_fixture(state, |v| v["held"] == true).await?;
        p["doubleSave"] = command(d, "double-save", Value::Null).await?;
        take(state, &m, d, "saving", &p, &mut rows, false).await?;
        p["release"] = data::release(&state.peek())?;
        closed(state, &m, d).await?;
        take(state, &m, d, "saved", &p, &mut rows, false).await?;
        wire::publish(&state.peek())?;
        p["actions"]
            .as_array_mut()
            .unwrap()
            .push(button(d, "slack-list-comments-refresh").await?);
        for _ in 0..400 {
            if wire::snapshot(&state.peek(), &m)["entryPage"]["raw"]["messages"][1]
                == wire::future_message()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        if wire::snapshot(&state.peek(), &m)["entryPage"]["raw"]["messages"][1]
            != wire::future_message()
        {
            return Err("Original smart-symbol readback missing".into());
        }
        wait_dom(
            d,
            "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
        )
        .await?;
        take(state, &m, d, "readback", &p, &mut rows, false).await?;
    }
    p["closedDisk"] = disk.snapshot().await?;
    Ok(
        json!({"ok":true,"case":if enabled{"own-edit-plain-smart-symbols"}else if disabled{"own-edit-plain-smart-symbols-disabled"}else{"own-edit-plain-smart-symbols-off"},"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"proof":p,"final":snapshot(state,&m),"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,"limits":["Original synthetic DOM keydown plus explicit default input; not OS keyboard or full IME automation.","Quotes are real insertText followed by the original deferred queue; no quote-specific keyboard binding or Delta injection.","Original substitution Undo/Redo literal suffix and accepted wire selector are checked independently; broader source vectors remain portable coverage."]}),
    )
}
