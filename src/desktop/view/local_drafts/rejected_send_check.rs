//! Native offline proof: definite rejection can return to local editing only.
use crate::state::ShellState;
use dioxus::prelude::*;

pub(super) async fn run(state: Signal<ShellState>) {
    if !matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("rejected-send" | "rejected-send-narrow")
    ) || std::env::var("SUPER_PLATINUM_REJECTED_SEND_CHECK").as_deref() != Ok("1")
        || state.peek().core.session.is_some()
        || state.peek().core.transport.is_some()
    {
        return;
    }
    if let Err(error) = crate::drafts::settlement_fixture::seed(state).await {
        eprintln!("rejected send check failed: fixture storage {error:?}");
        return;
    }
    if let Err(error) = crate::drafts::rejected_fixture::park_return(state).await {
        eprintln!("rejected send check failed: parked fixture storage {error:?}");
        return;
    }
    let mut eval = document::eval(
        r#"await (async()=>{
      const pause=()=>new Promise(resolve=>setTimeout(resolve,40));
      const assert=(value,label)=>{if(!value)throw Error(label);};
      const waitFor=async(test,label)=>{for(let i=0;i<150&&!test();i++)await pause();assert(test(),label);};
      const restored='This rejected send can return to an editable draft.';
      const uncertain='Unconfirmed send must never return automatically.';
      const frozen='Rejected original must not overwrite newer writing.';
      const newer='Newer writing remains independently editable.';
      const parked='Restored after navigation stays recoverable without a restart.';
      try {
        const root=()=>document.getElementById('managed-draft-recovery');
        const row=text=>[...(root()?.querySelectorAll('[data-managed-draft-recovery="delivery"]')||[])].find(row=>row.textContent.includes(text));
        const returnButton=text=>[...(row(text)?.querySelectorAll('button')||[])].find(button=>button.textContent.trim()==='Return to draft');
        await waitFor(()=>returnButton(restored),'rejected delivery exposes Return to draft');
        await new Promise(resolve=>setTimeout(resolve,200));
        assert(row(uncertain)&&!returnButton(uncertain),'uncertain delivery has no Return to draft');
        assert(row(frozen),'newer-writing frozen intent remains visible');
        const body=root().closest('.scheduled-body');
        const reachable=async(element,label)=>{
          element.scrollIntoView({block:'center',behavior:'instant'});await pause();
          const r=element.getBoundingClientRect(),b=body.getBoundingClientRect();
          assert(r.top>=b.top-1&&r.bottom<=b.bottom+1&&r.left>=b.left-1&&r.right<=b.right+1,label+' reachable');
        };
        const parkedRow=()=>[...(root()?.querySelectorAll('[data-managed-draft-recovery=local]')||[])].find(row=>row.textContent.includes(parked));
        await waitFor(()=>parkedRow(),'canceled foreground handoff stays in recovery');
        assert(![...body.querySelectorAll('[data-local-draft-key]')].some(row=>row.textContent.includes(parked)),'parked return has not silently overwritten the local book');
        const continueLocal=[...parkedRow().querySelectorAll('button')].find(button=>button.textContent.trim()==='Continue editing locally');
        assert(continueLocal,'parked return exposes actual Continue editing locally');
        await reachable(continueLocal,'parked return recovery');continueLocal.click();
        await waitFor(()=>!parkedRow()&&[...body.querySelectorAll('[data-local-draft-key]')].some(row=>row.textContent.includes(parked)&&row.querySelector('.draft-resume')),'parked return becomes ordinary draft without restart');
        const newerButton=returnButton(frozen);
        if(newerButton){
          await reachable(newerButton,'newer-writing guarded action');newerButton.click();
          await waitFor(()=>row(frozen)?.textContent.includes('This draft or your current writing changed'),'newer-writing attempt reports explicit stale protection');
          await waitFor(()=>!newerButton.disabled,'newer-writing guard releases busy state');
          assert(row(frozen),'guard retains rejected frozen intent');
        }
        assert([...body.querySelectorAll('article.scheduled-item')].some(row=>!row.hasAttribute('data-managed-draft-recovery')&&row.textContent.includes(newer)),'newer draft stays separately editable');
        const button=returnButton(restored);await reachable(button,'Return to draft');
        dioxus.send({stage:'before',ok:true,uncertainProtected:true,newerProtected:true,viewport:{width:innerWidth,height:innerHeight}});await dioxus.recv();
        button.click();
        const local=()=>[...document.querySelectorAll('article.scheduled-item')].find(row=>!row.hasAttribute('data-managed-draft-recovery')&&row.textContent.includes(restored));
        await waitFor(()=>local()?.querySelector('.draft-resume')&&!row(restored),'rejected intent becomes ordinary local draft');
        assert(row(uncertain)&&!returnButton(uncertain),'other uncertain intent unchanged');
        assert(row(frozen),'other rejected intent unchanged');
        const resume=local().querySelector('.draft-resume');
        await waitFor(()=>document.activeElement===resume,'successful return focuses ordinary Continue writing');
        await reachable(resume,'Continue writing');
        dioxus.send({stage:'restored',ok:true});await dioxus.recv();
        resume.click();
        await waitFor(()=>document.getElementById('channel-composer')?.value===restored,'ordinary composer contains restored text');
        const field=document.getElementById('channel-composer');
        await waitFor(()=>document.activeElement===field,'ordinary composer receives focus');
        assert(!field.disabled&&!field.readOnly,'restored composer is editable');
        dioxus.send({ok:true,restoredText:field.value,editable:true,focused:true,viewport:{width:innerWidth,height:innerHeight}});
      }catch(error){dioxus.send({ok:false,error:error.message});}
      await dioxus.recv();
    })();"#,
    );
    let mut result = eval.recv::<serde_json::Value>().await.unwrap_or_default();
    while result["stage"].is_string() {
        eprintln!(
            "rejected send {} ready: {}",
            result["stage"].as_str().unwrap_or_default(),
            result
        );
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let _ = eval.send("capture-ready");
        result = eval.recv::<serde_json::Value>().await.unwrap_or_default();
    }
    let durable = crate::drafts::rejected_fixture::verify()
        .await
        .unwrap_or_else(|error| serde_json::json!({"ok":false,"error":format!("{error:?}")}));
    if durable["ok"] != true {
        result["ok"] = false.into();
    }
    result["durable"] = durable;
    {
        let shell = state.peek();
        let offline = shell.core.session.is_none() && shell.core.transport.is_none();
        let composer = shell.core.composer.text == crate::drafts::rejected_fixture::RESTORED;
        result["native"] = serde_json::json!({"offline":offline,"composerRestored":composer});
        if !offline || !composer {
            result["ok"] = false.into();
        }
    }
    let _ = eval.send("received");
    eprintln!(
        "rejected send check {}: {}",
        if result["ok"] == true {
            "passed"
        } else {
            "failed"
        },
        result
    );
}
