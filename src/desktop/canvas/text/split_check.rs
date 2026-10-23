//! Actual browser Enter handshake; only the owned external runner sends the key.
use super::check::{checked, click, editor_layout, wait};
use super::split_fixture as fixture;
use super::*;
use serde_json::{Value, json};
#[path = "split_check_middle.rs"]
mod middle;
#[path = "split_check_repeat.rs"]
mod repeat;
#[path = "split_check_sequence.rs"]
mod sequence;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::requested() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => json!({"ok":false,"error":e}),
        };
        if tokio::task::spawn_blocking(move || eprintln!("canvas-split-check: {receipt}"))
            .await
            .is_err()
        {
            eprintln!(
                "canvas-split-check: {}",
                json!({"ok":false,"error":"Split receipt worker failed"})
            );
        }
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if !fixture::corpus_valid()
        || !fixture::mode_valid()
        || !fixture::typing_valid()
        || !fixture::reuse_valid()
        || !fixture::parent_valid()
        || !fixture::empty_valid()
        || !fixture::repeat_valid()
        || !fixture::sequence::valid()
    {
        return Err("Unknown split corpus".into());
    }
    if fixture::is_sequence() && fixture::sequence::count() < 2 {
        return Err(
            "Count-one uses the existing production end path; not a native sequence scenario"
                .into(),
        );
    }
    let corpus = fixture::corpus();
    let scenario = fixture::scenario();
    if !matches!(scenario.as_str(), "success" | "stale" | "partial") {
        return Err("Unknown split scenario".into());
    }
    wait(desktop,&format!(r#"({{ok:!!document.querySelector('[data-canvas-text-section={} ] .canvas-text-open:not(:disabled)')}})"#,json!(fixture::parent_id()))).await?;
    if !fixture::enabled(&state.peek()) {
        return Err("Expected disconnected split owner".into());
    }
    let original = fixture::proof(&state.peek());
    let initial_journal = checklist::recovery::split_fixture_journal(state)
        .await
        .map_err(str::to_owned)?;
    let mut stages = Vec::new();
    let cancelled_witness = enter(desktop, 0).await?;
    stage(
        state,
        desktop,
        if fixture::is_sequence() {
            "sequence"
        } else if fixture::is_repeat() {
            if fixture::typing() == "none" {
                "repeat"
            } else {
                "typed-third"
            }
        } else if fixture::is_empty() && fixture::typing() == "none" {
            "empty"
        } else if fixture::is_start_typing() {
            "typed-original"
        } else if fixture::is_start() {
            "start"
        } else if fixture::typing() == "none" {
            "split"
        } else {
            "typed-child"
        },
        0,
        &mut stages,
    )
    .await?;
    click(desktop, "#canvas-text-cancel").await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    stage(state, desktop, "cancelled", 1, &mut stages).await?;
    if fixture::proof(&state.peek()) != original {
        return Err("Cancel changed canonical split source".into());
    }
    let saved_witness = enter(desktop, 1).await?;
    if scenario == "stale" && !fixture::change_preflight(state) {
        return Err("Could not stage stale split source".into());
    }
    let duplicate=checked(desktop,r#"(()=>{const b=document.getElementById('canvas-text-save');if(!b||b.disabled)return {ok:false};b.click();b.click();return {ok:true,clicks:2};})()"#).await?;
    if scenario == "stale" {
        wait(desktop,r#"(()=>{const r=document.getElementById('canvas-text-editor'),f=document.getElementById('canvas-text-draft');return {ok:f?.isContentEditable&&!document.getElementById('canvas-text-cancel')?.disabled&&r?.querySelector(':scope > [role=alert]')?.textContent==='This paragraph or your access changed. Cancel and refresh.'};})()"#).await?;
        stage(state, desktop, "refused", 2, &mut stages).await?;
        click(desktop, "#canvas-text-cancel").await?;
        wait(
            desktop,
            r#"({ok:!document.getElementById('canvas-text-editor')})"#,
        )
        .await?;
        stage(state, desktop, "cancelled-stale", 3, &mut stages).await?;
    } else {
        wait(desktop,r#"(()=>{const f=document.getElementById('canvas-text-draft');return {ok:f?.contentEditable==='false'&&document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled};})()"#).await?;
        // Reservation and dispatch are asynchronous; readonly begins before disk.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        while fixture::proof(&state.peek())["pending"] != true {
            if tokio::time::Instant::now() >= deadline {
                return Err("Split did not dispatch after reservation".into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        stage(state, desktop, "saving", 2, &mut stages).await?;
        if !fixture::complete(state).await {
            return Err(format!(
                "Split pair completion failed; ACK evidence: {}",
                fixture::proof(&state.peek())["acknowledgements"]
            ));
        }
        if scenario == "partial" {
            wait(desktop,r#"({ok:document.querySelector('#canvas-text-save .canvas-text-save-label')?.textContent==='Unconfirmed'})"#).await?;
            stage(state, desktop, "uncertain", 3, &mut stages).await?;
        } else {
            wait(desktop,&format!(r#"(()=>{{const rows=[...document.querySelectorAll('[data-canvas-text-section] .canvas-paragraph-content')].map(n=>n.textContent);return {{ok:!document.getElementById('canvas-text-editor')&&rows.includes({})&&rows.includes({})}};}})()"#,json!(fixture::parent_text()),json!(if fixture::is_sequence(){fixture::sequence::final_text(0)}else{fixture::child_text().into()}))).await?;
            stage(state, desktop, "confirmed", 3, &mut stages).await?;
            if fixture::is_sequence() {
                sequence::continue_members(state, desktop, &mut stages).await?;
            } else if fixture::is_repeat() {
                repeat::continue_three(state, desktop, &mut stages).await?;
            } else if fixture::is_middle_typing() || fixture::is_start() || fixture::is_empty() {
                middle::continue_pair(state, desktop, &mut stages).await?;
            } else {
                let child = fixture::proof(&state.peek())["writes"][0]["child"]
                    .as_str()
                    .ok_or("Missing created child identity")?
                    .to_owned();
                click(
                    desktop,
                    &format!(
                        "[data-canvas-text-section={}] .canvas-text-open",
                        json!(child)
                    ),
                )
                .await?;
                wait(desktop,&format!(r#"(()=>{{const f=document.getElementById('canvas-text-draft');return {{ok:f?.isContentEditable&&f.textContent==={}}};}})()"#,json!(if fixture::is_sequence(){fixture::sequence::final_text(0)}else{fixture::child_text().into()}))).await?;
                stage(state, desktop, "child-editing", 4, &mut stages).await?;
                click(desktop, "#canvas-text-cancel").await?;
                wait(
                    desktop,
                    r#"({ok:!document.getElementById('canvas-text-editor')})"#,
                )
                .await?;
                click(
                    desktop,
                    &format!(
                        "[data-canvas-text-section={}] .canvas-text-open",
                        json!(if fixture::is_end() {
                            child.as_str()
                        } else {
                            "paragraph"
                        })
                    ),
                )
                .await?;
                wait(desktop,&format!(r#"(()=>{{const f=document.getElementById('canvas-text-draft');return {{ok:f?.isContentEditable&&f.textContent==={}}};}})()"#,json!(if fixture::is_end(){fixture::child_text()}else{corpus.left}))).await?;
                super::check::set_draft(desktop, &fixture::edited_html()).await?;
                click(desktop, "#canvas-text-save").await?;
                wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled})"#).await?;
                let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
                loop {
                    let proof = fixture::proof(&state.peek());
                    if proof["pending"] == true
                        && proof["writes"].as_array().map(Vec::len) == Some(2)
                    {
                        break;
                    }
                    if tokio::time::Instant::now() >= deadline {
                        return Err("Text continuation did not dispatch".into());
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                stage(state, desktop, "continued-saving", 5, &mut stages).await?;
                if !fixture::complete_text(&mut state.write()) {
                    let proof = fixture::proof(&state.peek());
                    let ack = proof["acknowledgements"]
                        .as_array()
                        .and_then(|rows| rows.last());
                    return Err(format!(
                        "Text continuation failed (ACK accepted={}, error={})",
                        ack.map(|v| &v["accepted"]).unwrap_or(&Value::Null),
                        ack.map(|v| &v["error"]).unwrap_or(&Value::Null)
                    ));
                }
                wait(desktop,&format!(r#"(()=>{{const rows=[...document.querySelectorAll('[data-canvas-text-section] .canvas-paragraph-content')].map(n=>n.textContent);return {{ok:!document.getElementById('canvas-text-editor')&&rows.includes({})&&rows.includes({})}};}})()"#,json!(if fixture::is_end(){corpus.left}else{fixture::continued_text()}),json!(if fixture::is_end(){fixture::continued_text()}else{fixture::child_text()}))).await?;
                stage(state, desktop, "continued-confirmed", 6, &mut stages).await?;
            }
        }
    }
    let final_source = fixture::proof(&state.peek());
    let final_journal = checklist::recovery::split_fixture_journal(state)
        .await
        .map_err(str::to_owned)?;
    let layout = checked(desktop, include_str!("split_check.js")).await?;
    let mut receipt = json!({"ok":true,"noSession":true,"noTransport":true,"scenario":scenario,"corpus":corpus.name,"mode":fixture::mode(),"typing":fixture::typing(),"parentMode":fixture::parent_mode(),"sequenceCount":fixture::sequence::count(),"sequenceHistory":fixture::sequence::history(),"sequenceLayout":fixture::sequence::layout(),"sequenceCadence":fixture::sequence::cadence(),"repeatHistory":fixture::repeat_history(),"repeatLayout":fixture::repeat_layout(),"emptyHistory":fixture::empty_history(),"emptyLayout":fixture::empty_layout(),"reuseBase":fixture::reuse_base(),"reuseContent":fixture::reuse_content(),"reuseChild":fixture::is_reuse().then(fixture::reuse_child),
        "httpFactoryExecuted":false,"trustedNativeEnter":true,"original":original,"initialJournal":initial_journal,"finalJournal":final_journal,
        "cancelledWitness":cancelled_witness,"savedWitness":saved_witness,"duplicate":duplicate,
        "fixture":final_source,"layout":layout});
    // Retain the complete ledger without cloning its growing nested Values on the UI task.
    receipt["stages"] = Value::Array(stages);
    Ok(receipt)
}
async fn enter(desktop: &dioxus::desktop::DesktopContext, ordinal: usize) -> Result<Value, String> {
    let corpus = fixture::corpus();
    click(
        desktop,
        &format!(
            "[data-canvas-text-section={}] .canvas-text-open",
            json!(fixture::parent_id())
        ),
    )
    .await?;
    wait(desktop,r#"(()=>{const f=document.getElementById('canvas-text-draft');try{return {ok:f?.isContentEditable&&document.activeElement===f&&window.superPlatinumRichEditor.snapshot(f).revision===0&&!!window.superPlatinumCanvasSplit};}catch{return {ok:false}}})()"#).await?;
    editor_layout(desktop).await?;
    let nonce = std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_NONCE")
        .map_err(|_| "Missing split input nonce")?;
    if nonce.len() != 32 || !nonce.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid split input nonce".into());
    }
    let ready = if fixture::is_empty() {
        checked(desktop,&format!(r#"(()=>{{const f=document.getElementById('canvas-text-draft'),r=document.getElementById('canvas-text-editor'),s=window.superPlatinumRichEditor.snapshot(f),section=f.querySelector('.rich-editor-section');if(s.revision!==0||f.textContent!==''||!section)return {{ok:false}};const range=document.createRange();range.setStart(section,0);range.collapse(true);f.focus();getSelection().removeAllRanges();getSelection().addRange(range);f.dataset.canvasSplitProbe={};return {{ok:true,generation:r.dataset.editorGeneration,visit:r.dataset.canvasVisit,revision:s.revision}};}})()"#,json!(format!("{nonce}:{ordinal}")))).await?
    } else {
        checked(desktop,&format!(r#"(()=>{{const f=document.getElementById('canvas-text-draft'),root=document.getElementById('canvas-text-editor');const s=window.superPlatinumRichEditor.snapshot(f);if(s.revision!==0||f.textContent!=={})return {{ok:false}};const w=document.createTreeWalker(f,NodeFilter.SHOW_TEXT),node=w.nextNode();if(!node||node.data.length<{})return {{ok:false}};const range=document.createRange();range.setStart(node,{});range.collapse(true);f.focus();getSelection().removeAllRanges();getSelection().addRange(range);f.dataset.canvasSplitProbe={};return {{ok:true,generation:root.dataset.editorGeneration,visit:root.dataset.canvasVisit,revision:s.revision}};}})()"#,json!(corpus.seed),corpus.offset,corpus.offset,json!(format!("{nonce}:{ordinal}")))).await?
    };
    eprintln!(
        "canvas-split-enter: {}",
        json!({"ok":true,"nonce":nonce,"ordinal":ordinal,
        "fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"generation":ready["generation"],
        "visit":ready["visit"],"fieldId":"canvas-text-draft","offset":corpus.offset,"seed":corpus.seed,"corpus":corpus.name,"mode":fixture::mode(),"typing":fixture::typing(),"parentMode":fixture::parent_mode(),"sequenceCount":fixture::sequence::count(),"sequenceHistory":fixture::sequence::history(),"sequenceLayout":fixture::sequence::layout(),"sequenceCadence":fixture::sequence::cadence(),"repeatHistory":fixture::repeat_history(),"repeatLayout":fixture::repeat_layout(),"emptyHistory":fixture::empty_history(),"emptyLayout":fixture::empty_layout(),"revision":0})
    );
    if fixture::is_sequence() {
        let reports = fixture::sequence::reports();
        let finals = reports
            .iter()
            .map(|r| r.last().cloned().unwrap_or_default())
            .collect::<Vec<_>>();
        let proof=wait(desktop,&format!(r#"(()=>{{const f=document.getElementById('canvas-text-draft'),r=document.getElementById('canvas-text-editor');const witness=window.superPlatinumCanvasSplit.capture(f,r.dataset.editorGeneration,r.dataset.canvasVisit);const sequence=witness?.sequence;return {{ok:witness?.kind==='sequence'&&f.dataset.canvasSequenceComplete==={}&&sequence?.events.filter(e=>e.kind==='enter').length==={}&&JSON.stringify(sequence.after[0].elements.slice(1).map(s=>s.elements.map(r=>r.text).join('')))===JSON.stringify({}),witness}};}})()"#,json!(format!("{nonce}:{ordinal}")),fixture::sequence::count(),json!(finals))).await?;
        return Ok(proof["witness"].clone());
    }
    if fixture::is_repeat() {
        let proof=wait(desktop,&format!(r#"(()=>{{const f=document.getElementById('canvas-text-draft'),r=document.getElementById('canvas-text-editor');const witness=window.superPlatinumCanvasSplit.capture(f,r.dataset.editorGeneration,r.dataset.canvasVisit);const events=witness?.typing?.events;return {{ok:witness?.repeat?.revision===2&&f.dataset.canvasRepeatComplete==={}&&({}===0?!witness.typing:events?.length==={}&&events.at(-1).afterText==={}&&f.dataset.canvasTypingComplete==={}),witness}};}})()"#,json!(format!("{nonce}:{ordinal}")),fixture::typing_reports().len(),fixture::typing_reports().len(),json!(fixture::typing_final_text()),json!(format!("{nonce}:{ordinal}")))).await?;
        return Ok(proof["witness"].clone());
    }
    let proof=wait(desktop,&format!(r#"(()=>{{const f=document.getElementById('canvas-text-draft'),r=document.getElementById('canvas-text-editor');const witness=window.superPlatinumCanvasSplit.capture(f,r.dataset.editorGeneration,r.dataset.canvasVisit);const events=witness?.typing?.events;return {{ok:!!witness&&(!{}||f.dataset.canvasEmptyComplete==={})&&({}===0?!witness.typing:events?.length==={}&&events.at(-1).afterText==={}&&f.dataset.canvasTypingComplete==={}),witness}};}})()"#,fixture::is_empty(),json!(format!("{nonce}:{ordinal}")),fixture::typing_reports().len(),fixture::typing_reports().len(),json!(fixture::typing_final_text()),json!(format!("{nonce}:{ordinal}")))).await?;
    Ok(proof["witness"].clone())
}
async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    ordinal: usize,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    if !fixture::enabled(&state.peek()) {
        return Err("Split fixture lost disconnected owner".into());
    }
    let source = fixture::proof(&state.peek());
    let journal = checklist::recovery::split_fixture_journal(state)
        .await
        .map_err(str::to_owned)?;
    let geometry = wait(desktop, include_str!("split_check.js")).await?;
    let editor = if geometry["native"].is_object() {
        editor_layout(desktop).await?
    } else {
        Value::Null
    };
    if fixture::proof(&state.peek()) != source {
        return Err("Split source changed while capturing geometry".into());
    }
    let receipt = json!({"ok":true,"stage":name,"ordinal":ordinal,"noSession":true,"noTransport":true,"fixture":source,"journal":journal,"geometry":geometry,"editorLayout":editor});
    let directory = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
        .ok_or("Missing capture directory")?;
    let directory = std::path::PathBuf::from(directory);
    if !directory.is_absolute() {
        return Err("Capture directory must be absolute".into());
    }
    let ack = directory.join(format!("{ordinal}.ack"));
    let receipt = tokio::task::spawn_blocking(move || {
        eprintln!("canvas-split-stage: {receipt}");
        receipt
    })
    .await
    .map_err(|_| "Split stage receipt worker failed")?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let path = ack.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Capture ACK worker failed")?
        {
            Ok(bytes) if bytes == b"captured\n" => break,
            Ok(_) => return Err("Malformed capture ACK".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Capture ACK read failed".into()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("Capture ACK timed out at {name}"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    if geometry != checked(desktop, include_str!("split_check.js")).await?
        || source != fixture::proof(&state.peek())
    {
        return Err(format!("Split source/layout changed during {name} capture"));
    }
    stages.push(receipt);
    Ok(())
}
