//! Disconnected native channel virtualization, navigation, and selection checks.
use crate::state::ShellState;
use dioxus::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use super_platinum_core::domain::PendingScrollTarget;
static STARTED: AtomicBool = AtomicBool::new(false);

pub(super) async fn run(mut state: Signal<ShellState>) {
    if std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() != Ok("channel-window-10000")
        || std::env::var("SUPER_PLATINUM_CHANNEL_WINDOW_CHECK").as_deref() != Ok("1")
        || STARTED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        eprintln!("channel window check failed: connected fixture");
        return;
    }
    let (date, unread, last, channel) = {
        let shell = state.peek();
        (
            shell
                .channel_rows
                .iter()
                .enumerate()
                .find(|(index, row)| *index > 3_000 && row.date_label.is_some())
                .map(|(_, row)| row.id.clone())
                .unwrap(),
            shell.channel_rows[4_999].id.clone(),
            shell.channel_rows.last().unwrap().id.clone(),
            shell.core.active_channel.clone().unwrap(),
        )
    };
    let unread_row = state.peek().channel_rows[5_000].id.clone();
    let mut eval = document::eval(
        r#"await(async()=>{
      const args=await dioxus.recv();
      const pause=()=>new Promise(r=>setTimeout(r,40));
      const wait=async(fn,label)=>{for(let i=0;i<150;i++){if(fn())return;await pause();}throw Error(label);};
      const assert=(v,label)=>{if(!v)throw Error(label);};
      const body=()=>document.getElementById('message-timeline');
      const rows=()=>[...body().querySelectorAll('[data-row-index]')];
      const visible=()=>{const b=body().getBoundingClientRect();return rows().filter(r=>{const a=r.getBoundingClientRect();return a.bottom>b.top&&a.top<b.bottom;});};
      const indices=()=>visible().map(r=>Number(r.dataset.rowIndex));
      const check=()=>{assert(rows().length>0&&rows().length<=100,'bounded mounted rich rows');assert(visible().length>0,'visible rows materialized');};
      const atBottom=()=>body().scrollHeight-body().scrollTop-body().clientHeight<48;
      const jump=async fraction=>{
        body().dispatchEvent(new WheelEvent('wheel',{deltaY:fraction===1?1:-1,bubbles:true}));await pause();
        body().dispatchEvent(new PointerEvent('pointerdown',{button:0,buttons:1,bubbles:true}));
        body().scrollTop=(body().scrollHeight-body().clientHeight)*fraction;
        body().dispatchEvent(new Event('scroll',{bubbles:true}));
        document.dispatchEvent(new PointerEvent('pointerup',{button:0,buttons:0,bubbles:true}));
        await wait(()=>visible().length>0,'far jump fills virtual spacer');
        await new Promise(r=>setTimeout(r,300));check();
        if(fraction===0)assert(indices().includes(0),'first cached row visible after top jump');
        else if(fraction===1)assert(atBottom()&&visible().some(r=>r.dataset.rowId===args.last),'latest pending row visible');
        else assert(indices()[0]>500&&indices()[0]<9500,'far absolute history index reached');
        return {fraction,first:indices()[0],last:indices().at(-1),mounted:rows().length,scrollTop:Math.round(body().scrollTop)};
      };
      const settled=[];
      const settle=async(phase,targetId,issuedAt)=>{
        let consecutive=0,firstMatchMs=null;const mismatches=[];
        for(let attempt=0;attempt<60;attempt++){
          await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));await pause();
          const dom=rows().map(row=>({id:row.dataset.rowId,index:Number(row.dataset.rowIndex)}));
          const shown=visible().map(row=>row.dataset.rowId);
          dioxus.send({stage:'sync',phase,rows:dom,visible:shown,target:targetId,pending:body().dataset.scrollPending});
          const native=await dioxus.recv();
          const match=native.ok&&shown.includes(targetId)&&body().dataset.scrollPending==='false';
          if(match){if(firstMatchMs===null)firstMatchMs=performance.now()-issuedAt;if(++consecutive===3){settled.push({phase,attempts:attempt+1,firstMatchMs,confirmedMs:performance.now()-issuedAt,mismatches});return;}}
          else {consecutive=0;if(mismatches.length<8)mismatches.push({attempt,native,first:dom[0]?.index,last:dom.at(-1)?.index,targetVisible:shown.includes(targetId)});}
        }
        throw Error('DOM/native target window did not settle: '+phase+' '+JSON.stringify(mismatches));
      };
      try{
        await wait(()=>body()&&rows().length&&window.superPlatinumSelectionLifecycle,'channel mounted');
        await wait(()=>atBottom()&&visible().some(r=>r.dataset.rowId===args.last),'initial latest pending row');check();
        const jumps=[];for(const fraction of [0,.25,.5,.75,1])jumps.push(await jump(fraction));
        assert(jumps[2].first>jumps[1].first+500&&jumps[3].first>jumps[2].first+500,'far jumps traverse distinct cached ranges');
        await jump(.4);
        const selected=visible()[0],range=document.createRange(),selection=getSelection();
        selected.dispatchEvent(new PointerEvent('pointerdown',{button:0,buttons:1,bubbles:true}));
        selected.dispatchEvent(new Event('selectstart',{bubbles:true}));
        await wait(()=>body().dataset.selectionPinned==='true','native channel selection pins projection');
        range.selectNodeContents(selected);selection.removeAllRanges();selection.addRange(range);
        const held=rows().map(r=>r.dataset.rowId),text=selection.toString();assert(text.length>0,'real DOM range selects text');
        body().scrollTop=(body().scrollHeight-body().clientHeight)*.7;body().dispatchEvent(new Event('scroll',{bubbles:true}));await pause();
        assert(body().dataset.selectionPinned==='true'&&JSON.stringify(rows().map(r=>r.dataset.rowId))===JSON.stringify(held),'drag retains exact mounted IDs');
        assert(selection.toString()===text,'drag retains selected text');
        document.body.dispatchEvent(new PointerEvent('pointerup',{button:0,buttons:0,bubbles:true}));document.body.dispatchEvent(new MouseEvent('mouseup',{button:0,buttons:0,bubbles:true}));
        await wait(()=>body().dataset.selectionPinned==='false'&&visible().length>0,'outside release materializes new far window');
        selection.removeAllRanges();await new Promise(r=>setTimeout(r,300));check();
        const dateIssuedAt=performance.now();dioxus.send({stage:'date',ok:true,jumps,selection:true});const target=await dioxus.recv();
        assert(target.ok,'production message target navigation lands');
        await wait(()=>visible().some(r=>r.dataset.rowId===args.date),'far message target visible');
        await settle('date',args.date,dateIssuedAt);
        const dateRow=rows().find(r=>r.dataset.rowId===args.date);assert(dateRow?.querySelector('.date-separator'),'far date boundary annotation retained');check();
        const unreadIssuedAt=performance.now();dioxus.send({stage:'unread',ok:true});const unread=await dioxus.recv();assert(unread.ok,'production unread navigation lands');
        await wait(()=>visible().some(r=>r.querySelector('.unread-divider')),'far unread divider visible');await settle('unread',args.unreadRow,unreadIssuedAt);check();
        dioxus.send({ok:true,jumps,selection:true,settled,finalVisible:visible().map(row=>row.dataset.rowId),finalPending:body().dataset.scrollPending,finalRows:rows().map(row=>({id:row.dataset.rowId,index:Number(row.dataset.rowIndex)})),dateTarget:args.date,unread:true,mounted:rows().length,first:indices()[0],last:indices().at(-1),viewport:{width:innerWidth,height:innerHeight}});
      }catch(error){dioxus.send({ok:false,error:error.message,mounted:body()?rows().length:0,indices:body()?indices():[]});}
      await dioxus.recv();
    })();"#,
    );
    let _ = eval.send(serde_json::json!({"date":date,"last":last,"unreadRow":unread_row}));
    let mut result = eval.recv::<serde_json::Value>().await.unwrap_or_default();
    let mut stages = Vec::new();
    while let Some(stage) = result["stage"].as_str() {
        if stage == "sync" {
            let reply = synchronized(&state.peek(), &result);
            let _ = eval.send(reply);
            result = eval.recv::<serde_json::Value>().await.unwrap_or_default();
            continue;
        }
        let target = match stage {
            "date" => PendingScrollTarget::Message(date.clone()),
            "unread" => PendingScrollTarget::FirstUnreadAfter(unread.clone()),
            _ => break,
        };
        stages.push(result.clone());
        {
            let mut shell = state.write();
            shell.core.pending_scroll_to = Some((channel.clone(), target.clone()));
            shell.stick_to_bottom = false;
            shell.reset_timeline_window();
        }
        // Use the complete production fixture navigation path, including the
        // exact pending-anchor retirement after the row actually lands.
        crate::bootstrap::refresh_selected_channel(state).await;
        let landed = state.peek().core.pending_scroll_to.is_none();
        let _ = eval.send(serde_json::json!({"ok":landed}));
        result = eval.recv::<serde_json::Value>().await.unwrap_or_default();
    }
    {
        let shell = state.peek();
        let final_check = synchronized(
            &shell,
            &serde_json::json!({"rows":result["finalRows"],"target":unread_row,"visible":result["finalVisible"],"pending":result["finalPending"]}),
        );
        let valid = final_check["ok"] == true
            && shell.channel_rows.len() == 10_001
            && shell.messages.len() <= 100
            && shell.messages.len() == shell.timeline_end - shell.timeline_start
            && !shell.selection_pinned;
        result["native"] = serde_json::json!({"metadata":shell.channel_rows.len(),"projected":shell.messages.len(),"start":shell.timeline_start,"end":shell.timeline_end,"selectionPinned":shell.selection_pinned});
        result["stages"] = serde_json::json!(stages);
        result["finalAgreement"] = final_check;
        if let Some(object) = result.as_object_mut() {
            object.remove("finalRows");
            object.remove("finalVisible");
            object.remove("finalPending");
        }
        if !valid {
            result["ok"] = false.into();
        }
    }
    let _ = eval.send("received");
    eprintln!(
        "channel window check {}: {}",
        if result["ok"] == true {
            "passed"
        } else {
            "failed"
        },
        result
    );
}

