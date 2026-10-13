//! Exercise real menus, Later rows and the shared native viewer without Slack.
use crate::state::ShellState;
use dioxus::prelude::*;
pub(super) async fn run(state: Signal<ShellState>) {
    let mut eval = document::eval(
        r#"await(async()=>{
      const pause=()=>new Promise(r=>setTimeout(r,40));
      const assert=(c,m)=>{if(!c)throw Error(m)};
      const wait=async(f,m)=>{for(let i=0;i<200&&!f();i++)await pause();assert(f(),m)};
      const row=id=>document.querySelector('[data-saved-file="'+id+'"]');
      const control=(id='FSTART')=>document.querySelector('.viewer [data-saved-file-control="'+id+'"]');
      const toggle=(id='FSTART')=>control(id)?.querySelector('[data-saved-file-toggle]');
      const close=async()=>{document.querySelector('#shell-overlay-dialog [aria-label="Close"]').click();await wait(()=>!document.querySelector('.viewer'),'viewer closed')};
      const actions=async(id='FSTART')=>{document.querySelector('.viewer .file-more').click();await wait(()=>toggle(id)&&!toggle(id).disabled,'saved file control ready')};
      const visible=async(element,label)=>{element.scrollIntoView({block:'center',behavior:'instant'});await pause();const r=element.getBoundingClientRect();assert(r.left>=0&&r.right<=innerWidth+1&&r.top>=0&&r.bottom<=innerHeight+1,label+' visible')};
      const dismissToast=async()=>{const button=document.querySelector('.toast [aria-label="Dismiss notification"]');if(button){button.click();await wait(()=>!document.querySelector('.toast'),'notification dismissed through actual control')}};
      const tabBounds=()=>{
        const panel=document.querySelector('.later-panel').getBoundingClientRect();
        const tabs=[...document.querySelectorAll('.later-tabs [role=tab]')];
        assert(tabs.length===3,'all Later status tabs mounted');
        return tabs.map(tab=>{const r=tab.getBoundingClientRect();assert(r.left>=panel.left-1&&r.right<=panel.right+1&&r.top>=panel.top-1&&r.bottom<=panel.bottom+1&&r.left>=0&&r.right<=innerWidth+1&&r.top>=0&&r.bottom<=innerHeight+1,'Later tab fully inside panel and window: '+tab.textContent.trim());return {text:tab.textContent.trim(),left:r.left,right:r.right,top:r.top,bottom:r.bottom};});
      };
      try {
        await wait(()=>document.querySelector('.viewer pre'),'initial native file viewer');
        assert(!row('FSTART'),'initial file not saved');
        assert(document.querySelector('.later-tabs [role=tab] span')?.textContent.trim()==='4','seeded saved count matches four file rows');tabBounds();await actions();
        assert(toggle().textContent.trim()==='Save to Later','actual File Actions save offered');
        await visible(toggle(),'Save to Later');toggle().click();
        await wait(()=>toggle()?.textContent.trim()==='Remove from Later'&&!toggle().disabled,'positive save reflected');
        await close();await wait(()=>row('FSTART'),'saved file appears in Later');
        row('FSTART').querySelector('.later-entry').click();
        await wait(()=>document.querySelector('.viewer pre')?.textContent.includes('FSTART'),'saved file reopens native original');
        assert(document.querySelector('#shell-overlay-dialog h2').textContent==='Saved release notes','hydrated native title');
        await actions();assert(toggle().textContent.trim()==='Remove from Later','saved file removal offered');
        await visible(toggle(),'Remove from Later');toggle().click();
        await wait(()=>toggle()?.textContent.trim()==='Save to Later'&&!toggle().disabled,'positive remove reflected');
        await close();await wait(()=>!row('FSTART'),'removed file leaves Later');
        row('FCOLDOPEN').querySelector('.later-entry').click();
        await wait(()=>document.querySelector('.viewer pre')?.textContent.includes('FCOLDOPEN'),'cold saved metadata opens native viewer');
        dioxus.send({stage:'viewer',ok:true});await dioxus.recv();
        const mountedInventory=document.querySelector('[data-saved-file-recovery]');
        assert(mountedInventory,'existing recovery inventory mounted before new uncertainty');
        await actions('FCOLDOPEN');assert(toggle('FCOLDOPEN').textContent.trim()==='Remove from Later','cold saved file removal offered');
        toggle('FCOLDOPEN').click();
        await wait(()=>control('FCOLDOPEN')?.textContent.includes('unconfirmed')&&!toggle('FCOLDOPEN'),'new failed transport retained in real menu');
        await close();
        await wait(()=>document.querySelector('[data-saved-file-change="FCOLDOPEN"]')?.textContent.includes('unconfirmed'),'new uncertainty appears without refresh');
        assert(document.querySelector('[data-saved-file-recovery]')===mountedInventory,'existing inventory updated in place');
        dioxus.send({stage:'hydrate',ok:true});await dioxus.recv();
        await wait(()=>row('FPREVIEW')?.textContent.includes('Background loaded notes'),'cold background title hydration');
        assert(row('FPREVIEW').textContent.includes('Plain text')&&row('FPREVIEW').textContent.includes('You'),'file type and owner shown');
        assert(row('FMISSING').textContent.includes('File unavailable'),'unavailable metadata explained');
        row('FMISSING').querySelector('.later-entry').click();
        await wait(()=>document.querySelector('.toast')?.textContent.includes('This saved file is unavailable.'),'missing file explains failure');
        assert(!document.querySelector('.viewer'),'missing file does not open stale viewer');await dismissToast();
        assert(row('FDELETED').textContent.includes('This file was deleted'),'known deletion explained');
        assert(row('FDELETED').querySelector('.later-entry').disabled,'deleted open disabled');
        for(const id of ['FCOLDOPEN','FPREVIEW','FMISSING','FDELETED']) {
          const remove=row(id).querySelector('[aria-label="Remove from Later"]');
          assert(remove&&!remove.disabled,'remove remains actionable '+id);await visible(remove,'remove '+id);
        }
        const recovery=()=>document.querySelector('[data-saved-file-change="FRECOVERY"]');
        await wait(()=>recovery()?.textContent.includes('unconfirmed'),'absent-file receipt remains discoverable');
        assert(!row('FRECOVERY'),'receipt recovery does not invent saved membership');
        const check=[...recovery().querySelectorAll('button')].find(b=>b.textContent.trim()==='Check saved status');
        assert(check&&!check.disabled,'read-only recovery check available');await visible(check,'check saved status');check.click();
        await wait(()=>recovery()?.textContent.includes('Please wait before checking again.')&&[...recovery().querySelectorAll('button')].some(b=>b.textContent.trim()==='Check saved status'&&!b.disabled),'inconclusive check preserves uncertain receipt and cooldown');
        const stop=[...recovery().querySelectorAll('button')].find(b=>b.textContent.trim()==='Stop tracking…');stop.click();
        await wait(()=>recovery().textContent.includes('This does not undo it'),'retirement requires explicit confirmation');
        [...recovery().querySelectorAll('button')].find(b=>b.textContent.trim()==='Keep checking').click();
        await wait(()=>recovery().textContent.includes('Check saved status'),'cancel retirement keeps recovery');
        assert(!recovery().querySelector('[data-saved-file-toggle]'),'uncertain receipt offers no unsafe retry');
        await visible([...recovery().querySelectorAll('button')].find(b=>b.textContent.trim()==='Check saved status'),'retained recovery action');
        await dismissToast();dioxus.send({stage:'recovery',ok:true});await dioxus.recv();
        [...recovery().querySelectorAll('button')].find(b=>b.textContent.trim()==='Stop tracking…').click();
        await pause();[...recovery().querySelectorAll('button')].find(b=>b.textContent.trim()==='Stop tracking').click();
        await wait(()=>!recovery()||recovery().textContent.includes('Please wait before making another change.'),'retired receipt reaches terminal UI automatically');
        if(recovery()) {
          assert(!recovery().querySelector('[data-saved-file-toggle]')&&!recovery().textContent.includes('unconfirmed')&&!recovery().textContent.includes('Check saved status'),'retired cooldown cannot dispatch or imply uncertainty');
          assert(recovery().textContent.includes('wait'),'retained cooldown is explained');
          assert([...recovery().querySelectorAll('button')].some(b=>b.textContent.includes('Refresh')),'cooldown can be refreshed');
        }
        await dismissToast();document.querySelector('.later-body').scrollTop=0;await pause();
        const tabs=tabBounds();
        dioxus.send({ok:true,tabs,savedRemoved:true,coldOpen:true,backgroundHydrated:true,missingRetry:true,deletedDisabled:true,recoveryRetained:true,newUncertaintyVisible:true,retirementConfirmed:true,viewport:{width:innerWidth,height:innerHeight}});
      }catch(e){dioxus.send({ok:false,error:e.message})}
      await dioxus.recv();
    })();"#,
    );
    let mut result = eval.recv::<serde_json::Value>().await.unwrap_or_default();
    if result["stage"] == "viewer" {
        eprintln!("later files viewer ready: {result}");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let _ = eval.send("captured");
        result = eval.recv::<serde_json::Value>().await.unwrap_or_default();
    }
    if result["stage"] == "hydrate" {
        let generation = state.peek().core.later.generation;
        super::files::hydrate(state, None, generation).await;
        let _ = eval.send("hydrated");
        result = eval.recv::<serde_json::Value>().await.unwrap_or_default();
    }
    if result["stage"] == "recovery" {
        eprintln!("later files recovery ready: {result}");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let _ = eval.send("captured");
        result = eval.recv::<serde_json::Value>().await.unwrap_or_default();
    }
    let durable = super::file_fixture::verify_receipts()
        .await
        .unwrap_or_else(|error| serde_json::json!({"ok":false,"error":error}));
    if durable["ok"] != true {
        result["ok"] = serde_json::json!(false);
    }
    result["receipts"] = durable;
    let _ = eval.send("received");
    let shell = state.peek();
    result["native"] = serde_json::json!({"savedStartPresent":shell.core.later.items.contains_key(&super::SavedKey::file("FSTART")),"cachedFiles":shell.core.later.files.len(),"fileErrors":shell.core.later.file_errors.len()});
    eprintln!(
        "later files check {}: {result}",
        if result["ok"] == true {
            "passed"
        } else {
            "failed"
        }
    );
}
