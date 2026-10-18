// Closed disconnected-fixture DOM adapter. Events are synthetic, never OS input.
(() => {
  if (Object.hasOwn(window, '__listRefinementSave')) return {ok:false,reason:'already-installed'};
  const rootSelector='.slack-list-dialog';
  const scope=()=>owner?.node||document.querySelector(rootSelector);
  const inside=(e,root=scope())=>!!e&&!!root&&e!==root&&root.contains(e);
  const q=s=>{const e=document.querySelector(s);return s===rootSelector||inside(e)?e:null;};
  const all=s=>{const nodes=[...document.querySelectorAll(s)];return s==='[role="dialog"]'?nodes:nodes.filter(e=>inside(e));};
  const same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
  const live=e=>!!e&&e.isConnected&&!e.closest('[hidden],[inert]');
  const rect=e=>{const r=e?.getBoundingClientRect();return r?[r.x,r.y,r.width,r.height]:null;};
  const find=()=>q('.slack-list-find input');
  function box(e, ring = false) {
    if (!live(e)) return {ok: false, rect: rect(e), reason: 'not-live'};
    const r = e.getBoundingClientRect(), clip = [0, 0, innerWidth, innerHeight], ancestors = [];
    let shown = r.width > 0 && r.height > 0;
    for (let p = e; p; p = p.parentElement) {
      const s = getComputedStyle(p), b = p.getBoundingClientRect();
      if (s.display === 'none' || ['hidden', 'collapse'].includes(s.visibility)) shown = false;
      if (p !== e) {
        const x = /auto|scroll|hidden|clip/.test(s.overflowX), y = /auto|scroll|hidden|clip/.test(s.overflowY);
        // Overflow clips at the client area: borders and scrollbars are not visible space.
        // Client metrics have no transform, so decline transformed/zoomed ancestry.
        if (s.transform !== 'none' || !['1', 'normal'].includes(String(s.zoom))) shown = false;
        const client = [p.clientLeft, p.clientTop, p.clientWidth, p.clientHeight];
        const bounds = [b.left + client[0], b.top + client[1], b.left + client[0] + client[2], b.top + client[1] + client[3]];
        if (x || y) ancestors.push({id: p.id || null, x, y, border: [b.left, b.top, b.right, b.bottom], client, bounds});
        if (x) { clip[0] = Math.max(clip[0], bounds[0]); clip[2] = Math.min(clip[2], bounds[2]); }
        if (y) { clip[1] = Math.max(clip[1], bounds[1]); clip[3] = Math.min(clip[3], bounds[3]); }
      }
    }
    const style = getComputedStyle(e), width = parseFloat(style.outlineWidth), offset = parseFloat(style.outlineOffset);
    const outlined = Number.isFinite(width) && width > 0 && Number.isFinite(offset) && style.outlineStyle !== 'none';
    const delta = ring && outlined ? Math.max(0, width + offset) : 0;
    const edges = [r.left - delta, r.top - delta, r.right + delta, r.bottom + delta];
    const ok = shown && (!ring || outlined) && [...edges, ...clip].every(Number.isFinite)
      && ancestors.every(a => a.bounds.every(Number.isFinite))
      && edges[0] >= clip[0] && edges[1] >= clip[1] && edges[2] <= clip[2] && edges[3] <= clip[3];
    return {ok, rect: rect(e), edges, clip, ancestors, outline: ring ? {width, offset, style: style.outlineStyle} : null};
  }
  // Find uses the existing design-system border and three-pixel shadow, not an outline.
  function findFocus(e) {
    const geometry=box(e),style=e?getComputedStyle(e):null;
    const shadow=style?.boxShadow?.match(/^rgba?\(([^)]+)\)\s+0px\s+0px\s+0px\s+3px$/);
    const color=shadow?.[1].split(/[\s,\/]+/).filter(Boolean).map(Number);
    const painted=color?.length===4&&same(color,[50,140,255,.1]);
    const borders=['Top','Right','Bottom','Left'].map(side=>({side,width:parseFloat(style?.[`border${side}Width`]),style:style?.[`border${side}Style`]||null,color:style?.[`border${side}Color`]||null}));
    const border=borders.every(side=>{
      const color=side.color?.match(/^rgba?\(([^)]+)\)$/)?.[1].split(/[\s,\/]+/).filter(Boolean).map(Number);
      return side.width===1&&side.style==='solid'&&color?.every(Number.isFinite)
        &&(color.length===3||color.length===4&&color[3]>0&&color[3]<=1)
        &&color.slice(0,3).every(v=>v>=0&&v<=255);
    });
    const r=geometry.rect,edges=r?[r[0]-3,r[1]-3,r[0]+r[2]+3,r[1]+r[3]+3]:null,clip=geometry.clip;
    return {kind:'find-border-shadow',ok:e===find()&&document.activeElement===e&&geometry.ok&&painted===true&&border&&!!edges
      &&edges[0]>=clip[0]&&edges[1]>=clip[1]&&edges[2]<=clip[2]&&edges[3]<=clip[3],
      focused:document.activeElement===e,geometry,edges,shadow:style?.boxShadow||null,painted:painted===true,border,borders};
  }
  function focusRing(e) { return e===find()?findFocus(e):box(e,true); }
  function hit(e) {
    if (!box(e).ok) return false;
    const r = e.getBoundingClientRect();
    return [.1, .5, .9].every(x => [.1, .5, .9].every(y => {
      const top = document.elementFromPoint(r.left + r.width * x, r.top + r.height * y);
      return top === e || e.contains(top);
    }));
  }
  const queryText='Launch item 0', created='V_REFINED_CREATED';
  const views=[['','All loaded items'],['V_PRECISION','Estimates'],['V_LANE','Delivery lanes'],['V_PRECISION_BOARD','Estimate board']];
  const serialValue=v=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=0?String(v):typeof v==='string'&&/^(0|[1-9][0-9]*)$/.test(v)?v:null;
  const fail=reason=>({ok:false,reason});
  let owner=null,panel=null,serial=null,panelKind=null,pendingPanel=null,opener=null;
  let findNode=null,findRange=null,queryNode=null,action=null,busy=null,selectionProof=null,statusNode=null,dragging=false;
  let fieldsReturns=0,applied=false;const events=[];
  const topDialog=e=>all('[role="dialog"]').filter(n=>live(n)&&n.getBoundingClientRect().width>0).at(-1)===e;
  function stamp(){return {visit:owner?.visit||null,selection:owner?.selection||null,view:owner?.view||null,serial,kind:panelKind,phase:panel?.dataset.renameStage||null,statusSerial:q('#slack-list-refinement-save-status')?.dataset.refinementSerial||null};}
  function owned(){const r=q('.slack-list-dialog');return !!owner&&r===owner.node&&live(r)&&r.dataset.dialogGeneration===owner.visit&&r.dataset.viewSelection===owner.selection&&q('#slack-list-view-select')===owner.picker&&owner.picker.value===owner.view;}
  function custody(){const currentStatus=q('#slack-list-refinement-save-status'),status=!statusNode?!currentStatus:currentStatus===statusNode&&live(statusNode)&&statusNode.dataset.refinementSerial===selectionProof?.serial||!currentStatus&&!statusNode.isConnected&&action?.selector==='#slack-list-refinement-status-close';return owned()&&status&&!pendingPanel&&q('#slack-list-rename')===panel&&(!panel||live(panel)&&panel.dataset.renameSerial===serial)&&topDialog(panel||owner.node);}
  function retained(){const e=find(),range=e?[e.selectionStart,e.selectionEnd,e.selectionDirection]:null;
    const gone=!!selectionProof&&!e&&!!findNode&&!findNode.isConnected;
    return {ok:owned()&&(gone||live(e)&&e===findNode&&e.value===queryText&&same(range,findRange)),sameNode:!!e&&e===findNode,oldDisconnected:!!findNode&&!findNode.isConnected,absent:!e,value:e?.value??null,range};}
  function projection(){return all('.slack-list-lane').map(l=>({key:l.dataset.groupKey,label:l.getAttribute('aria-label'),count:Number(l.dataset.count),rows:[...l.querySelectorAll('.slack-list-card')].map(r=>({id:r.dataset.rowId,instance:r.dataset.instanceId,cells:[...r.querySelectorAll('[data-column-id]')].map(c=>({column:c.dataset.columnId,text:c.querySelector('.slack-list-cell-text')?.textContent||''}))}))}));}
  function rowsMatch(rows,moved,hidden){const progress=rows.find(r=>r.key==='OPT_PROGRESS'),done=rows.find(r=>r.key==='OPT_DONE');return same(rows.map(r=>r.key),moved?['OPT_DONE','OPT_PROGRESS']:['OPT_PROGRESS','OPT_DONE'])&&progress?.label==='In progress'&&progress.count===9&&done?.label==='Done'&&done.count===0&&done.rows.length===0&&progress.rows.length===9&&progress.rows.every((r,i)=>r.id===`R_LIST_${String(i).padStart(3,'0')}`&&same(r.cells.map(c=>c.column),hidden?['C_NAME']:['C_NAME','C_NUMBER'])&&r.cells[0].text===`Launch item ${String(i+1).padStart(2,'0')}`);}
  function reveal(e,reason){const active=document.activeElement,before=stamp();e.scrollIntoView({block:'nearest',inline:'nearest'});const result={index:events.length,kind:'reveal',reason,id:e.id||null,before,after:stamp(),sameActiveElement:active===document.activeElement,box:box(e)};events.push(result);return {...result,ok:result.sameActiveElement&&same(before,stamp())&&box(e).ok};}
  function focusProof(stage,mode){const active=document.activeElement,enabled=e=>live(e)&&!e.disabled;
    let ok=false,reason='unowned';
    if(panel){
      const retainedAction=enabled(action?.node)&&panel.contains(action.node)?action.node:enabled(busy)&&panel.contains(busy)?busy:null;
      if(stage==='fields'){ok=active===q('#slack-list-field-visible');reason='actual-visibility-choice';}
      else if(stage==='name'){ok=active===q('#slack-list-rename-name');reason='actual-name-input';}
      else {const ids=['slack-list-rename-leave','slack-list-rename-check','slack-list-rename-close','slack-list-new-view-open'];ok=enabled(active)&&panel.contains(active)&&ids.includes(active.id)&&(!retainedAction||active===retainedAction)&&!!action;reason=retainedAction?'retained-action-or-busy':'disabled-action-recovery';}
    }else if(stage==='baseline'||stage==='local'){ok=active===findNode&&retained().ok;reason='retained-find';}
    else if(selectionProof){ok=active===owner?.picker;reason='actual-selected-picker';}
    else {ok=active===opener&&enabled(opener)||active===owner?.picker;reason='actual-owned-return';}
    return {ok:ok&&enabled(active),reason,id:active?.id||null,sameAction:!!action&&active===action.node,sameBusy:!!busy&&active===busy,ring:focusRing(active)};
  }
  function actionReady(){if(!custody()||!retained().ok||dragging)return false;const active=document.activeElement,enabled=e=>live(e)&&!e.disabled;
    let focused=false;
    if(panel&&panel.dataset.renameStage==='Editing')focused=panelKind==='fields'?[q('#slack-list-field-choice'),q('#slack-list-field-visible'),action?.node].includes(active):[q('#slack-list-rename-name'),action?.node].includes(active);
    else if(panel){const prior=enabled(action?.node)&&panel.contains(action.node)?action.node:enabled(busy)&&panel.contains(busy)?busy:null;focused=prior?active===prior:!!action&&['slack-list-rename-leave','slack-list-rename-check','slack-list-new-view-open','slack-list-rename-close'].includes(active?.id);}
    else focused=active===findNode||active===opener||active===owner.picker||active===action?.node&&['#slack-list-refinement-status-check','#slack-list-refinement-status-close'].includes(action.selector);
    return focused&&live(active)&&!active.disabled&&(panel?panel.contains(active):owner.node.contains(active))&&focusRing(active).ok;}
  function targetOwned(e,selector){return inside(e,owner?.node)&&(panel?inside(e,panel):true)&&(!['#slack-list-refinement-status-check','#slack-list-refinement-status-close'].includes(selector)||statusNode===q('#slack-list-refinement-save-status')&&inside(e,statusNode));}
  function prepare(selector){if(!actionReady())return null;const e=q(selector);if(!live(e)||e.disabled||!targetOwned(e,selector))return null;
    const before=stamp(),active=document.activeElement,revealed=reveal(e,'action');
    if(!revealed.ok||!custody()||!retained().ok||q(selector)!==e||!targetOwned(e,selector)||document.activeElement!==active||!same(before,stamp())||!hit(e))return null;
    e.focus();if(document.activeElement!==e||!focusRing(e).ok||!custody()||!retained().ok||q(selector)!==e||!targetOwned(e,selector)||!same(before,stamp()))return null;
    action={node:e,selector,serial,owner:before};return {e,before,revealed};
  }
  const api={
    ready(){const root=q('.slack-list-dialog'),picker=q('#slack-list-view-select'),e=find(),rows=projection();
      const ok=!owner&&live(root)&&topDialog(root)&&!q('#slack-list-rename')&&root.dataset.listId==='F_LIST_FIXTURE'&&serialValue(root.dataset.dialogGeneration)!==null&&serialValue(root.dataset.viewSelection)!==null&&picker instanceof HTMLSelectElement&&picker.value==='V_LANE'&&same([...picker.options].map(o=>[o.value,o.textContent]),views)&&e instanceof HTMLInputElement&&!e.disabled&&rows.length===2&&rows[0].key==='OPT_PROGRESS'&&rows[0].count===71&&rows[0].rows.length===10&&rows[1].key==='OPT_DONE'&&rows[1].count===0&&!q('#slack-list-refinement-save')&&!q('#slack-list-refinement-save-new');
      return {ok,visit:root?.dataset.dialogGeneration||null,selection:root?.dataset.viewSelection||null,view:picker?.value||null,rows,viewport:[innerWidth,innerHeight]};
    },
    query(){if(!this.ready().ok)return fail('initial-layout');const root=q('.slack-list-dialog'),picker=q('#slack-list-view-select'),e=find(),active=document.activeElement,visit=root.dataset.dialogGeneration,selection=root.dataset.viewSelection;
      const current=()=>root===q('.slack-list-dialog')&&root.dataset.dialogGeneration===visit&&root.dataset.viewSelection===selection&&inside(e,root)&&inside(picker,root)&&find()===e&&q('#slack-list-view-select')===picker&&picker.value==='V_LANE'&&topDialog(root);
      if(!current())return fail('query-owner');e.scrollIntoView({block:'nearest',inline:'nearest'});if(document.activeElement!==active||!current()||!hit(e))return fail('query-reveal');
      owner={node:root,picker,visit,selection,view:'V_LANE'};queryNode=e;e.focus();
      if(!current()||!owned()||!focusRing(e).ok)return fail('query-focus');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,queryText);e.dispatchEvent(new Event('input',{bubbles:true}));
      events.push({kind:'input',selector:'.slack-list-find input',value:queryText,owner:stamp(),trusted:false});return {ok:true,owner:stamp(),trusted:false};
    },
    bindFind(options={}){if(!options||typeof options!=='object'||Array.isArray(options))return fail('find-options');const e=find(),remount=options.remount===true;if(Object.keys(options).some(k=>k!=='remount')||options.remount!==undefined&&typeof options.remount!=='boolean')return fail('find-options');
      const active=document.activeElement,range=e?[e.selectionStart,e.selectionEnd,e.selectionDirection]:null;
      const bounded=range&&Number.isInteger(range[0])&&Number.isInteger(range[1])&&range[0]>=0&&range[0]<=range[1]&&range[1]<=queryText.length&&['forward','backward','none'].includes(range[2]);
      const oldDisconnected=!!findNode&&!findNode.isConnected,newNode=!!findNode&&e!==findNode;
      const binding=remount?!!selectionProof&&oldDisconnected&&newNode:e===queryNode&&!findNode&&active===e&&rowsMatch(projection(),false,false);
      if(!owned()||!live(e)||e.value!==queryText||!bounded||!binding)return fail('find-binding');
      findNode=e;findRange=range;return {ok:true,remount,oldDisconnected,newNode,value:e.value,range,sameActiveElement:active===document.activeElement,focused:active?.id||null};
    },
    bindPanel(proof){const p=q('#slack-list-rename'),s=serialValue(proof?.serial),kind=proof?.kind;
      if(!owned()||!retained().ok||!pendingPanel||pendingPanel!==kind||!['fields','name','save'].includes(kind)||!live(p)||!topDialog(p)||s===null||p.dataset.renameSerial!==s||p.dataset.renameView!=='V_LANE')return fail('panel-owner');
      const valid=kind==='fields'?p.dataset.localRefinement==='true'&&p.dataset.visibilityView==='V_LANE'&&p.dataset.renameStage==='Editing':kind==='name'?p.dataset.refinementSave==='new'&&p.dataset.renameStage==='Editing':p.dataset.refinementSave==='existing'&&['Preparing','Saving'].includes(p.dataset.renameStage);
      if(!valid)return fail('panel-kind');panel=p;serial=s;panelKind=kind;pendingPanel=null;busy=null;return {ok:true,owner:stamp(),title:q('#slack-list-rename-title')?.textContent||null};
    },
    choose(selector,value){if(panelKind!=='fields'||panel?.dataset.renameStage!=='Editing'||!((selector==='#slack-list-field-choice'&&value==='C_NUMBER')||(selector==='#slack-list-field-visible'&&['shown','hidden'].includes(value))))return fail('choice-allowlist');
      const e=q(selector);if(!(e instanceof HTMLSelectElement)||![...e.options].some(o=>o.value===value))return fail('choice-options');const step=prepare(selector);if(!step)return fail('choice-custody');
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,value);e.dispatchEvent(new Event('change',{bubbles:true}));events.push({kind:'change',selector,value,owner:step.before,trusted:false});return {ok:true,selector,value,owner:step.before,revealed:step.revealed,trusted:false};
    },
    name(value){if(value!=='Focused delivery'||panelKind!=='name'||panel?.dataset.renameStage!=='Editing')return fail('name-allowlist');const step=prepare('#slack-list-rename-name');if(!step||!(step.e instanceof HTMLInputElement))return fail('name-custody');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(step.e,value);step.e.dispatchEvent(new Event('input',{bubbles:true}));events.push({kind:'input',selector:'#slack-list-rename-name',value,owner:step.before,trusted:false});return {ok:true,value,owner:step.before,trusted:false};
    },
    click(selector){const allowed=panel?['#slack-list-rename-close','#slack-list-rename-confirm','#slack-list-rename-check','#slack-list-new-view-open']:['#slack-list-fields','#slack-list-refinement-save','#slack-list-refinement-save-new','#slack-list-refinement-status-check','#slack-list-refinement-status-close'];
      if(!allowed.includes(selector))return fail('click-allowlist');
      if(selector==='#slack-list-fields'&&applied||selector==='#slack-list-rename-close'&&panelKind==='fields'&&fieldsReturns!==0)return fail('fields-sequence');
      if(selector==='#slack-list-rename-confirm'&&(panelKind==='fields'?(q('#slack-list-field-choice')?.value!=='C_NUMBER'||q('#slack-list-field-visible')?.value!=='hidden'||fieldsReturns!==1):panelKind!=='name'||q('#slack-list-rename-name')?.value!=='Focused delivery'))return fail('confirm-draft');
      if(['#slack-list-refinement-save','#slack-list-refinement-save-new'].includes(selector)&&!applied)return fail('apply-required');
      const step=prepare(selector);if(!step)return fail('click-custody');
      if(selector==='#slack-list-fields'){pendingPanel='fields';opener=step.e;}
      if(selector==='#slack-list-refinement-save'){pendingPanel='save';opener=step.e;}
      if(selector==='#slack-list-refinement-save-new'){pendingPanel='name';opener=step.e;}
      step.e.click();events.push({kind:'click',selector,owner:step.before,trusted:false});return {ok:true,selector,owner:step.before,revealed:step.revealed,hitPoints:9,trusted:false};
    },
    async dragDoneBeforeProgress(){if(!actionReady()||panel||!rowsMatch(projection(),false,false)||q('#slack-list-refinement-save'))return fail('drag-initial');
      const source=q('.slack-list-lane[data-group-key="OPT_DONE"] .slack-list-lane-title'),target=q('.slack-list-lane[data-group-key="OPT_PROGRESS"] .slack-list-lane-title'),active=document.activeElement,before=stamp();
      const current=()=>inside(source,owner?.node)&&inside(target,owner?.node)&&q('.slack-list-lane[data-group-key="OPT_DONE"] .slack-list-lane-title')===source&&q('.slack-list-lane[data-group-key="OPT_PROGRESS"] .slack-list-lane-title')===target;
      if(!current()||!live(source)||!live(target)||source.getAttribute('draggable')!=='true'||typeof DataTransfer!=='function'||typeof DragEvent!=='function')return fail('drag-nodes');
      const frame=()=>new Promise(resolve=>requestAnimationFrame(resolve)),dataTransfer=new DataTransfer(),sent=[];
      const send=(e,type)=>{const r=e.getBoundingClientRect(),event=new DragEvent(type,{bubbles:true,cancelable:true,dataTransfer,clientX:r.left+r.width/2,clientY:r.top+r.height/2});e.dispatchEvent(event);sent.push({type,trusted:event.isTrusted,defaultPrevented:event.defaultPrevented,rect:rect(e)});};
      if(!reveal(source,'drag-source').ok||!current()||!hit(source)||!owned()||document.activeElement!==active)return fail('drag-source-hit');
      dragging=true;
      try {send(source,'dragstart');await frame();if(!owned()||!current()||!same(before,stamp())||!live(source)||!live(target)||document.activeElement!==active)return fail('drag-start-owner');
        if(!reveal(target,'drag-target').ok||!current()||!hit(target)||!owned())return fail('drag-target-hit');send(target,'dragover');await frame();
        if(!owned()||!current()||!same(before,stamp())||!live(source)||!live(target)||document.activeElement!==active||!hit(target))return fail('drag-drop-owner');send(target,'drop');await frame();
        const result={ok:owned()&&current()&&same(before,stamp())&&document.activeElement===active&&sent.every(e=>e.trusted===false),kind:'synthetic-header-drag',owner:before,events:sent,sourceKey:'OPT_DONE',targetKey:'OPT_PROGRESS'};events.push(result);return result;
      } finally {if(live(source)&&inside(source,owner?.node)&&owned())send(source,'dragend');dragging=false;}
    },
    returned(selector){if(!['#slack-list-fields','#slack-list-refinement-save','#slack-list-refinement-save-new','#slack-list-view-select'].includes(selector))return fail('return-selector');if(!owned()||pendingPanel||!panel||panel.isConnected||q('#slack-list-rename')||!topDialog(owner.node)||!action||action.serial!==serial||!['#slack-list-rename-close','#slack-list-rename-confirm'].includes(action.selector))return fail('return-owner');
      const target=q(selector),expected=panelKind==='fields'?opener:opener?.isConnected&&!opener.disabled?opener:owner.picker;
      if(target!==expected||target!==document.activeElement||!live(target)||target.disabled||!focusRing(target).ok||!hit(target)||!retained().ok)return fail('return-focus');
      if(panelKind==='fields'){if(action.selector==='#slack-list-rename-close')fieldsReturns++;else applied=true;}
      const oldSerial=serial;panel=null;serial=null;panelKind=null;busy=null;return {ok:true,selector,oldSerial,sameOpener:target===opener,oldDisconnected:true,focus:focusRing(target),find:retained()};
    },
    acceptSelection(proof){const s=serialValue(proof?.serial),visit=serialValue(proof?.visit),selection=serialValue(proof?.selection),r=q('.slack-list-dialog'),picker=q('#slack-list-view-select');
      if(!owner||selectionProof||pendingPanel||!panel||panel.isConnected||q('#slack-list-rename')||!live(r)||r!==owner.node||picker!==owner.picker||visit!==owner.visit||r.dataset.dialogGeneration!==visit||s!==serial||proof.id!==created||selection===null||selection===owner.selection||r.dataset.viewSelection!==selection||picker.value!==created||!topDialog(r)||document.activeElement!==picker||!focusRing(picker).ok||!action||action.serial!==serial||!['#slack-list-rename-confirm','#slack-list-new-view-open'].includes(action.selector))return fail('selection-proof');
      const currentStatus=q('#slack-list-refinement-save-status');if(action.selector==='#slack-list-rename-confirm'&&(!live(currentStatus)||currentStatus.dataset.refinementSerial!==s))return fail('selection-status-owner');
      selectionProof={visit,selection,id:created,serial:s,priorSelection:owner.selection};statusNode=currentStatus;owner.selection=selection;owner.view=created;panel=null;serial=null;panelKind=null;busy=null;return {ok:true,proof:{...selectionProof},oldPanelDisconnected:true,statusSerial:currentStatus?.dataset.refinementSerial||null,find:retained(),picker:rect(picker)};
    },
    geometry(stage,mode){if(!['existing','new','lost'].includes(mode)||!['baseline','local','fields','dirty','name','saving','pending','observed','board'].includes(stage)||stage==='name'&&mode==='existing')return fail('stage');
      if(!custody()||!retained().ok)return fail('geometry-custody');const active=document.activeElement,phase=panel?.dataset.renameStage||null,rows=projection(),pendingNew=stage==='pending'&&mode==='new';
      const expectedPhase=stage==='fields'||stage==='name'?'Editing':stage==='saving'?'Saving':stage==='pending'?mode==='lost'?'Uncertain':mode==='existing'?'Checking':null:stage==='observed'?mode==='lost'?'Available':mode==='existing'?'Observed':null:null;
      const moved=stage!=='baseline'&&!(stage==='pending'&&mode==='existing'),hidden=['dirty','name','saving','pending','observed','board'].includes(stage);
      const rowProof=pendingNew?rows.length===0&&!q('.slack-list-board')&&!q('.slack-list-grid'):rowsMatch(rows,moved,hidden);
      const buttons=panel?[...panel.querySelectorAll('button')]:q('#slack-list-refinement-save-status')?[...q('#slack-list-refinement-save-status').querySelectorAll('button')]:[];
      const controls=buttons.map(e=>({id:e.id,text:e.textContent,disabled:e.disabled,box:box(e,e===active),hit:e.disabled?false:hit(e)}));
      const expectedButtons=panel?['fields','name'].includes(stage)?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-confirm']:stage==='saving'||stage==='pending'&&mode==='existing'?['slack-list-rename-leave']:stage==='pending'&&mode==='lost'?['slack-list-rename-leave','slack-list-rename-check']:stage==='observed'&&mode==='lost'?['slack-list-rename-leave','slack-list-rename-close','slack-list-rename-check','slack-list-new-view-open']:['slack-list-rename-leave','slack-list-rename-close']:mode==='new'&&stage==='observed'?['slack-list-refinement-status-close']:[];
      const exactButtons=same(controls.map(c=>c.id),expectedButtons);
      const labels={'slack-list-rename-leave':'Close list','slack-list-rename-close':['fields','name'].includes(stage)?'Cancel':mode==='lost'?'Close review':'Done','slack-list-rename-confirm':stage==='fields'?'Apply':'Save for everyone','slack-list-rename-check':'Check current view','slack-list-new-view-open':'Open view','slack-list-refinement-status-close':'Dismiss'};
      const controlCopy=controls.every(c=>c.text===labels[c.id]);
      const textIds=panel?['slack-list-rename-title','slack-list-rename-message',...(panelKind==='fields'?['slack-list-field-visibility-current']:['slack-list-refinement-preference'])]:q('#slack-list-refinement-save-status')?['slack-list-refinement-save-status']:[];
      const texts=textIds.map(id=>{const e=q('#'+id);return {id,text:e?.textContent||null,box:box(e)};});
      const selects=['slack-list-field-choice','slack-list-field-visible'].map(id=>{const e=q('#'+id);return e?{id,value:e.value,disabled:e.disabled,options:[...e.options].map(o=>[o.value,o.textContent]),box:box(e),hit:!e.disabled&&hit(e)}:null;}).filter(Boolean);
      const fieldProof=stage!=='fields'||selects.length===2&&selects[0].value==='C_NUMBER'&&selects[1].value==='hidden'&&selects.every(s=>!s.disabled&&s.box.ok&&s.hit)&&q('#slack-list-rename-confirm')?.textContent==='Apply';
      const name=q('#slack-list-rename-name'),nameProof=stage!=='name'||name?.value==='Focused delivery'&&!name.disabled&&box(name).ok&&hit(name);
      const saves=['slack-list-refinement-save','slack-list-refinement-save-new','slack-list-refinement-reset'].map(id=>{const e=q('#'+id);return {id,present:!!e,disabled:e?.disabled??null,text:e?.textContent||null};});
      const saveProof=['baseline','local','board'].includes(stage)||stage==='observed'&&mode!=='lost'?saves.every(s=>!s.present):['dirty','name','saving'].includes(stage)||stage==='observed'&&mode==='lost'?saves.every(s=>s.present):true;
      const options=[...owner.picker.options].map(o=>[o.value,o.textContent]);const expectedOptions=selectionProof||stage==='observed'&&mode==='lost'?[...views,[created,pendingNew?'Loading saved view…':'Focused delivery']]:views;
      const focus=focusProof(stage,mode),partial=all('.slack-list-warning').filter(e=>e.textContent==='Only part of this list is available. More items may exist.'),notices=all('.slack-list-notices [role="status"]').map(e=>e.textContent);
      const surfaceControls=panel?[]:['slack-list-view-select','slack-list-fields',...saves.filter(s=>s.present).map(s=>s.id)].map(id=>{const e=q('#'+id);return {id,disabled:e?.disabled??null,box:box(e),hit:!e?.disabled&&hit(e)};});
      const surfaceGeometry=!panel&&!pendingNew?['.slack-list-lane[data-group-key="OPT_PROGRESS"] .slack-list-lane-title','.slack-list-lane[data-group-key="OPT_DONE"] .slack-list-lane-title','.slack-list-lane[data-group-key="OPT_PROGRESS"] .slack-list-card'].map(selector=>({selector,box:box(q(selector))})):[];
      const pendingProof=!pendingNew||!find()&&!findNode.isConnected&&notices.includes('Loading the saved view metadata…')&&!!q('#slack-list-refinement-save-status');
      const bounded=box(owner.node).ok&&(!panel||box(panel).ok),sameActiveElement=active===document.activeElement;
      const ok=phase===expectedPhase&&rowProof&&fieldProof&&nameProof&&saveProof&&pendingProof&&exactButtons&&controlCopy&&same(options,expectedOptions)&&focus.ok&&focus.ring.ok&&bounded&&sameActiveElement&&partial.length===1&&controls.every(c=>!c.disabled&&c.box.ok&&c.hit)&&surfaceControls.every(c=>c.box.ok&&(c.disabled||c.hit))&&surfaceGeometry.every(g=>g.box.ok)&&texts.every(t=>t.box.ok)&&custody()&&retained().ok;
      if(ok&&panel&&!['fields','name'].includes(stage))busy=active;
      return {ok,stage,mode,owner:stamp(),phase,expectedPhase,rows,rowProof,controls,exactButtons,controlCopy,surfaceControls,surfaceGeometry,texts,selects,fieldProof,name:name?{value:name.value,disabled:name.disabled,rect:rect(name)}:null,nameProof,saves,saveProof,options,notices,pendingProof,focus,bounded,sameActiveElement,partial:partial.map(e=>e.textContent),find:retained(),viewport:[innerWidth,innerHeight],panel:rect(panel),root:rect(owner.node)};
    },
    history(){return {ok:true,owner:stamp(),fieldsReturns,applied,selectionProof:selectionProof?{...selectionProof}:null,events:events.slice()};}
  };
  window.__listRefinementSave=api;return {ok:true};
})()
