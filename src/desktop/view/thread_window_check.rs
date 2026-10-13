//! Native disconnected regression for bounded thread rendering and jumps.
use crate::state::ShellState;
use dioxus::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
static STARTED: AtomicBool = AtomicBool::new(false);

pub(super) async fn run(state: Signal<ShellState>) {
    if !crate::fixture::is_fixture()
        || std::env::var_os("SUPER_PLATINUM_THREAD_WINDOW_CHECK").is_none()
        || STARTED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        eprintln!("thread window check failed: connected fixture");
        return;
    }
    let (target, total, pending, root, gap) = {
        let shell = state.peek();
        (
            shell.thread_target.clone(),
            shell.thread_window.rows.len(),
            shell.thread_messages.last().map(|m| m.id.clone()),
            shell.thread_root.clone(),
            shell.thread_history.newer.is_some(),
        )
    };
    let mut eval = document::eval(
        r#"await(async()=>{
      const args=await dioxus.recv();
      window.superPlatinumAnchorDebug=Boolean(args.trace);
      const report=async v=>{dioxus.send(v);await dioxus.recv();};
      const pause=()=>new Promise(r=>setTimeout(r,40));
      const wait=async(fn,label)=>{for(let i=0;i<150;i++){if(fn())return;await pause();}throw Error(label);};
      const assert=(v,label)=>{if(!v)throw Error(label);};
      const body=()=>document.getElementById('thread-body');
      const rows=()=>[...body().querySelectorAll('[data-thread-index]')];
      const visible=()=>{const b=body().getBoundingClientRect();return rows().filter(r=>{const a=r.getBoundingClientRect();return a.bottom>b.top&&a.top<b.bottom;});};
      const bounds=()=>({top:Math.round(body().scrollTop),height:Math.round(body().scrollHeight),viewport:body().clientHeight,atBottom:body().scrollHeight-body().scrollTop-body().clientHeight<48,first:visible()[0]?.dataset.rowId,last:visible().at(-1)?.dataset.rowId});
      const check=()=>{assert(rows().length<=102,'bounded mounted rows');assert(rows().some(r=>r.dataset.rowId===args.root),'root retained');assert(rows().some(r=>r.dataset.rowId===args.pending),'pending retained');};
      const jump=async fraction=>{
        window.superPlatinumThreadAnchor?.note('fixture-jump', fraction);
        body().dispatchEvent(new WheelEvent('wheel',{deltaY:-1,bubbles:true}));await pause();
        body().dispatchEvent(new PointerEvent('pointerdown',{button:0,buttons:1,bubbles:true}));
        body().scrollTop=(body().scrollHeight-body().clientHeight)*fraction;
        body().dispatchEvent(new Event('scroll',{bubbles:true}));
        document.dispatchEvent(new PointerEvent('pointerup',{button:0,buttons:0,bubbles:true}));
        await wait(()=>visible().length>1,'jump materializes replies');
        if(fraction===1)await wait(()=>bounds().atBottom&&visible().some(r=>r.dataset.rowId===args.pending),'latest remains visible after final jump');
        await new Promise(r=>setTimeout(r,250));check();
        if(fraction===1)assert(bounds().atBottom&&visible().some(r=>r.dataset.rowId===args.pending),'final bottom stable after measurements');
        if(fraction===0)assert(visible().some(r=>r.dataset.rowId===args.root),'root visible after top jump');
      };
      try {
        await wait(()=>body()&&rows().length>0,'thread mounted');
        if(args.trace){
          let prototype=body(),descriptor;
          while(prototype&&!descriptor){descriptor=Object.getOwnPropertyDescriptor(prototype,'scrollTop');prototype=Object.getPrototypeOf(prototype);}
          if(descriptor?.get&&descriptor?.set)Object.defineProperty(body(),'scrollTop',{configurable:true,get(){return descriptor.get.call(this);},set(value){window.superPlatinumThreadAnchor?.note('scrollTop-assignment',{value,stack:Error().stack});descriptor.set.call(this,value);}});
        }
        if(args.target) await wait(()=>visible().some(r=>r.dataset.rowId===args.target),'exact target visible');
        else await wait(()=>body().scrollHeight-body().scrollTop-body().clientHeight<48,'initial latest visible');
        check();const initial=rows().length,initialBounds=bounds();
        if(args.gap){const control=body().querySelector('[data-thread-page="newer"]');assert(control,'newer page control retained');assert(control.getBoundingClientRect().top<rows().find(r=>r.dataset.rowId===args.pending).getBoundingClientRect().top,'page gap before pending');}
        const jumps=[];
        for(const fraction of [0,.25,.5,.75,1]){const start=performance.now();await jump(fraction);jumps.push({fraction,rows:rows().length,ms:Math.round(performance.now()-start),bounds:bounds()});}
        await report({ok:true,total:args.total,initial,initialBounds,jumps});
      } catch(error){await report({ok:false,case:error.message,rows:body()?rows().length:0,bounds:body()?bounds():null,anchor:window.superPlatinumThreadAnchor?.debug()});}
    })();"#,
    );
    let _ = eval.send(
        serde_json::json!({"target":target,"total":total,"pending":pending,"root":root,"gap":gap,"trace":std::env::var("SUPER_PLATINUM_THREAD_ANCHOR_TRACE").as_deref()==Ok("1")}),
    );
    let mut result = eval.recv::<serde_json::Value>().await.unwrap_or_default();
    {
        let shell = state.peek();
        result["native_state"] = serde_json::json!({"start":shell.thread_window.start,"end":shell.thread_window.end,"at_bottom":shell.thread_at_bottom,"target":shell.thread_target});
    }
    let _ = eval.send("received");
    eprintln!(
        "thread window check {}: {}",
        if result["ok"] == true {
            "passed"
        } else {
            "failed"
        },
        result
    );
}
