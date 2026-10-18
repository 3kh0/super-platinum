async function scheduledForwardCheck(operation,args={}) {
  const fail=m=>{throw new Error(m);};
  const frame=()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
  const rect=e=>{const r=e.getBoundingClientRect();return{x:r.x,y:r.y,width:r.width,height:r.height};};
  const finite=r=>Object.values(r).every(Number.isFinite)&&r.width>0&&r.height>0;
  const inside=(a,b)=>a.x>=b.x&&a.y>=b.y&&a.x+a.width<=b.x+b.width&&a.y+a.height<=b.y+b.height;
  const shown=e=>e?.isConnected&&getComputedStyle(e).display!=='none'&&getComputedStyle(e).visibility==='visible'&&Number(getComputedStyle(e).opacity)>0;
  function visible(e) {
    if(!shown(e))return false;const r=rect(e);if(!finite(r)||!inside(r,{x:0,y:0,width:innerWidth,height:innerHeight}))return false;
    for(let p=e.parentElement;p;p=p.parentElement){const s=getComputedStyle(p),q=p.getBoundingClientRect();
      if(!shown(p))return false;
      if(/auto|scroll|hidden|clip/.test(s.overflowX)&&(r.x<q.left+p.clientLeft||r.x+r.width>q.left+p.clientLeft+p.clientWidth))return false;
      if(/auto|scroll|hidden|clip/.test(s.overflowY)&&(r.y<q.top+p.clientTop||r.y+r.height>q.top+p.clientTop+p.clientHeight))return false;
    }return true;
  }
  function evidence(e){
    const r=rect(e),hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2),style=getComputedStyle(e);
    const width=Number.parseFloat(style.outlineWidth),offset=Number.parseFloat(style.outlineOffset);
    const ring=Number.isFinite(width)&&width>0&&Number.isFinite(offset)&&!['none','hidden'].includes(style.outlineStyle)
      &&style.outlineColor!=='transparent'&&!/rgba\([^)]*,\s*0\s*\)/.test(style.outlineColor);
    const margin=ring?Math.max(0,width+offset):0;
    const paint={x:r.x-margin,y:r.y-margin,width:r.width+2*margin,height:r.height+2*margin};
    const clips=[];let paintVisible=visible(e)&&finite(paint)&&inside(paint,{x:0,y:0,width:innerWidth,height:innerHeight});
    for(let p=e.parentElement;p;p=p.parentElement){
      const s=getComputedStyle(p),q=p.getBoundingClientRect(),x=/auto|scroll|hidden|clip/.test(s.overflowX),y=/auto|scroll|hidden|clip/.test(s.overflowY);
      if(x||y){const box={x:q.left+p.clientLeft,y:q.top+p.clientTop,width:p.clientWidth,height:p.clientHeight};clips.push({id:p.id,x,y,box});
        if(!finite(box)||(x&&(paint.x<box.x||paint.x+paint.width>box.x+box.width))||(y&&(paint.y<box.y||paint.y+paint.height>box.y+box.height)))paintVisible=false;
      }
    }
    return{id:e.id,tag:e.tagName,text:e.textContent?.trim()||'',disabled:!!e.disabled,rect:r,visible:visible(e),hit:hit===e||e.contains(hit),active:document.activeElement===e,
      outline:{width,offset,style:style.outlineStyle,color:style.outlineColor},ring,paint,paintVisible,clips};
  }
  const root=()=>document.querySelector(args.root||'#forward-dialog')||fail('Exact current surface absent');
  const button=(scope,label)=>{const list=[...scope.querySelectorAll('button')].filter(e=>e.textContent.trim()===label);if(list.length!==1)fail('Expected one button: '+label);return list[0];};
  async function focus(e){if(!e?.isConnected||e.disabled)fail('Disabled/detached action');e.scrollIntoView({block:'center',inline:'nearest'});await frame();if(!visible(e))fail('Action clipped');e.focus({preventScroll:true});await frame();if(document.activeElement!==e||!visible(e))fail('Focus changed/clipped');const v=evidence(e);if(!v.hit)fail('Action occluded');return v;}
  if(operation==='ready'){
    const scope=root(),editor=scope.querySelector('#forward-note');if(!editor||editor.contentEditable!=='true')fail('Native editor not ready');
    const note=window.superPlatinumRichEditor.read(editor.id);return{ok:true,root:scope.id,owner:scope.dataset.scheduledForwardOwner,generation:scope.dataset.editorGeneration,note};
  }
  if(operation==='click'){
    const scope=root(),e=button(scope,args.label),before=await focus(e);if(root()!==scope)fail('Surface replaced before action');e.click();return{ok:true,label:args.label,root:scope.id,owner:scope.dataset.scheduledForwardOwner||null,before};
  }
  if(operation==='summary'){
    const scope=root(),items=[...scope.querySelectorAll('summary')].filter(e=>e.textContent.trim()===args.label);
    if(items.length!==1)fail('Expected one disclosure');const before=await focus(items[0]);items[0].click();await frame();
    if(!items[0].parentElement.open)fail('Observed version did not open');return{ok:true,label:args.label,before};
  }
  if(operation==='compose'){
    const scope=root(),e=scope.querySelector('#forward-note');const before=await focus(e);
    if(e.textContent.trim())fail('Fixture note was not initially empty');
    const r=document.createRange();r.selectNodeContents(e);r.collapse(false);const s=getSelection();s.removeAllRanges();s.addRange(r);
    for(const [command,value]of [['bold',null],['insertText','Native rich 日本語'],['bold',null],['insertText',' — keep é 🙂']]){
      if(!document.execCommand(command,false,value))fail('Native rich command refused');
    }
    await frame();if(root()!==scope||!e.isConnected)fail('Rich source replaced');
    return{ok:true,before,note:window.superPlatinumRichEditor.read(e.id),syntheticCommands:true};
  }
  if(operation==='select'){
    const scope=root(),field=scope.querySelector('#forward-destination');await focus(field);
    const setter=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set;setter.call(field,args.query);field.dispatchEvent(new Event('input',{bubbles:true}));await frame();
    const options=[...scope.querySelectorAll('[data-destination]')].filter(e=>e.dataset.destination===args.id);if(options.length!==1)fail('Exact picker choice missing');const before=await focus(options[0]);options[0].click();await frame();return{ok:true,id:args.id,before,selected:[...scope.querySelectorAll('.forward-chip')].map(e=>e.textContent.trim())};
  }
  if(operation==='date'){
    const scope=root(),field=scope.querySelector('input[type=datetime-local]');const before=await focus(field);
    const d=new Date(args.seconds*1000),p=n=>String(n).padStart(2,'0');
    const value=`${d.getFullYear()}-${p(d.getMonth()+1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(field,value);field.dispatchEvent(new Event('input',{bubbles:true}));await frame();
    return{ok:true,before,value,seconds:new Date(field.value).getTime()/1000};
  }
  if(operation==='private'){
    const scope=root(),e=scope.querySelector('.schedule-toggle input[type=checkbox]');const before=await focus(e);if(!e.checked)e.click();await frame();if(!e.checked)fail('Private confirmation missing');return{ok:true,before,checked:e.checked};
  }
  if(operation==='append'){
    const scope=root(),e=scope.querySelector('[contenteditable=true]');const before=await focus(e),r=document.createRange();r.selectNodeContents(e);r.collapse(false);const s=getSelection();s.removeAllRanges();s.addRange(r);
    if(!document.execCommand('insertText',false,args.text))fail('Native insertion failed');await frame();return{ok:true,before,note:window.superPlatinumRichEditor.read(e.id)};
  }
  if(operation==='inspect'){
    const scope=root(),active=document.activeElement;
    if(!scope.contains(active)||active===scope||active.disabled)fail('Focus escaped current surface');
    const focusBefore=evidence(active);if(!focusBefore.visible||!focusBefore.hit||!focusBefore.ring||!focusBefore.paintVisible)fail('Active target or complete focus ring clipped/occluded');
    const controls=[...scope.querySelectorAll('button,input,[contenteditable]')].map(e=>({id:e.id,text:e.textContent?.trim()||'',disabled:!!e.disabled,editable:e.contentEditable}));
    const editor=scope.querySelector('[contenteditable]');const note=editor?window.superPlatinumRichEditor.read(editor.id):null;
    await frame();if(root()!==scope||document.activeElement!==active||JSON.stringify(evidence(active))!==JSON.stringify(focusBefore))fail('Focus/surface/geometry changed while capturing');
    return{ok:true,root:scope.id,owner:scope.dataset.scheduledForwardOwner||null,generation:scope.dataset.editorGeneration||null,viewport:{width:innerWidth,height:innerHeight},focus:focusBefore,controls,note,text:scope.textContent.trim()};
  }
  if(operation==='focus')return{ok:true,before:await focus(button(root(),args.label))};
  fail('Unknown operation');
}
