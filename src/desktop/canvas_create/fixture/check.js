(() => {
  if (window.canvasCreationCheck) return;
  const dialog = () => document.querySelector('.canvas-create-dialog');
  const fields = () => [document.getElementById('canvas-create-title'), document.getElementById('canvas-create-body')];
  const button = label => [...(dialog()?.querySelectorAll('button') || [])].find(b => b.textContent.trim() === label);
  const rect = e => {const r=e.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
  const hit = e => {
    if (!e) return false;
    const r=rect(e),x=(r.left+r.right)/2,y=(r.top+r.bottom)/2;
    const actual=document.elementFromPoint(x,y);
    return r.width>0&&r.height>0&&x>=0&&y>=0&&x<innerWidth&&y<innerHeight&&!!actual&&(actual===e||e.contains(actual));
  };
  const set = (e, value, input) => {
    Object.getOwnPropertyDescriptor(e.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype,'value').set.call(e,value);
    if (input) e.dispatchEvent(new Event('input',{bubbles:true}));
  };
  let shortcutCount=0;
  window.canvasCreationCheck = (operation,args={}) => {
    const d=dialog(),[title,body]=fields();
    if(operation==='absent')return {ok:!d};
    if(operation==='viewport')return {ok:true,width:innerWidth,height:innerHeight};
    if(operation==='palette'){
      const p=document.querySelector('[data-dialog-kind="palette"]'),f=p?.querySelector('#overlay-input');
      if(d||!p||!f)return {ok:false,reason:'actual palette is missing or Canvas is still mounted'};
      const r=rect(p),bounded=r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1;
      return {ok:bounded&&p.scrollWidth<=p.clientWidth+1&&hit(f)&&document.activeElement===f,
        kind:'palette',dialog:r,clientWidth:p.clientWidth,scrollWidth:p.scrollWidth,
        noOverflow:p.scrollWidth<=p.clientWidth+1,footerHitTested:hit(f),
        viewport:{width:innerWidth,height:innerHeight},controls:[{label:'Channel or person',rect:rect(f),hit:hit(f)}],
        focused:document.activeElement===f,canvasAbsent:!d,shortcutCount};
    }
    if(operation==='leave-palette'){
      const p=document.querySelector('[data-dialog-kind="palette"]'),f=p?.querySelector('#overlay-input');
      if(d||!p||!f||!hit(f)||document.activeElement!==f)return {ok:false,reason:'palette field not current'};
      f.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
      return {ok:true};
    }
    if(operation==='reopen-ready'||operation==='reopen'){
      if(document.querySelector('[data-keyboard-dialog]'))return {ok:false,reason:'another dialog still mounted'};
      const b=[...document.querySelectorAll('.files-header button')].find(e=>e.textContent.trim()==='New canvas');
      if(!b||b.disabled)return {ok:false,reason:'actual New canvas control missing'};
      if(operation==='reopen-ready'){const ready=hit(b);return {ok:ready,reason:ready?undefined:'New canvas is covered'};}
      b.scrollIntoView({block:'nearest'});b.focus();
      if(!hit(b))return {ok:false,reason:'New canvas is covered'};
      b.click();return {ok:true};
    }
    if(!d||!title||!body)return {ok:false,reason:'missing native editor'};
    if(operation==='palette-shortcut'){
      body.scrollIntoView({block:'nearest'});body.focus();
      if(!hit(body))return {ok:false,reason:'shortcut source is covered'};
      const event=new KeyboardEvent('keydown',{key:'k',code:'KeyK',ctrlKey:true,bubbles:true,cancelable:true});
      shortcutCount++;body.dispatchEvent(event);
      return {ok:event.defaultPrevented,count:shortcutCount,key:event.key,ctrl:event.ctrlKey,
        meta:event.metaKey,trusted:event.isTrusted,prevented:event.defaultPrevented,visit:body.dataset.canvasVisit};
    }
    if(operation==='read')return {ok:!args.editable||(!title.readOnly&&!body.readOnly&&!title.disabled&&!body.disabled),title:title.value,markdown:body.value,titleReadonly:title.readOnly,bodyReadonly:body.readOnly,titleDisabled:title.disabled,bodyDisabled:body.disabled,
      visit:title.dataset.canvasVisit,codePoints:[...body.value].map(c=>c.codePointAt(0)),utf16Length:body.value.length,
      checked:[...d.querySelectorAll('input[type=radio]:checked')].length,error:d.querySelector('[role=alert]')?.textContent||'',
      status:[...d.querySelectorAll('[role=status]')].map(e=>e.textContent),created:d.textContent.includes('Your canvas is ready.')};
    if(operation==='input'){
      for(const [e,value] of [[title,args.title],[body,args.markdown]]) {
        if(typeof value!=='string')continue;
        if(e.disabled||e.readOnly)return {ok:false,reason:'field is not editable'};
        e.scrollIntoView({block:'nearest'});e.focus();
        if(!hit(e))return {ok:false,reason:'editable field is covered'};
      }
      if(typeof args.title==='string')set(title,args.title,args.dispatch!==false);
      if(typeof args.markdown==='string')set(body,args.markdown,args.dispatch!==false);
      return {ok:true};
    }
    if(operation==='composition-start'){
      if(body.disabled||body.readOnly)return {ok:false,reason:'composition field is not editable'};
      body.scrollIntoView({block:'nearest'});body.focus();
      if(!hit(body))return {ok:false,reason:'composition field is covered'};
      body.focus();body.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:'\u{65e5}'}));
      set(body,args.markdown,false);return {ok:true};
    }
    if(operation==='composition-end'){
      body.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'\u{65e5}'}));
      body.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true};
    }
    if(operation==='click'){
      const b=args.label==='Close header'?d.querySelector('[aria-label="Close new canvas"]'):button(args.label);
      if(!b||b.disabled)return {ok:false,reason:'missing or disabled '+args.label};
      b.scrollIntoView({block:'nearest'});b.focus();
      if(!hit(b))return {ok:false,reason:'covered '+args.label,rect:rect(b)};
      b.click();return {ok:true};
    }
    if(operation==='review'){
      const radios=[...d.querySelectorAll('input[type=radio]')];
      return {ok:radios.length===args.count&&radios.every(r=>!r.checked),radios:radios.length,checked:radios.filter(r=>r.checked).length};
    }
    if(operation==='disclosure'){
      const label=args.kind==='canonical'?'Current saved canvas content':'Retained local canvas content';
      const e=d.querySelector('textarea[aria-label="'+label+'"]');
      if(!e||e.value!==args.markdown)return {ok:false,reason:'exact disclosure not rendered'};
      e.scrollIntoView({block:'nearest'});
      return {ok:hit(e),markdown:e.value,readonly:e.readOnly};
    }
    if(operation==='choose-retained'){
      const radios=[...d.querySelectorAll('input[type=radio]')];
      const r=radios[1];if(!r||r.disabled)return {ok:false,reason:'missing retained choice'};
      r.scrollIntoView({block:'center'});r.focus();if(!hit(r))return {ok:false,reason:'covered retained choice'};r.click();return {ok:true};
    }
    if(operation==='geometry'){
      const r=rect(d),footer=[...d.querySelectorAll('footer button')].filter(b=>!b.disabled);
      const bounded=r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1;
      return {ok:bounded&&d.scrollWidth<=d.clientWidth+1&&footer.length>0&&footer.every(hit),dialog:r,
        clientWidth:d.clientWidth,scrollWidth:d.scrollWidth,noOverflow:d.scrollWidth<=d.clientWidth+1,footerHitTested:footer.every(hit),viewport:{width:innerWidth,height:innerHeight},
        controls:footer.map(b=>({label:b.textContent.trim(),rect:rect(b),hit:hit(b)}))};
    }
    return {ok:false,reason:'unknown check operation'};
  };
})();
