//! Disconnected signed-native recovery checks with task-owned injected storage.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use super_platinum_core::forwarding::{Status, store::Begin};

static STARTED: AtomicBool = AtomicBool::new(false);
const ACCOUNT: &str = "00000000-0000-4000-8000-000000000071";
const DESTINATION: &str = "C_FORWARD_RECOVERY_MISSING";
const NOTE: &str = "Recovered note · 日本語 🚀";

/// Called before loading a source so the seed cannot lose a race with the UI.
/// Reopening the dialog in the seed process must never replace its first ID.
pub(super) async fn seed(
    storage: &recovery::Storage,
    target: &ForwardTarget,
) -> Result<(), String> {
    if !crate::fixture::is_fixture()
        || std::env::var("SUPER_PLATINUM_FORWARD_RECOVERY_CHECK").as_deref() != Ok("seed")
        || !std::env::var("SUPER_PLATINUM_FIXTURE")
            .is_ok_and(|name| name.starts_with("forward-recovery"))
    {
        return Ok(());
    }
    if std::env::var_os("SUPER_PLATINUM_FORWARD_RECOVERY_DB").is_none() {
        return Err("Recovery fixture requires isolated storage.".into());
    }
    if !storage.source(target).await?.is_empty() {
        return Ok(());
    }
    let blocks = vec![
        serde_json::json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":NOTE,"style":{"bold":true}},
            {"type":"text","text":"\nSecond line kept."}
        ]}]}),
    ];
    let request = api::ForwardMessage::new(
        target.channel.clone(),
        target.message.ts.clone(),
        DESTINATION.into(),
        blocks,
    );
    match storage
        .begin(request, "Recovered destination · 日本語".into())
        .await?
    {
        Begin::Started(_) | Begin::Blocked(_) => Ok(()),
    }
}