fn synchronized(shell: &ShellState, sample: &serde_json::Value) -> serde_json::Value {
    let rows = sample["rows"].as_array();
    let metadata = shell
        .channel_rows
        .get(shell.timeline_start..shell.timeline_end);
    let agrees = rows.zip(metadata).is_some_and(|(rows, metadata)| {
        rows.len() == metadata.len()
            && rows.len() == shell.messages.len()
            && rows
                .iter()
                .zip(metadata)
                .zip(&shell.messages)
                .enumerate()
                .all(|(offset, ((dom, row), message))| {
                    dom["id"].as_str() == Some(row.id.as_str())
                        && message.id == row.id
                        && dom["index"].as_u64() == Some((shell.timeline_start + offset) as u64)
                })
    });
    let target = sample["target"].as_str();
    let target_present =
        target.is_some_and(|target| shell.messages.iter().any(|message| message.id == target));
    let target_visible = sample["visible"]
        .as_array()
        .is_some_and(|visible| visible.iter().any(|id| id.as_str() == target));
    let pending = shell.core.pending_scroll_to.is_some();
    serde_json::json!({"ok":agrees&&target_present&&target_visible&&!pending&&sample["pending"]=="false", "matches":agrees,"targetPresent":target_present,"targetVisible":target_visible,"pending":pending,"start":shell.timeline_start,"end":shell.timeline_end,"projected":shell.messages.len()})
}
