//! Actual missing-snippet opening, protected UTF-16 capture and section Save.
use super::super::{files as file, section_reference as source};
use super::*;
use crate::profile_edit::sections::rich;
mod retirement;
mod status;
pub(super) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(150),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"case":source::case(),"error":error,"source":source::snapshot(&state.peek()),"model":mentions::snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"case":source::case(),"error":"Section reference timeout"})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("profile-canvas-section-check: {result}")
        })
        .await;
    });
}
async fn settled(count: usize, held: bool) -> Result<(), String> {
    for _ in 0..100 {
        let s = source::status();
        if s["requests"].as_array().map(Vec::len) == Some(count)
            && s["held"] == held
            && s["active"] == if held { 1 } else { 0 }
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Section request did not settle: {}",
        source::status()
    ))
}
async fn no_editor(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich-pane')&&!!document.getElementById('profile-section-field-long')})").await.map(|_|())
}
async fn token_view(
    desktop: &DesktopContext,
    text: &str,
    reply: source::Reply,
) -> Result<Value, String> {
    let units = json!(source::expected(reply));
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById('profile-section-rich');if(!e)return{{ok:false}};const c=window.superPlatinumRichEditor.resolved(e),tokens=[...e.querySelectorAll('.rich-editor-token')],expected={units};const actual=tokens.map(t=>Array.from({{length:t.textContent.length}},(_,i)=>t.textContent.charCodeAt(i)));const icons=tokens.map(t=>{{const icon=t.querySelector(':scope > .rich-editor-section-symbol'),r=icon?.getBoundingClientRect();return{{section:t.classList.contains('rich-editor-section-canvas'),disabled:t.getAttribute('aria-disabled'),editable:t.contentEditable,icon:icon?{{hidden:icon.getAttribute('aria-hidden'),paths:[...icon.querySelectorAll('path')].map(p=>p.getAttribute('d')),width:r.width,height:r.height}}:null}};}});return{{ok:e.isContentEditable&&c.snapshot.blocks[0].elements[0].elements[0].text==={}&&tokens.length===2&&JSON.stringify(actual)===JSON.stringify(expected)&&JSON.stringify(c.tokens.map(t=>t.labelUnits))===JSON.stringify(expected)&&c.tokens.every(t=>!Object.hasOwn(t,'label')&&t.presentation?.kind==='canvas-section'&&t.presentation.icon==='description'&&t.presentation.disabled===false)&&icons.every(i=>i.section&&i.disabled==='false'&&i.editable==='false'&&i.icon?.width>0&&i.icon?.height>0),capture:c,units:actual,icons,editorEditable:e.isContentEditable}};}})()"#,json!(text))).await
}
async fn edit(desktop: &DesktopContext, text: &str, reply: source::Reply) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById('profile-section-rich'),leaf=e?.querySelector('.rich-editor-section > span:not(.rich-editor-token)');if(!e?.isContentEditable||!leaf)return{{ok:false}};leaf.textContent={};e.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText'}}));return{{ok:true}};}})()"#,json!(text))).await?;
    token_view(desktop, text, reply).await.map(|_| ())
}
async fn tamper(desktop: &DesktopContext) -> Result<Value, String> {
    wait_dom(desktop,r#"(()=>{const e=document.getElementById('profile-section-rich'),t=e?.querySelector('.rich-editor-section-canvas'),path=t?.querySelector('path'),text=[...t?.childNodes||[]].find(n=>n.nodeType===Node.TEXT_NODE);if(!path||!text)return{ok:false};const reject=()=>{try{window.superPlatinumRichEditor.resolved(e);return false;}catch{return true;}};const d=path.getAttribute('d');path.setAttribute('d','M0 0');const iconMutationRefused=reject();path.setAttribute('d',d);const old=text.data;let changed=false;for(let i=0;i<old.length;i++){if(old.charCodeAt(i)===0xd83d){text.data=old.slice(0,i)+String.fromCharCode(0xd83e)+old.slice(i+1);changed=true;break;}}const equalDisplayUnitMutationRefused=changed&&reject();text.data=old;t.classList.remove('rich-editor-section-canvas');const descriptorRemovalRefused=reject();t.classList.add('rich-editor-section-canvas');return{ok:iconMutationRefused&&equalDisplayUnitMutationRefused&&descriptorRemovalRefused&&!!window.superPlatinumRichEditor.resolved(e),iconMutationRefused,equalDisplayUnitMutationRefused,descriptorRemovalRefused,restored:true};})()"#).await
}
async fn shot(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    exercise: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    capture(state,desktop,name,stage,json!({"source":source::snapshot(&state.peek()),"cache":rich::files::fixture_status(&state.peek(),file::ID),"exercise":exercise}),stages,captures).await
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let ready = readiness(name).await?;
    if !source::named() || !file::enabled(&state.peek()) {
        return Err("Closed section owner missing".into());
    }
    if source::case() == "success" {
        source::rich::seed(state).await?;
    }
    select_values(state, desktop).await?;
    if source::case() == "status" {
        return status::run(state, desktop, name, ready).await;
    }
    let case = source::case();
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    source::configure(
        if case == "missing" {
            source::Reply::Missing
        } else {
            source::Reply::Present
        },
        true,
    );
    file::mode(file::Mode::Normal);
    click(desktop, "Edit description", false).await?;
    settled(1, true).await?;
    shot(
        state,
        desktop,
        name,
        "section-loading",
        json!({"held":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if case == "cancel" {
        click(desktop, "Cancel description edit", false).await?;
        no_editor(desktop).await?;
        for _ in 0..100 {
            if source::status()["cancelled"] == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        let release = source::release();
        settled(1, false).await?;
        if release != json!({"delivered":false,"cancelledBeforeReply":true}) {
            return Err("Cancelled section accepted reply".into());
        }
        shot(
            state,
            desktop,
            name,
            "cancelled",
            json!({"release":release}),
            &mut stages,
            &mut captures,
        )
        .await?;
        source::configure(source::Reply::Present, false);
        click(desktop, "Edit description", false).await?;
        settled(2, false).await?;
    } else {
        let release = source::release();
        settled(1, false).await?;
        if release["delivered"] != true {
            return Err("Section response not delivered".into());
        }
    }
    let initial = if case == "missing" {
        source::Reply::Missing
    } else {
        source::Reply::Present
    };
    let completion = if case == "success" {
        wait_dom(desktop, include_str!("section_reference/completion.js")).await?
    } else {
        Value::Null
    };
    let token = token_view(desktop, "Working with ", initial).await?;
    shot(
        state,
        desktop,
        name,
        "opened",
        json!({"token":token,"completion":completion}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if case == "retirement" {
        retirement::run(state, desktop, name, &mut stages, &mut captures).await?;
    } else {
        let mut final_reply = source::Reply::Present;
        if case == "success" || case == "failure" {
            edit(desktop, "Retained draft with ", source::Reply::Present).await?;
            source::configure(
                if case == "success" {
                    source::Reply::Changed
                } else {
                    source::Reply::Failure
                },
                false,
            );
            rich::files::sections::fixture_refresh(state)
                .await
                .map_err(|e| e.to_string())?;
            settled(2, false).await?;
            let token = token_view(desktop, "Retained draft with ", source::Reply::Present).await?;
            let proof = tamper(desktop).await?;
            shot(
                state,
                desktop,
                name,
                "refreshed-mounted",
                json!({"mountedLabelRetained":true,"token":token,"tamper":proof}),
                &mut stages,
                &mut captures,
            )
            .await?;
            if case == "success" {
                final_reply = source::Reply::Changed;
            } else {
                click(desktop, "Cancel description edit", false).await?;
                no_editor(desktop).await?;
                click(desktop, "Edit description", false).await?;
                settled(3, false).await?;
                let token = token_view(desktop, "Working with ", source::Reply::Missing).await?;
                shot(
                    state,
                    desktop,
                    name,
                    "failed-reopened",
                    json!({"errorClearedFile":true,"token":token}),
                    &mut stages,
                    &mut captures,
                )
                .await?;
            }
        }
        if case == "missing" || case == "failure" {
            click(desktop, "Cancel description edit", false).await?;
            no_editor(desktop).await?;
            source::configure(source::Reply::Present, false);
            click(desktop, "Edit description", false).await?;
            settled(if case == "failure" { 4 } else { 2 }, false).await?;
            let token = token_view(desktop, "Working with ", source::Reply::Present).await?;
            shot(
                state,
                desktop,
                name,
                "reopened-present",
                json!({"newOpeningRetried":true,"token":token}),
                &mut stages,
                &mut captures,
            )
            .await?;
        }
        edit(desktop, "Saved with ", source::Reply::Present).await?;
        use_description(state, desktop, "Saved with ").await?;
        shot(
            state,
            desktop,
            name,
            "used",
            json!({"rawTokensPreserved":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        save(state, desktop, Outcome::Saved, 1, 1).await?;
        click(desktop, "Edit description", false).await?;
        let token = token_view(desktop, "Saved with ", final_reply).await?;
        shot(
            state,
            desktop,
            name,
            "confirmed",
            json!({"savedAndReadBack":true,"token":token}),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":case,"canvasSection":true,"mentionKind":"canvas","titleBlocks":true,"readiness":ready,"stages":stages,"captures":captures,"viewport":stages[0]["geometry"]["viewport"],"captureChecksPassed":true,"files":file::status(),"sections":source::status(),"visualReview":"PNG inspection required separately"}),
    )
}
