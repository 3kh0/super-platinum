// Bounded checks on actual mounted elements. No controller/Owner calls or fake outcomes.
async function managedScheduleCheck(input) {
  await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
  const dialog=document.getElementById('managed-schedule-dialog');
  const editor=document.getElementById('managed-schedule-message');
  const date=document.getElementById('managed-schedule-date');
  const body=document.getElementById('managed-schedule-body');
  const field=document.getElementById(`${input.target}-composer`);
  const panel=document.querySelector('.scheduled-panel');
  const viewport=[innerWidth,innerHeight];
  const rect=element=>{if(!element)return null;const r=element.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height};};
  const visible=r=>!!r&&r.width>0&&r.height>0&&r.x>=-1&&r.y>=-1&&r.x+r.width<=innerWidth+1&&r.y+r.height<=innerHeight+1;
  function measure(element) {
    const r=rect(element),points=[];
    if(r)for(const x of [.15,.5,.85])for(const y of [.15,.5,.85]) {
      const px=r.x+r.width*x,py=r.y+r.height*y,hit=document.elementFromPoint(px,py);
      points.push({x:px,y:py,owned:!!hit&&element.contains(hit),hit:hit?{tag:hit.tagName,id:hit.id||'',classes:typeof hit.className==='string'?hit.className:''}:null});
    }
    return {rect:r,visible:visible(r),points,unobscured:points.length===9&&points.every(p=>p.owned),disabled:!!element?.disabled,text:element?.textContent||'',focused:document.activeElement===element};
  }
  const byText=(root,text)=>[...root.querySelectorAll('button')].find(button=>button.textContent.trim()===text);
  const read=()=>({blocks:window.superPlatinumRichEditor.read('managed-schedule-message'),date:date.value,postAt:Math.floor(Date.parse(date.value)/1000),generation:dialog.dataset.editorGeneration,composing:dialog.dataset.composing});
  if(input.op==='retain-review') {
    const submit=dialog&&byText(dialog,input.label);
    if(window.__managedScheduleRetirement)return {ok:false,error:'Old review proof not consumed'};
    const primary=measure(submit);
    if(!dialog?.isConnected||!submit?.isConnected||!primary.visible||!primary.unobscured||primary.disabled)
      return {ok:false,error:'Connected reviewed submit required',primary};
    const generation=dialog.dataset.editorGeneration;
    if(!generation)return {ok:false,error:'Review generation required'};
    window.__managedScheduleRetirement={dialog,submit,generation};
    return {ok:true,generation,dialogConnected:dialog.isConnected,submitConnected:submit.isConnected,primary,viewport,documentWidth:document.documentElement.scrollWidth};
  }
  if(input.op==='retire-review') {
    const retained=window.__managedScheduleRetirement;
    if(!retained)return {ok:false,error:'Retained review references required'};
    try {
      for(let attempt=0;attempt<60;attempt++) {
        const proof={generation:retained.generation,dialogDisconnected:!retained.dialog.isConnected,
          submitDisconnected:!retained.submit.isConnected,currentDialogAbsent:!document.getElementById('managed-schedule-dialog')};
        if(proof.dialogDisconnected&&proof.submitDisconnected&&proof.currentDialogAbsent)return {ok:true,...proof};
        await new Promise(resolve=>requestAnimationFrame(resolve));
      }
      return {ok:false,error:'Old review did not retire completely'};
    } finally {delete window.__managedScheduleRetirement;}
  }
  if(input.op==='dismiss-notice') {
    const toast=document.getElementById('status-toast');
    const button=toast?.querySelector('.toast-dismiss[aria-label="Dismiss notification"]');
    const text=toast?.querySelector('.toast-text')?.textContent;
    if(!toast||!button||text!==input.expected||toast.getAttribute('role')!=='status')return {ok:false,error:'Expected completion notice unavailable'};
    const before={text,role:toast.getAttribute('role'),toast:measure(toast),dismiss:measure(button),viewport,documentWidth:document.documentElement.scrollWidth};
    if(!before.toast.visible||!before.toast.unobscured||!before.dismiss.visible||!before.dismiss.unobscured||before.dismiss.disabled)return {ok:false,error:'Completion notice dismiss is clipped or covered',before};
    button.focus({preventScroll:true});
    if(document.getElementById('status-toast')!==toast||toast.querySelector('.toast-text')?.textContent!==text)return {ok:false,error:'Completion notice changed before dismissal'};
    // Exercise the existing UI handler. Fixtures intentionally freeze toast expiry.
    button.click();
    for(let attempt=0;attempt<60;attempt++) {
      await new Promise(resolve=>requestAnimationFrame(resolve));
      if(!document.getElementById('status-toast'))return {ok:true,before,clicked:true,afterAbsent:true};
    }
    return {ok:false,error:'Completion notice did not dismiss',before,clicked:true,afterAbsent:false};
  }
  const bodySample=()=>({generation:dialog.dataset.editorGeneration,scrollTop:body.scrollTop,scrollHeight:body.scrollHeight,clientHeight:body.clientHeight,rect:rect(body)});
  const inside=(child,parent)=>child&&parent&&child.x>=parent.x-1&&child.y>=parent.y-1&&child.x+child.width<=parent.x+parent.width+1&&child.y+child.height<=parent.y+parent.height+1;
  if(input.op==='reveal') {
    if(!dialog||!body||!['editor','date'].includes(input.view))return {ok:false,error:'Current dialog scroll body required'};
    const before=bodySample();
    // Reveal only this dialog's body. Never scroll an ancestor or replace focus.
    body.scrollTop=input.view==='editor'?0:body.scrollHeight;
    await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
    const after=bodySample();
    return {ok:body.isConnected&&dialog.isConnected&&before.generation===after.generation,requested:input.view,before,after};
  }
  if(input.op==='closed')return {ok:!dialog,closed:!dialog};
  if(input.op==='panel-ready')return {ok:!!panel&&!document.querySelector('.thread-pane'),threadPaneAbsent:!document.querySelector('.thread-pane')};
  if(input.op==='schedule-control') {
    const button=field?.closest('footer')?.querySelector('[aria-label="Schedule or save draft"]');
    const proof=measure(button);const ok=proof.visible&&proof.unobscured;
    if(ok){button.focus({preventScroll:true});button.click();}
    return {ok,clicked:ok&&!proof.disabled,control:proof,viewport,documentWidth:document.documentElement.scrollWidth};
  }
  if(input.op==='open') {
    const button=input.rejected?byText(document,'Review schedule'):field?.closest('footer')?.querySelector('[aria-label="Schedule or save draft"]');
    if(!button)return {ok:false,error:'Actual scheduling control absent'};
    const proof=measure(button);const ok=proof.visible&&proof.unobscured&&!proof.disabled;
    if(ok){button.focus({preventScroll:true});button.click();}
    return {ok,clicked:ok,control:proof,viewport,documentWidth:document.documentElement.scrollWidth};
  }
  if(input.op==='click') {
    const button=dialog&&byText(dialog,input.label);
    if(!button)return {ok:false,error:'Actual dialog button absent',label:input.label};
    const proof=measure(button),allowed=proof.visible&&proof.unobscured;
    if(allowed){button.focus({preventScroll:true});button.click();}
    return {ok:allowed,clicked:allowed&&!proof.disabled,control:proof,viewport,documentWidth:document.documentElement.scrollWidth};
  }
  if(input.op==='close') {
    const button=dialog?.querySelector('[aria-label="Close scheduling"]');
    if(!button||button.disabled)return {ok:false,error:'Close unavailable'};
    button.focus({preventScroll:true});button.click();return {ok:true};
  }
  if(input.op==='edit') {
    if(!dialog||!editor||editor.contentEditable!=='true')return {ok:false,error:'Mounted editable rich input required'};
    // Genuine native input path; retain the component's WeakMap ownership.
    if(input.change) {
      const walker=document.createTreeWalker(editor,NodeFilter.SHOW_TEXT);
      let node=walker.nextNode();
      while(node&&node.parentElement.closest('[contenteditable=false]'))node=walker.nextNode();
      if(!node)return {ok:false,error:'Frozen visible text absent'};
      node.textContent='Changed rich schedule ';
    } else {
      const paragraph=document.createElement('div'),bold=document.createElement('strong');
      bold.textContent='Frozen schedule \u2014 \u65e5\u672c\u8a9e \u{1f680} e\u0301';paragraph.append(bold);
      const quote=document.createElement('blockquote'),italic=document.createElement('em');italic.textContent='Quoted detail';quote.append(italic);
      const list=document.createElement('ol');list.start=3;const item=document.createElement('li');item.textContent='List item';list.append(item);
      editor.replaceChildren(paragraph,quote,list);
    }
    editor.focus({preventScroll:true});
    editor.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText',data:null}));
    return {ok:true,...read()};
  }
  if(input.op==='time') {
    if(!date||date.disabled)return {ok:false,error:'Editable schedule date required'};
    const time=new Date((Math.floor(Date.now()/60000)+120)*60000);
    const pad=value=>String(value).padStart(2,'0');
    const value=`${time.getFullYear()}-${pad(time.getMonth()+1)}-${pad(time.getDate())}T${pad(time.getHours())}:${pad(time.getMinutes())}`;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(date,value);
    date.dispatchEvent(new Event('input',{bubbles:true}));
    return {ok:true,...read()};
  }
  if(input.op==='read') {
    if(!dialog||!editor||!date||!date.value||dialog.dataset.composing!=='false')return {ok:false,error:'Current rich editor mount unavailable'};
    return {ok:true,...read()};
  }
  if(input.op==='measure') {
    const surface=dialog?'dialog':panel?'panel':'composer';
    const elements=dialog?{dialog,header:dialog.querySelector(':scope > header'),title:dialog.querySelector('#managed-schedule-title'),destination:dialog.querySelector(':scope > p.scheduled-hint'),footer:dialog.querySelector(':scope > footer'),body,editor,editorLabel:dialog.querySelector('#managed-schedule-message-label'),toolbar:editor?.previousElementSibling,date,dateLabel:dialog.querySelector('label[for="managed-schedule-date"]'),hint:dialog.querySelector('#managed-schedule-hint'),primary:dialog.querySelector('footer button.primary'),close:dialog.querySelector('[aria-label="Close scheduling"]')}:
      panel?{panel,header:panel.querySelector('header'),row:panel.querySelector('.scheduled-item')||panel.querySelector('.local-draft-item')||panel.querySelector('#managed-draft-recovery')}:
      {field,footer:field?.closest('footer'),schedule:field?.closest('footer')?.querySelector('[aria-label="Schedule or save draft"]')};
    const controls=Object.fromEntries(Object.entries(elements).map(([name,element])=>[name,measure(element)]));
    const bodyView=dialog?input.view:null;
    const selected=bodyView==='editor'?['editor','editorLabel','toolbar']:['date','dateLabel','hint'];
    const required=dialog?['dialog','header','title','destination','footer','primary','close','body',...selected]:Object.keys(controls);
    const valid=required.every(name=>controls[name].visible&&controls[name].unobscured)
      &&(!dialog||(['editor','date'].includes(bodyView)&&selected.every(name=>inside(controls[name].rect,controls.body.rect))));
    const current=dialog?read():null;
    return {ok:valid,surface,controls,viewport,documentWidth:document.documentElement.scrollWidth,current,bodyView,bodyScroll:null,threadPaneAbsent:surface==='panel'?!document.querySelector('.thread-pane'):null,
      // BODY/HTML text includes injected styles and runtime script source; it is
      // not focused control content and must not balloon fixture diagnostics.
      active:{id:document.activeElement?.id||'',tag:document.activeElement?.tagName||'',text:['BODY','HTML'].includes(document.activeElement?.tagName)?'':(document.activeElement?.textContent||'').slice(0,512)},
      text:(dialog||panel||field?.closest('footer'))?.textContent||'',busy:dialog?.getAttribute('aria-busy')||null};
  }
  return {ok:false,error:'Unsupported Schedule DOM operation'};
}