pub(crate) async fn run(state: Signal<ShellState>) {
    if !crate::fixture::is_fixture()
        || std::env::var_os("SUPER_PLATINUM_FORWARD_RECOVERY_CHECK").is_none()
        || STARTED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    let mode = std::env::var("SUPER_PLATINUM_FORWARD_RECOVERY_CHECK").unwrap_or_default();
    let (storage, target) = {
        let shell = state.peek();
        if shell.core.session.is_some()
            || shell.core.transport.is_some()
            || shell.core.active_account.as_deref() != Some(ACCOUNT)
            || !matches!(mode.as_str(), "seed" | "restart" | "retire")
            || std::env::var_os("SUPER_PLATINUM_FORWARD_RECOVERY_DB").is_none()
        {
            eprintln!("forward recovery check failed: fixture isolation");
            return;
        }
        let (Some(storage), Some(target)) = (
            recovery::Storage::capture(&shell),
            shell.forward_target.clone(),
        ) else {
            eprintln!("forward recovery check failed: missing fixture context");
            return;
        };
        (storage, target)
    };
    if let Err(error) = seed(&storage, &target).await {
        eprintln!("forward recovery check failed: {error}");
        return;
    }
    let original = match storage.source(&target).await {
        Ok(receipts) if receipts.len() == 1 && receipts[0].unresolved() => receipts[0].clone(),
        _ => {
            eprintln!("forward recovery check failed: expected one unresolved receipt");
            return;
        }
    };
    let mut eval = document::eval(
        r#"
    await(async()=>{
        const args=await dioxus.recv();
        const report=async value=>{dioxus.send(value);await dioxus.recv();};
        const pause=()=>new Promise(r=>setTimeout(r,40));
        const assert=(ok,label)=>{if(!ok)throw Error(label);};
        const wait=async(fn,label)=>{for(let i=0;i<150;i++){if(fn())return;await pause();}throw Error(label);};
        const root=()=>document.getElementById('forward-dialog');
        const button=(label,host=root())=>host&&[...host.querySelectorAll('button')].find(b=>b.textContent.trim()===label);
        const row=()=>root()?.querySelector(`[data-delivery][data-destination="${args.destination}"]`);
        const visible=element=>{
            if(!element)return false;
            const r=element.getBoundingClientRect();
            if(r.width<=0||r.height<=0||r.top<0||r.bottom>innerHeight+1||r.left<0||r.right>innerWidth+1)return false;
            for(let parent=element.parentElement;parent;parent=parent.parentElement){
                if(/auto|scroll|hidden|clip/.test(getComputedStyle(parent).overflowY)){
                    const bounds=parent.getBoundingClientRect();
                    if(r.top<bounds.top-1||r.bottom>bounds.bottom+1)return false;
                }
            }
            return true;
        };
        const restored=async()=>{
            await wait(()=>row()?.dataset.delivery==='uncertain'&&button('Check delivery'),'durable receipt restored');
            assert(!root().querySelector('#forward-destination')&&!root().querySelector('#forward-note'),'recovery has no fresh send editor');
            assert(!button('Forward'),'recovery cannot send a new request');
        };
        const showNote=async()=>{
            const details=row().querySelector('.forward-saved-note');
            assert(details,'original note available');
            if(!details.open)details.querySelector('summary').click();
            await wait(()=>details.open&&details.textContent.includes(args.note)&&details.textContent.includes('Second line kept.'),'Unicode note restored');
            assert([...details.querySelectorAll('.bold,strong,b')].some(n=>n.textContent.includes(args.note)),'rich note formatting retained');
        };
        try {
            await restored();
            await showNote();
            button('Resend…').focus();button('Resend…').click();
            await wait(()=>button('Cancel')===document.activeElement,'resend confirmation focus');
            assert(root().textContent.includes('could create a duplicate'),'resend duplicate warning');
            button('Cancel').click();
            await wait(()=>button('Resend…')===document.activeElement,'resend cancel restores focus');
            button('Check delivery').click();
            await wait(()=>root().querySelector('[role=alert]')?.textContent.includes('Connect to Slack'),'offline check feedback');
            assert(row().dataset.delivery==='uncertain','offline cannot claim delivery');
            await wait(()=>visible(root().querySelector('[role=alert]')),'offline feedback visible');
            button('Resend…').click();await wait(()=>button('Resend original'),'explicit resend confirmation');
            button('Resend original').click();
            await wait(()=>button('Check delivery')&&!button('Check delivery').disabled,'offline resend completes');
            assert(row().dataset.delivery==='uncertain','offline resend retains uncertainty');
            await report({stage:'unchanged'});
            root().querySelector('[aria-label="Close forward dialog"]').click();
            await wait(()=>!root(),'close recovery dialog');
            await wait(()=>document.querySelectorAll('#saved-forwards [data-saved-forward]').length===1,'global recovery inventory');
            const inventory=document.getElementById('saved-forwards');
            assert(inventory.textContent.includes('Recovered destination · 日本語'),'missing destination title retained');
            button('Review delivery',inventory).click();await restored();await showNote();
            button('Stop tracking…').focus();button('Stop tracking…').click();
            await wait(()=>button('Stop tracking')&&button('Cancel')===document.activeElement,'retirement confirmation focus');
            assert(root().textContent.includes('will remain unconfirmed'),'retirement does not claim success');
            await wait(()=>visible(button('Stop tracking'))&&visible(button('Cancel')),'retirement actions visible');
            button('Cancel').click();
            await wait(()=>button('Stop tracking…')===document.activeElement,'retirement cancel restores focus');
            await report({stage:'unchanged'});
            if(args.mode==='retire'){
                button('Stop tracking…').click();await wait(()=>button('Stop tracking'),'retirement reopened');
                button('Stop tracking').click();
                await wait(()=>!row()&&root().textContent.includes('Tracking stopped'),'explicit retirement completed');
                root().querySelector('[aria-label="Close forward dialog"]').click();await wait(()=>!root(),'retired dialog closed');
                await wait(()=>!document.querySelector('#saved-forwards [data-saved-forward]'),'retired inventory row removed');
            }else{
                button('Stop tracking…').click();
                await wait(()=>button('Cancel')===document.activeElement&&visible(button('Stop tracking')),'capture retirement confirmation');
            }
            assert(document.documentElement.scrollWidth<=innerWidth+1,'no horizontal overflow');
            if(root()){
                const box=root().getBoundingClientRect();
                assert(box.left>=0&&box.top>=0&&box.right<=innerWidth+1&&box.bottom<=innerHeight+1,'dialog fits viewport');
            }
            await report({ok:true,mode:args.mode});
        }catch(error){await report({ok:false,case:error.message});}
    })();
    "#,
    );
    let _ = eval.send(serde_json::json!({"mode":mode,"destination":DESTINATION,"note":NOTE}));
    loop {
        let mut result = eval.recv::<serde_json::Value>().await.unwrap_or_default();
        if result["stage"] == "unchanged" {
            let unchanged = storage
                .source(&target)
                .await
                .is_ok_and(|rows| rows == vec![original.clone()]);
            let _ = eval.send("received");
            if !unchanged {
                eprintln!(
                    "forward recovery check failed: offline or cancelled action changed durable request"
                );
                break;
            }
            continue;
        }
        if result["ok"] == true {
            let correct = storage.source(&target).await.is_ok_and(|rows| {
                rows.len() == 1
                    && rows[0].attempt_id == original.attempt_id
                    && rows[0].delivery.request == original.delivery.request
                    && rows[0].delivery.started_at == original.delivery.started_at
                    && matches!(rows[0].delivery.status, Status::Uncertain(_))
                    && rows[0].retired == (mode == "retire")
            });
            if !correct {
                result = serde_json::json!({"ok":false,"case":"final durable identity or retirement mismatch"});
            }
        }
        let _ = eval.send("received");
        eprintln!(
            "forward recovery check {}: {}",
            if result["ok"] == true {
                "passed"
            } else {
                "failed"
            },
            result
        );
        break;
    }
}
