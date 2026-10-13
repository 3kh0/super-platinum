//! Offline native assertions: recovery is reviewable without authorizing replay.
use crate::state::ShellState;
use dioxus::prelude::*;

pub(super) async fn run(state: Signal<ShellState>) {
    if !matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("managed-draft-recovery" | "managed-draft-recovery-narrow")
    ) || std::env::var_os("SUPER_PLATINUM_MANAGED_DRAFT_RECOVERY_CHECK").is_none()
        || state.peek().core.session.is_some()
        || state.peek().core.transport.is_some()
    {
        return;
    }
    if let Err(error) = crate::drafts::settlement_fixture::seed(state).await {
        eprintln!("managed draft recovery check failed: fixture storage {error:?}");
        return;
    }
    let mut eval = document::eval(
        r#"await (async()=>{
      const pause=()=>new Promise(resolve=>setTimeout(resolve,40));
      const assert=(value,label)=>{if(!value)throw Error(label);};
      try{
        for(let i=0;i<150&&!document.querySelector('[data-managed-draft-recovery="delivery"]');i++)await pause();
        await new Promise(resolve=>setTimeout(resolve,200));
        const root=document.getElementById('managed-draft-recovery'),body=root?.closest('.scheduled-body');
        assert(root&&body,'real draft recovery inventory mounted');
        const kinds=['delivery','remote','alternate','local'],texts=['Frozen send remains available after restart.','Remote draft text was preserved.','Alternate local text was preserved.','Unfinished preparation stays editable.'];
        const bounds=[];
        for(let i=0;i<kinds.length;i++){
          const row=root.querySelector('[data-managed-draft-recovery="'+kinds[i]+'"]');
          assert(row&&row.textContent.includes(texts[i]),'exact retained '+kinds[i]+' content');
          const label=row.querySelector('.scheduled-meta');label.scrollIntoView({block:'center',behavior:'instant'});await pause();
          const box=body.getBoundingClientRect(),r=label.getBoundingClientRect();
          assert(r.top>=box.top-1&&r.bottom<=box.bottom+1&&r.left>=box.left-1&&r.right<=box.right+1,'recovery label reachable: '+kinds[i]);
          bounds.push({kind:kinds[i],left:r.left,right:r.right,top:r.top,bottom:r.bottom});
        }
        assert(root.querySelector('[data-managed-draft-recovery="delivery"]').textContent.includes('Delivery unconfirmed'),'uncertain delivery remains explicit');
        const local=[...body.querySelectorAll('article.scheduled-item')].find(row=>!row.hasAttribute('data-managed-draft-recovery')&&row.textContent.includes('Newer local draft stays separate.'));
        assert(local&&local.querySelector('.draft-resume'),'newer local draft remains independently editable');
        const buttons=[...root.querySelectorAll('button')].map(button=>button.textContent.trim());
        assert(buttons.every(text=>text==='Refresh'||text==='Check delivery'||text==='Checking…'||text==='Continue editing locally'||text==='Preparing local editing…'||text==='Review local versions'),'recovery offers only review, delivery checks, or local editing');
        assert(root.querySelector('[data-managed-draft-recovery="local"] button')?.textContent.trim()==='Continue editing locally','clean ownership has an explicit local-only release');
        const delivery=root.querySelector('[data-managed-draft-recovery="delivery"]');
        const check=[...delivery.querySelectorAll('button')].find(button=>button.textContent.trim()==='Check delivery');
        assert(check,'uncertain send has a read-only delivery check');
        check.click();
        for(let i=0;i<100&&!delivery.textContent.includes('Connect to Slack before checking delivery.');i++)await pause();
        assert(delivery.textContent.includes('Connect to Slack before checking delivery.'),'disconnected check keeps saved content and explains connection requirement');
        assert(delivery.textContent.includes(texts[0]),'checking never consumes saved content');
        assert(!check.disabled,'disconnected check releases busy state');
        assert(!document.querySelector('#channel-composer'),'empty local composer was not replaced with a recovered send');

        const opener=()=>document.getElementById('local-versions-T1-C1-channel-open');
        const panel=()=>document.getElementById('local-versions-T1-C1-channel');
        const waitFor=async(test,label)=>{for(let i=0;i<150&&!test();i++)await pause();assert(test(),label);};
        const reachable=async(element,label)=>{
          element.scrollIntoView({block:'center',behavior:'instant'});await pause();
          const r=element.getBoundingClientRect(),b=body.getBoundingClientRect();
          assert(r.top>=b.top-1&&r.bottom<=b.bottom+1&&r.left>=b.left-1&&r.right<=b.right+1,label+' reachable');
        };
        assert(opener(),'conflicting local versions have one review opener');
        await reachable(opener(),'review opener');opener().click();
        await waitFor(()=>panel()?.querySelectorAll('input[type=radio]').length===3,'all exact local choices loaded');
        assert(panel().getAttribute('role')==='region'&&panel().getAttribute('aria-label')==='Review local versions','review region labeled');
        assert(panel().querySelector('[role=radiogroup]')?.getAttribute('aria-label')==='Local draft versions','native radio group labeled');
        const choiceText=[...panel().querySelectorAll('[data-local-version-choice]')].map(row=>row.textContent);
        for(const text of ['Current local draft','Newer local draft stays separate.','Preserved local version','Alternate local text was preserved.','Saved local version 1','Saved editor version was preserved.','reviewed-attachment.txt'])assert(panel().textContent.includes(text),'review preserves '+text);
        await waitFor(()=>document.activeElement===panel()?.querySelector('input[type=radio]'),'opening focuses first radio');
        assert(panel().querySelector('input[type=radio]').checked,'current local version initially selected');
        const third=panel().querySelectorAll('input[type=radio]')[2];await reachable(third,'saved editor choice');third.click();await pause();
        assert(third.checked,'real native radio selection applies');
        const cancel=[...panel().querySelectorAll('button')].find(button=>button.textContent==='Cancel');
        await reachable(cancel,'cancel control');cancel.click();
        await waitFor(()=>!panel(),'cancel closes review');
        await waitFor(()=>document.activeElement===opener(),'cancel restores opener focus');
        assert(root.textContent.includes('Saved editor version was preserved.')&&local.textContent.includes('Newer local draft stays separate.'),'cancel preserves saved versions and current draft');
        opener().click();await waitFor(()=>panel()?.querySelectorAll('input[type=radio]').length===3,'reopen reloads persisted inventory');
        assert(panel().querySelector('input[type=radio]').checked,'reopen resets to current local choice');
        panel().querySelector('input').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',code:'Escape',bubbles:true}));
        await waitFor(()=>!panel(),'synthetic Escape closes only review');
        await waitFor(()=>document.activeElement===opener(),'Escape restores opener focus');
        assert(document.getElementById('managed-draft-recovery')===root,'Escape preserves Drafts surface');
        opener().click();await waitFor(()=>panel()?.querySelectorAll('input[type=radio]').length===3,'final review reopened');
        const chosen=panel().querySelectorAll('input[type=radio]')[1];await reachable(chosen,'preserved local choice');chosen.click();await pause();
        assert(chosen.checked,'preserved local version selected');
        const apply=[...panel().querySelectorAll('button')].find(button=>button.textContent==='Use selected version');
        await reachable(apply,'apply control');
        dioxus.send({stage:'review',ok:true,choices:choiceText});await dioxus.recv();
        apply.click();
        await waitFor(()=>!panel()&&!opener(),'settlement closes and removes reviewed versions');
        await waitFor(()=>document.activeElement?.isConnected&&document.activeElement.tagName==='BUTTON'&&document.activeElement.textContent.trim()==='Refresh'&&!document.activeElement.disabled,'successful settlement restores reachable inventory focus');
        assert(!root.querySelector('[data-managed-draft-recovery=alternate]')&&!root.querySelector('[data-managed-draft-recovery=writer-alternate]'),'reviewed alternates disappear after commit');
        const settledLocal=[...body.querySelectorAll('article.scheduled-item')].find(row=>!row.hasAttribute('data-managed-draft-recovery')&&row.textContent.includes('Alternate local text was preserved.'));
        assert(settledLocal&&settledLocal.querySelector('.draft-resume'),'selected local draft remains independently editable');
        assert(root.querySelector('[data-managed-draft-recovery=remote]')?.textContent.includes('Remote draft text was preserved.'),'remote conflict is retained after local settlement');
        assert(delivery.textContent.includes('Frozen send remains available after restart.')&&delivery.textContent.includes('Delivery unconfirmed'),'other frozen delivery unchanged');

        body.scrollTop=0;await pause();
        dioxus.send({ok:true,kinds,buttons,bounds,newerLocal:local.textContent,settledLocal:settledLocal.textContent,selectionCancelReopen:true,escapeFocus:true,successFocus:true,viewport:{width:innerWidth,height:innerHeight}});
      }catch(error){dioxus.send({ok:false,error:error.message});}
      await dioxus.recv();
    })();"#,
    );
    let mut result = eval.recv::<serde_json::Value>().await.unwrap_or_default();
    if result["stage"] == "review" {
        eprintln!("managed draft review ready: {}", result);
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let _ = eval.send("capture-ready");
        result = eval.recv::<serde_json::Value>().await.unwrap_or_default();
    }
    let durable = crate::drafts::settlement_fixture::verify()
        .await
        .unwrap_or_else(|error| serde_json::json!({"ok":false,"error":format!("{error:?}")}));
    if durable["ok"] != true {
        result["ok"] = serde_json::json!(false);
    }
    result["settlement"] = durable;
    {
        let shell = state.peek();
        let frozen = shell
            .managed_drafts
            .entries
            .values()
            .filter(|view| {
                view.local.empty()
                    && view
                        .stored
                        .as_ref()
                        .is_some_and(|stored| stored.state.consumption().is_some())
            })
            .count();
        result["native"] = serde_json::json!({"emptyComposer":shell.core.composer.text.is_empty(),"frozenEmptyLocal":frozen,"recoveryRows":crate::drafts::session::recovery_rows(&shell).len()});
        if !shell.core.composer.text.is_empty() || frozen != 1 {
            result["ok"] = serde_json::json!(false);
        }
    }
    let _ = eval.send("received");
    eprintln!(
        "managed draft recovery check {}: {}",
        if result["ok"] == true {
            "passed"
        } else {
            "failed"
        },
        result
    );
}
