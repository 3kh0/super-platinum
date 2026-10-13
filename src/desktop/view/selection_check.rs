//! Disconnected native selection lifecycle verification; no system input.
use crate::state::ShellState;
use dioxus::prelude::*;

pub(super) async fn run(state: Signal<ShellState>) {
    if !crate::fixture::is_fixture()
        || std::env::var_os("SUPER_PLATINUM_SELECTION_CHECK").is_none()
        || state.peek().core.session.is_some()
        || state.peek().core.transport.is_some()
    {
        return;
    }
    let mut eval = document::eval(
        r#"await (async()=>{
      const pause=()=>new Promise(resolve=>setTimeout(resolve,40));
      const wait=async(fn,label)=>{for(let i=0;i<150;i++){if(fn())return;await pause();}throw Error(label);};
      const assert=(value,label)=>{if(!value)throw Error(label);};
      const roots=()=>[document.getElementById('message-timeline'),document.getElementById('thread-body')];
      const visible=root=>{const box=root.getBoundingClientRect();return [...root.querySelectorAll('[data-row-id]')].filter(row=>{const r=row.getBoundingClientRect();return r.bottom>box.top&&r.top<box.bottom;});};
      const pinned=root=>root.dataset.selectionPinned==='true';
      const start=async root=>{
        const row=visible(root)[0];assert(row,'selection starts on visible text');
        row.dispatchEvent(new PointerEvent('pointerdown',{button:0,buttons:1,bubbles:true}));
        row.dispatchEvent(new Event('selectstart',{bubbles:true}));
        await wait(()=>pinned(root),'selection pin acknowledged');return row;
      };
      const release=async(root,type)=>{
        if(type==='blur')window.dispatchEvent(new Event('blur'));
        else {
          document.body.dispatchEvent(new PointerEvent(type==='mouseup'?'pointerup':type,{button:0,buttons:0,bubbles:true}));
          if(type==='mouseup')document.body.dispatchEvent(new MouseEvent('mouseup',{bubbles:true}));
        }
        await wait(()=>!pinned(root)&&visible(root).length>0,'release resumes visible rows');
        await new Promise(resolve=>setTimeout(resolve,250));
        assert(!pinned(root)&&visible(root).length>0,'release stable after viewport sampling');
      };
      try {
        await wait(()=>window.superPlatinumSelectionLifecycle&&roots().every(root=>root&&visible(root).length>0),'both transcripts and selection bridge mounted');
        const [channel,thread]=roots(),checks=[];
        await wait(()=>thread.scrollHeight-thread.scrollTop-thread.clientHeight<48,'initial thread latest');
        const row=await start(channel),range=document.createRange(),selection=getSelection();
        range.selectNodeContents(row);selection.removeAllRanges();selection.addRange(range);const selected=selection.toString();
        await release(channel,'mouseup');assert(selected.length>0&&selection.toString()===selected,'mounted selected text preserved after outside release');
        checks.push('outside channel release preserves selected text');selection.removeAllRanges();
        for(const [root,type,fraction] of [[channel,'mouseup',.25],[thread,'pointercancel',.4],[thread,'blur',.65]]) {
          await start(root);root.scrollTop=(root.scrollHeight-root.clientHeight)*fraction;root.dispatchEvent(new Event('scroll',{bubbles:true}));
          await pause();assert(pinned(root),'selection stays pinned during drag');
          await release(root,type);checks.push(root.id+' '+type+' releases virtual jump');
        }
        thread.dispatchEvent(new WheelEvent('wheel',{deltaY:1,bubbles:true}));await pause();
        thread.dispatchEvent(new PointerEvent('pointerdown',{button:0,buttons:1,bubbles:true}));
        thread.scrollTop=thread.scrollHeight;thread.dispatchEvent(new Event('scroll',{bubbles:true}));document.dispatchEvent(new PointerEvent('pointerup',{button:0,buttons:0,bubbles:true}));
        await wait(()=>thread.scrollHeight-thread.scrollTop-thread.clientHeight<48&&visible(thread).length>0,'thread can still reach latest');
        await new Promise(resolve=>setTimeout(resolve,250));
        assert(!pinned(channel)&&!pinned(thread),'both pins released');
        dioxus.send({ok:true,checks,channelVisible:visible(channel).length,threadVisible:visible(thread).length,threadRows:thread.querySelectorAll('[data-thread-index]').length});
      }catch(error){dioxus.send({ok:false,error:error.message});}
      await dioxus.recv();
    })();"#,
    );
    let mut result = eval.recv::<serde_json::Value>().await.unwrap_or_default();
    result["native_pins"] = serde_json::json!({"channel":state.peek().selection_pinned,"thread":state.peek().thread_window.selection_pinned});
    if result["native_pins"]["channel"] == true || result["native_pins"]["thread"] == true {
        result["ok"] = serde_json::json!(false);
    }
    let _ = eval.send("received");
    eprintln!(
        "selection lifecycle check {}: {}",
        if result["ok"] == true {
            "passed"
        } else {
            "failed"
        },
        result
    );
}
