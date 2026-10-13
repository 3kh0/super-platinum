//! Native forwarding checks never load a session or send a Slack request.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
static STARTED: AtomicBool = AtomicBool::new(false);
pub(crate) async fn run(state: Signal<ShellState>) {
    if !crate::fixture::is_fixture() || std::env::var_os("SUPER_PLATINUM_FORWARD_CHECK").is_none() {
        return;
    }
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        eprintln!("forward check failed: connected fixture");
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    if !name.starts_with("forward") {
        return;
    }
    // Start from the mounted dialog, after the native interpreter has applied its
    // initial DOM. A root future can queue a long eval before that handshake.
    if STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    let original_target = state.peek().forward_target.clone();
    let original_channel = state.peek().core.active_channel.clone();
    let mut eval = dioxus::document::eval(
        r#"
    await (async()=>{
        const name=await dioxus.recv();
        const report=async value=>{dioxus.send(value);return await dioxus.recv();};
        const pause=()=>new Promise(r=>setTimeout(r,90));
        const wait=async(fn,label)=>{for(let i=0;i<80;i++){if(fn())return;await pause();}throw Error(label);};
        const assert=(ok,label)=>{if(!ok)throw Error(label);};
        const stable=value=>JSON.stringify(value,(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))):v);
        const root=()=>document.getElementById('forward-dialog');
        const button=text=>[...root().querySelectorAll('button')].find(b=>b.textContent.trim()===text);
        const input=()=>document.getElementById('forward-destination');
        const note=()=>document.getElementById('forward-note');
        const key=(el,key,shiftKey=false)=>el.dispatchEvent(new KeyboardEvent('keydown',{key,shiftKey,bubbles:true,cancelable:true}));
        const query=async value=>{input().value=value;input().dispatchEvent(new Event('input',{bubbles:true}));await pause();};
        const option=()=>root().querySelector('[role=option]');
        const choose=async value=>{await query(value);await wait(option,'destination matches');key(input(),'Enter');await pause();};
        try {
            await wait(()=>root()&&note()?.textContent.includes('Worth a look'),'mounted rich note');
            await wait(()=>document.activeElement===input(),'destination gets initial focus');
            const original=stable(window.superPlatinumRichEditor.read('forward-note'));
            // Repeated native receipts exercise the fast eval completion race.
            for(let i=0;i<12;i++) {const reply=await report({stage:'note'});assert(stable(reply.note)===original,'native rich note receipt '+i);}
            assert(root().dataset.keyboardDialog==='true','focus trap enabled');
            const close=root().querySelector('[aria-label="Close forward dialog"]');
            const last=[...root().querySelectorAll('button:not([disabled])')].at(-1);
            last.focus();key(last,'Tab');await pause();assert(document.activeElement===close,'Tab wraps inside dialog');
            if(name.includes('partial')) {
                assert(root().querySelectorAll('[data-delivery]').length===3,'three destination receipts');
                assert(root().querySelector('.forward-result[data-destination=C1]').dataset.delivery==='sent','success retained');
                assert(root().querySelector('.forward-result[data-destination=C2]').dataset.delivery==='failed','rejection shown');
                assert(root().querySelector('.forward-result[data-destination=C5]').dataset.delivery==='uncertain','uncertainty shown');
                assert(note().getAttribute('contenteditable')==='false','uncertain note frozen');
                await query('general');assert(!option(),'successful destination cannot be re-added');
                await query('private');assert(![...root().querySelectorAll('[role=option]')].some(b=>b.dataset.destination==='C5'),'uncertain destination excluded');
                await choose('ship');assert(root().querySelectorAll('.forward-chip').length===1,'failed destination may be selected');
                button('Resend…').click();await wait(()=>button('Cancel')===document.activeElement,'resend confirmation focused');
                assert(root().textContent.includes('could create a duplicate'),'resend warning');
                button('Cancel').click();await wait(()=>button('Check delivery'),'resend cancelled');
                button('Check delivery').click();await wait(()=>root().querySelector('[role=alert]')?.textContent.includes('Connect to Slack'),'offline check retained');
                button('Resend…').click();await wait(()=>button('Resend original'),'explicit resend confirmation');
                button('Resend original').click();await wait(()=>button('Check delivery')&&!button('Check delivery').disabled,'offline resend finishes');
                assert(root().querySelector('.forward-result[data-destination=C5]').dataset.delivery==='uncertain','offline resend preserves uncertainty');
            } else if(name.includes('people')) {
                await query('alex');await wait(()=>root().querySelector('[data-destination="person:UREMOTE1"]'),'remote directory result');
                await choose('alex');assert(root().querySelector('.forward-chip').textContent.includes('Alex Rivera'),'remote person selected');
                const reply=await report({stage:'person'});assert(reply.hydrated&&reply.no_dm&&reply.source_kept,'selection hydrates without opening DM or navigating');
                await query('alex');assert(!root().querySelector('[data-destination="person:UREMOTE1"]'),'selected person excluded');
                await choose('#ship');assert(root().querySelectorAll('.forward-chip').length===2,'mixed person and channel recipients');
                await query('@maya');assert(root().querySelectorAll('[role=option]').length===1,'existing DM deduplicated');
                await query('#maya');assert(!option(),'channel prefix excludes people and DMs');
            } else {
                await choose('ship');await choose('general');assert(root().querySelectorAll('.forward-chip').length===2,'multi destination selection');
                await query('ship');assert(!option(),'selected destination excluded');
                assert(note().getAttribute('contenteditable')==='true','note remains editable');
            }
            button('Forward').click();await wait(()=>root().querySelector('[role=alert]')?.textContent.includes('Connect to Slack')&&!button('Forward').disabled,'offline submission retains dialog');
            assert(stable(window.superPlatinumRichEditor.read('forward-note'))===original,'note retained through failures');
            assert(root().querySelectorAll('.forward-chip').length===(name.includes('partial')?1:2),'destinations retained');
            const d=root().getBoundingClientRect();assert(d.left>=0&&d.right<=innerWidth+1&&d.bottom<=innerHeight+1,'dialog fits viewport');
            assert(root().scrollWidth<=root().clientWidth+1,'no horizontal overflow');
            const header=close.getBoundingClientRect(),footer=button('Forward').getBoundingClientRect();assert(header.top>=d.top&&footer.bottom<=d.bottom+1,'header and footer remain visible');
            // Escape must close only this dialog, including from the rich editor.
            note().focus();key(note(),'Escape');await wait(()=>!root(),'Escape dismisses dialog');
            await report({stage:'restore'});await wait(root,'restored capture fixture');
            await wait(()=>document.activeElement===input(),'destination focus restored on reopen');
            if(name.includes('partial')) {button('Resend…').click();await wait(()=>button('Cancel')===document.activeElement,'capture confirmation focused');}
            await pause();await report({ok:true});
        } catch(error) {await report({ok:false,case:error.message});}
    })();
    "#,
    );
    let _ = eval.send(name.clone());
    loop {
        let receipt = eval.recv::<serde_json::Value>().await.unwrap_or_default();
        match receipt["stage"].as_str() {
            Some("note") => {
                let note = read_note("fixture-forward").await;
                let _ = eval.send(serde_json::json!({"note":note}));
                continue;
            }
            Some("person") => {
                let shell = state.peek();
                let ws = &shell.core.workspaces[shell.core.active_team.as_ref().unwrap()];
                let _=eval.send(serde_json::json!({"hydrated":ws.users.contains_key("UREMOTE1"),"no_dm":!ws.channels.values().any(|c|c.user.as_deref()==Some("UREMOTE1")),"source_kept":shell.core.active_channel==original_channel}));
                continue;
            }
            Some("restore") => {
                let mut state = state;
                let mut shell = state.write();
                shell.forward_target = original_target.clone();
                shell.overlay = Some(Overlay::Forward);
            }
            _ => {
                let _ = eval.send("received");
                eprintln!(
                    "forward check {}: {}",
                    if receipt["ok"] == true {
                        "passed"
                    } else {
                        "failed"
                    },
                    receipt
                );
                break;
            }
        }
        let _ = eval.send("received");
    }
}
