//! Gated offline fixture assertion against the actual native message rows.
use crate::state::ShellState;
use dioxus::prelude::*;

pub(super) async fn run(state: Signal<ShellState>) {
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    if !matches!(
        name.as_str(),
        "send-outcomes" | "send-outcomes-narrow" | "send-outcomes-thread"
    ) || std::env::var_os("SUPER_PLATINUM_SEND_OUTCOMES_CHECK").is_none()
        || state.peek().core.session.is_some()
        || state.peek().core.transport.is_some()
    {
        return;
    }
    let threaded = name == "send-outcomes-thread";
    let target = if threaded {
        "thread-body"
    } else {
        "message-timeline"
    };
    let composer = if threaded {
        "thread-composer"
    } else {
        "channel-composer"
    };
    let mut eval = document::eval(&format!(
        r#"{delivery_guard}
    await (async()=>{{
      const target={target:?},composer={composer:?};
      const pause=()=>new Promise(resolve=>setTimeout(resolve,40));
      const assert=(value,label)=>{{if(!value)throw Error(label);}};
      const root=()=>document.getElementById(target);
      try{{
        for(let i=0;i<150&&(!root()||root().querySelectorAll('.pending-label').length!==4||!document.getElementById(composer));i++)await pause();
        await new Promise(resolve=>setTimeout(resolve,250));
        const container=root();assert(container,'transcript mounted');
        const labels=[...container.querySelectorAll('.pending-label')];
        const texts=labels.map(label=>label.textContent.trim());
        assert(texts.length===4,'four retained pending rows');
        assert(texts.filter(text=>text==='Not sent').length===2,'two explicit rejection labels');
        assert(texts.filter(text=>text==='Delivery unconfirmed').length===2,'two uncertain delivery labels');
        assert(!texts.some(text=>/Sending|Uploading/.test(text)),'stopped rows never imply active transfer');
        const content=container.textContent;
        for(const text of ['This message was rejected.','The connection ended before delivery','This upload was rejected.','These files may have arrived.','release-notes.txt','roadmap.pdf'])assert(content.includes(text),'preserved content: '+text);
        assert(document.getElementById(composer).value==='A newer draft stays here.','newer composer preserved');
        const composerGuards=[];
        for(const field of ['channel-composer','thread-composer'].map(id=>document.getElementById(id)).filter(Boolean)){{
          const original=field.value,scope=field.dataset.draftScope,style=field.getAttribute('style');
          const start=field.selectionStart,end=field.selectionEnd,direction=field.selectionDirection;
          try{{
            assert(typeof scope==='string'&&scope.length>0,field.id+' has destination scope');
            assert(!clearDeliveredComposer(field.id,scope+'-obsolete',original)&&field.value===original,'same text in another destination is preserved');
            assert(!replaceReviewedComposer(field.id,scope+'-obsolete',original,'Selected version')&&field.value===original,'review cannot replace another destination');
            composerGuards.push(field.id+':wrong-scope-same-value');
            field.value='Native input after durable consumption 日本語';
            const newer=field.value;
            assert(!clearDeliveredComposer(field.id,scope,original)&&field.value===newer,'newer native text is preserved');
            assert(!replaceReviewedComposer(field.id,scope,original,'Selected version')&&field.value===newer,'review cannot replace newer native text');
            composerGuards.push(field.id+':newer-native-value');
            field.value=original;
            field.dispatchEvent(new CompositionEvent('compositionstart',{{bubbles:true,data:'日'}}));
            assert(field.dataset.composing==='true','native compositionstart guard is installed');
            assert(!clearDeliveredComposer(field.id,scope,original)&&field.value===original,'active composition prevents delivery clear');
            assert(!replaceReviewedComposer(field.id,scope,original,'Selected version')&&field.value===original,'active composition prevents review replacement');
            composerGuards.push(field.id+':active-composition');
            field.dispatchEvent(new CompositionEvent('compositionend',{{bubbles:true,data:'日本語'}}));
            assert(field.dataset.composing==='false','native compositionend resets guard');
            const selected='Reviewed local version 日本語';
            assert(replaceReviewedComposer(field.id,scope,original,selected)&&field.value===selected,'matching reviewed text replaces after composition ends');
            composerGuards.push(field.id+':matching-review-after-composition');
            field.value=original;
            assert(clearDeliveredComposer(field.id,scope,original)&&field.value==='','matching delivered text clears after composition ends');
            composerGuards.push(field.id+':matching-clear-after-composition');
          }}finally{{
            if(field.dataset.composing==='true')field.dispatchEvent(new CompositionEvent('compositionend',{{bubbles:true,data:''}}));
            field.value=original;
            if(style===null)field.removeAttribute('style');else field.setAttribute('style',style);
            field.setSelectionRange(start,end,direction);
          }}
          assert(field.value===original,'fixture native composer restored');
        }}
        assert(document.getElementById(composer).value==='A newer draft stays here.','fixture composer remains intact after guarded-clear checks');
        const bounds=[];
        // A narrow transcript cannot display all four file/text rows together.
        // Exercise ordinary scrolling and require each label to be reachable.
        container.dispatchEvent(new WheelEvent('wheel',{{deltaY:-1,bubbles:true}}));
        await pause();
        for(let index=0;index<4;index++){{
          const label=container.querySelectorAll('.pending-label')[index];
          assert(label,'status remains mounted while scrolling');
          label.scrollIntoView({{block:'center',behavior:'instant'}});
          container.dispatchEvent(new Event('scroll',{{bubbles:true}}));
          await new Promise(resolve=>setTimeout(resolve,150));
          const box=container.getBoundingClientRect(),r=label.getBoundingClientRect();
          bounds.push({{text:label.textContent.trim(),left:r.left,right:r.right,top:r.top,bottom:r.bottom}});
          assert(r.top>=box.top-1&&r.bottom<=box.bottom+1&&r.left>=box.left-1&&r.right<=box.right+1,'status fully visible after scrolling: '+label.textContent.trim());
        }}
        container.scrollTop=0;container.dispatchEvent(new Event('scroll',{{bubbles:true}}));await pause();
        dioxus.send({{ok:true,target,labels:texts,bounds,composerGuards,viewport:{{width:innerWidth,height:innerHeight}},composer:document.getElementById(composer).value}});
      }}catch(error){{dioxus.send({{ok:false,target,error:error.message}});}}
      await dioxus.recv();
    }})();"#,
        delivery_guard = include_str!("../composer_delivery.js"),
    ));
    let mut result = eval.recv::<serde_json::Value>().await.unwrap_or_default();
    result["native_status_count"] = serde_json::json!(state.peek().pending_delivery.len());
    if state.peek().pending_delivery.len() != 4 {
        result["ok"] = serde_json::json!(false);
    }
    let _ = eval.send("received");
    eprintln!(
        "send outcomes check {}: {}",
        if result["ok"] == true {
            "passed"
        } else {
            "failed"
        },
        result
    );
}
