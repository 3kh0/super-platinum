//! Actual DOM events through the production section handlers; fixture transport only.
use super::*;
use crate::slack_links_check::wait_dom;
use dioxus::desktop::DesktopContext;
use serde_json::json;

pub(crate) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    if !interaction_enabled(&state.peek())
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("profile-sections-save" | "profile-sections-save-narrow")
        )
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let result = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("profile-sections-check: {result}");
    });
}

async fn run(mut state: Signal<ShellState>, desktop: &DesktopContext) -> Result<Value, String> {
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    if !matches!(
        name.as_str(),
        "profile-sections-save" | "profile-sections-save-narrow"
    ) {
        return Err("Section save check requires its named disconnected fixture".into());
    }
    let mut stages = Vec::new();
    field(desktop, "Product engineering", false).await?;
    input(desktop, "Saved focus").await?;
    click(desktop, "Save section", true).await?;
    field(desktop, "Saved focus", true).await?;
    expect_counts(&state.peek(), 1, 0)?;
    complete(&mut state.write());
    field(desktop, "Saved focus", false).await?;
    wait_dom(desktop, r#"({ok:document.querySelector('.profile-edit-notice')?.textContent==='Profile fields saved.'&&!Array.from(document.querySelectorAll('button')).some(b=>b.textContent==='Keep current section')})"#).await?;
    expect_counts(&state.peek(), 1, 1)?;
    capture(desktop, &name, "01-confirmed", &mut stages).await?;

    state
        .write()
        .profile_edit
        .sections
        .fixture
        .as_mut()
        .unwrap()
        .outcome = Outcome::Rejected;
    input(desktop, "Retained rejected edit").await?;
    click(desktop, "Save section", false).await?;
    field(desktop, "Retained rejected edit", true).await?;
    complete(&mut state.write());
    field(desktop, "Retained rejected edit", false).await?;
    wait_dom(desktop, r#"({ok:document.querySelector('.profile-edit-notice')?.textContent.includes('draft is retained')})"#).await?;
    expect_counts(&state.peek(), 2, 1)?;
    capture(desktop, &name, "02-rejected", &mut stages).await?;

    state
        .write()
        .profile_edit
        .sections
        .fixture
        .as_mut()
        .unwrap()
        .outcome = Outcome::Uncertain;
    input(desktop, "Retained unconfirmed edit").await?;
    click(desktop, "Save section", true).await?;
    field(desktop, "Retained unconfirmed edit", true).await?;
    complete(&mut state.write());
    wait_dom(desktop, r#"({ok:document.querySelector('.profile-edit-notice')?.textContent.includes('not confirmed')&&Array.from(document.querySelectorAll('.profile-edit button')).some(b=>b.textContent==='Check profile'&&!b.disabled)})"#).await?;
    capture(desktop, &name, "03-uncertain", &mut stages).await?;
    click(desktop, "Check profile", false).await?;
    field(desktop, "Retained unconfirmed edit", true).await?;
    wait_dom(desktop, r#"({ok:document.querySelector('.profile-edit-notice')?.textContent.includes('different values')})"#).await?;
    expect_counts(&state.peek(), 3, 2)?;
    {
        let shell = state.peek();
        let view = presentation(&shell).ok_or("Missing uncertain section")?;
        if !view.uncertain
            || !view.can_keep
            || view.can_save
            || !view.form.as_ref().is_some_and(|f| f.dirty())
        {
            return Err("Mismatched readback discarded or unlocked the proposed edit".into());
        }
    }
    capture(desktop, &name, "04-mismatch", &mut stages).await?;
    click(desktop, "Check profile", false).await?;
    field(desktop, "Retained unconfirmed edit", true).await?;
    // A repeated mismatch has identical DOM content. Wait for the actual
    // handler's read count, rather than mistaking the old render for completion.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let reads = state
            .peek()
            .profile_edit
            .sections
            .fixture
            .as_ref()
            .unwrap()
            .reads;
        if reads >= 3 {
            expect_counts(&state.peek(), 3, 3)?;
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err("Repeated Check did not reach the section read handler".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    click(desktop, "Keep current section", false).await?;
    wait_dom(desktop, r#"({ok:!document.getElementById('profile-sections-title')&&!!document.getElementById('profile-edit-form')})"#).await?;
    if pending(&state.peek()) {
        return Err("Explicit Keep did not retire the uncertain save".into());
    }
    let visit = state.peek().profile_edit.draft.as_ref().unwrap().visit;
    super::super::open(state, visit);
    click(desktop, "About you", false).await?;
    field(desktop, "Saved focus", false).await?;
    expect_counts(&state.peek(), 3, 3)?;
    wait_dom(
        desktop,
        r#"({ok:!document.querySelector('.profile-edit-notice')})"#,
    )
    .await?;
    capture(desktop, &name, "05-keep-reopened", &mut stages).await?;
    let layout = wait_dom(desktop, LAYOUT).await?;
    let shell = state.peek();
    let fixture = shell.profile_edit.sections.fixture.as_ref().unwrap();
    Ok(
        json!({"ok":true,"duplicateGuard":true,"confirmedSave":true,"rejectionRetainsDraft":true,
        "mismatchRetainsDraft":true,"repeatedCheckNoReplay":true,"explicitKeep":true,
        "canonicalReopen":true,"preparedRequests":fixture.attempts.len(),"reads":fixture.reads,
        "noSession":shell.core.session.is_none(),"noTransport":shell.core.transport.is_none(),
        "viewport":layout["viewport"],"layout":layout,"stages":stages,"stageLayouts":true}),
    )
}

async fn capture(
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    // Keep the proposed field visible in each capture. The scrollable section
    // may contain more fields than the narrow viewport; its footer stays fixed.
    wait_dom(desktop, r#"(()=>{const e=document.getElementById('profile-section-field-text');if(!e)return{ok:false};e.scrollIntoView({block:'nearest'});return{ok:true};})()"#).await?;
    let native = wait_dom(desktop, LAYOUT).await?;
    let directory =
        std::env::var("SUPER_PLATINUM_UI_CAPTURE_DIR").unwrap_or_else(|_| "tmp/agent-ui".into());
    let path = std::path::Path::new(&directory).join(format!("{name}-{stage}.png"));
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    stages.push(json!({"stage":stage,"path":path,"width":width,"height":height,"native":native}));
    Ok(())
}

fn expect_counts(shell: &ShellState, attempts: usize, reads: usize) -> Result<(), String> {
    let fixture = shell
        .profile_edit
        .sections
        .fixture
        .as_ref()
        .ok_or("Fixture disappeared")?;
    if fixture.attempts.len() != attempts || fixture.reads != reads {
        return Err(format!(
            "Expected {attempts} writes/{reads} reads, got {}/{}",
            fixture.attempts.len(),
            fixture.reads
        ));
    }
    if shell.core.session.is_some() || shell.core.transport.is_some() {
        return Err("Offline section check acquired a transport".into());
    }
    Ok(())
}

async fn field(desktop: &DesktopContext, value: &str, disabled: bool) -> Result<(), String> {
    let value = serde_json::to_string(value).unwrap();
    wait_dom(desktop, &format!(r#"(()=>{{const e=document.getElementById('profile-section-field-text');return{{ok:!!e&&e.value==={value}&&e.disabled==={disabled}}};}})()"#)).await?;
    Ok(())
}

async fn input(desktop: &DesktopContext, value: &str) -> Result<(), String> {
    let value = serde_json::to_string(value).unwrap();
    wait_dom(desktop, &format!(r#"(()=>{{const e=document.getElementById('profile-section-field-text');if(!e||e.disabled)return{{ok:false}};e.focus();e.value={value};e.dispatchEvent(new Event('input',{{bubbles:true}}));return{{ok:true}};}})()"#)).await?;
    Ok(())
}

async fn click(desktop: &DesktopContext, label: &str, twice: bool) -> Result<(), String> {
    let label = serde_json::to_string(label).unwrap();
    wait_dom(desktop, &format!(r#"(()=>{{const b=Array.from(document.querySelectorAll('.profile-edit button')).find(b=>b.textContent==={label});if(!b||b.disabled)return{{ok:false}};b.click();if({twice})b.click();return{{ok:true}};}})()"#)).await?;
    Ok(())
}

const LAYOUT: &str = r#"(()=>{
    const d=document.querySelector('.profile-edit'),r=d?.getBoundingClientRect();
    const body=d?.querySelector('.profile-edit-body'),br=body?.getBoundingClientRect();
    const footer=d?.querySelector('footer'),footerRect=footer?.getBoundingClientRect();
    const field=document.getElementById('profile-section-field-text'),fr=field?.getBoundingClientRect();
    const locked=document.getElementById('profile-section-field-locked');
    const buttons=Array.from(footer?.querySelectorAll('button')||[]);
    const notices=Array.from(d?.querySelectorAll('.profile-edit-notice')||[]).filter(p=>p.textContent.trim());
    const insideFooter=e=>{
        const q=e.getBoundingClientRect(),style=getComputedStyle(e);
        return !!r&&!!footerRect&&q.width>0&&q.height>0&&style.visibility!=='hidden'&&style.display!=='none'
            &&q.left>=r.left&&q.right<=r.right+1&&q.top>=footerRect.top&&q.bottom<=footerRect.bottom+1
            &&q.top>=r.top&&q.bottom<=r.bottom+1&&q.top>=0&&q.bottom<=innerHeight+1;
    };
    const noticesVisible=notices.every(insideFooter);
    return{ok:!!r&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1
        &&d.scrollWidth<=d.clientWidth+1&&!!fr&&fr.width>0&&fr.top>=br.top-1&&fr.bottom<=br.bottom+1
        &&locked?.disabled&&locked.value==='Preserved organization value'&&buttons.every(insideFooter)&&noticesVisible,
        viewport:[innerWidth,innerHeight],width:innerWidth,height:innerHeight,fieldValue:field?.value,
        fieldDisabled:field?.disabled,notice:notices[0]?.textContent||'',noticesVisible,
        noticeCount:notices.length,managedFieldPreserved:locked?.value==='Preserved organization value'};
})()"#;
