import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import test from 'node:test';

const source = fs.readFileSync(path.join(process.argv[2] || '.', 'src/desktop/slack_lists/check/view_order.js'), 'utf8');
const original=[['V_ORDER_FIRST','Planning'],['V_ORDER_SELECTED','Delivery'],['V_ORDER_LAST','Archive']];
const desired=[original[1],original[0],original[2]];
const messages={Editing:'Review this one-step move, then Save for everyone. Cancel leaves the shared view order unchanged.',Saving:'Saving shared view order...',
  Uncertain:'The shared view order is not confirmed. Check current views explicitly. This request will not be sent again.',
  Conflict:'The returned views differ from this move. Review the current order. The earlier request may still arrive; closing does not repeat or undo it.',
  Observed:'The current shared view order matches this move. This does not prove which request changed it.'};
const currentFields='Current fields: Item (text), Done (checkbox), Estimate (number), Extended field (future_field), Notes (rich_text), Status (select), Assignee (user), Channel (channel), Reference (link), Budget (currency), Teams (select); views: Planning, Delivery, Archive';
function harness() {
  const byId = new Map(), state = {serial: 10, visit: 1, selection: 8, clicks: 0, changes: 0, scrolls: 0, focusCalls: 0, reusePanel: false};
  let document, root, panel = null, find, opener, picker, rows, headers, partial, buttons = [], pendingRevealHook = null;
  class Element {
    constructor(id, parent = null, bounds = [20,20,100,24]) {
      this.id = id; this.parentElement = parent; this.children = []; this.isConnected = true; this.disabled = false;
      this.dataset = {}; this.textContent = ''; this.bounds = bounds; this.style = {}; this.hidden = false;
      if (parent) parent.children.push(this); if (id) byId.set(id, this);
    }
    get previousElementSibling(){const siblings=this.parentElement?.children||[];return siblings[siblings.indexOf(this)-1]||null;}
    get nextElementSibling(){const siblings=this.parentElement?.children||[];return siblings[siblings.indexOf(this)+1]||null;}
    closest(selector) {
      if (selector === '[hidden],[inert]') { for (let e = this; e; e = e.parentElement) if (e.hidden || e.inert) return e; }
      return null;
    }
    contains(e) { for (; e; e = e.parentElement) if (e === this) return true; return false; }
    getBoundingClientRect() { const [x,y,width,height] = this.bounds; return {x,y,width,height,left:x,top:y,right:x+width,bottom:y+height}; }
    focus() { state.focusCalls++; document.activeElement = this; }
    scrollIntoView() { state.scrolls++; pendingRevealHook?.(this); }
    getAttribute(name) { return this[name] ?? null; }
    querySelector(selector) {
      if (selector.startsWith('#')) { const e = byId.get(selector.slice(1)); return e && this.contains(e) ? e : null; }
      if (selector === 'input,select,textarea') return this.children.find(e=>e instanceof Input||e instanceof Select)||null;
      if (selector === '.slack-list-cell-text') return this.children.find(e => e.cellText) || null;
      if (selector === 'input[type="checkbox"]') return this.children.find(e => e.checkbox) || null;
      return null;
    }
    querySelectorAll(selector) {
      if (selector === 'button') return buttons.filter(e => this.contains(e) && e.isConnected);
      if (selector === 'li[data-view-id]') return this.children.filter(e=>e.dataset.viewId);
      if (selector === '[data-column-id]') return this.children.filter(e => e.dataset.columnId);
      return [];
    }
    click() { state.clicks++; this.onclick?.(); }
    dispatchEvent(event) { if (event.type === 'change') { state.changes++; this.onchange?.(); } }
  }
  class Input extends Element {
    get value() { return this._value || ''; }
    set value(v) { this._value = v; this.selectionStart = this.selectionEnd = v.length; this.selectionDirection = 'none'; }
  }
  class Select extends Element {
    get value() { return this._value || ''; }
    set value(v) { this._value = v; }
    get selectedOptions() { return this.options.filter(o => o.value === this.value); }
  }
  function disconnect(e) {
    e.isConnected = false; if (e.id && byId.get(e.id) === e) byId.delete(e.id);
    e.children.forEach(disconnect);
  }
  function newRoot() {
    root = new Element('root', null, [4,4,652,682]); root.dataset = {dialogGeneration:String(state.visit),viewSelection:String(state.selection)};
    find = new Input('find', root, [20,65,180,24]); find.value = 'Launch item 0';
    opener=new Element('slack-list-view-move-up',root,[220,20,110,24]);opener.dataset={reorderView:'V_ORDER_SELECTED',reorderDirection:'up',reorderSelection:String(state.selection)};opener.textContent='Move view up';opener.onclick=()=>mount();
    const down=new Element('slack-list-view-move-down',root,[220,50,110,24]);down.dataset={reorderView:'V_ORDER_SELECTED',reorderDirection:'down',reorderSelection:String(state.selection)};down.textContent='Move view down';
    picker=new Select('slack-list-view-select',root,[350,20,140,24]);picker.options=[['','All loaded items'],...original].map(([value,textContent])=>({value,textContent}));picker.value='V_ORDER_SELECTED';
    new Element('slack-list-close', root, [540,20,90,24]).textContent = 'Close';
    headers = ['C_NAME','C_NUMBER','C_CHECK'].map((id,i) => { const e = new Element('', root, [20+i*150,105,140,24]); e.dataset.columnId = id; return e; });
    rows = Array.from({length:9},(_,i) => {
      const row = new Element('',root); row.dataset.rowId = `R_LIST_${String(i).padStart(3,'0')}`;
      for (const [j,id] of ['C_NAME','C_NUMBER','C_CHECK'].entries()) {
        const cell = new Element('',row); cell.dataset.columnId = id;
        if (j === 2) { const c = new Input('',cell); c.checkbox = true; c.checked = i%2===1; }
        else { const text = new Element('',cell); text.cellText = true; text.textContent = j===0?`Launch item ${String(i+1).padStart(2,'0')}`:String(i); }
      } return row;
    });
    partial = new Element('partial',root,[20,95,500,20]); partial.textContent = 'Only part of this List is loaded.';
  }
  function orderList(id,values,y){
    const label=new Element('',panel,[30,y,580,18]);label.textContent={before:'Before',after:'After saving',current:'Current custom views'}[id];
    const list=new Element('slack-list-view-order-'+id,panel,[30,y+22,580,60]);
    for(const [i,[view,name]] of values.entries()){const li=new Element('',list,[40,y+22+i*20,560,20]);li.dataset.viewId=view;li.textContent=name;}
    return list;
  }
  function mount(serial = ++state.serial) {
    if(panel)disconnect(panel);
    panel=new Element('slack-list-rename',root,[14,14,630,660]);panel.dataset={renameSerial:String(serial),renameStage:'Editing',renameView:'V_ORDER_SELECTED',reorderDirection:'up'};
    new Element('slack-list-rename-title',panel,[30,30,580,24]).textContent='Move view';
    new Element('slack-list-view-order-move',panel,[30,58,580,20]).textContent='Move Delivery up by one place.';
    new Element('notice',panel,[30,80,580,30]).textContent='This changes the shared custom view order for everyone with access. Built-in views stay in their own section.';
    orderList('before',original,120);orderList('after',desired,215);orderList('current',original,310);
    new Element('slack-list-rename-message',panel,[30,405,580,54]);
    setStage('Editing');byId.get('slack-list-rename-close').focus();opener.disabled=true;byId.get('slack-list-view-move-down').disabled=true;
  }
  function setStage(stage, retired = false) {
    panel.dataset.renameStage=stage;
    byId.get('slack-list-rename-message').textContent = retired ? 'Could not confirm the current schema. The earlier request will not be sent again.' : messages[stage] || 'Checking current fields...';
    const oldButtons = buttons.filter(e => e.isConnected); buttons = [];
    const priorCurrent = byId.get('slack-list-rename-current'); if (priorCurrent) disconnect(priorCurrent);
    if (['Conflict','Observed'].includes(stage)) new Element('slack-list-rename-current',panel,[30,475,580,75]).textContent = currentFields;
    const specs = [['slack-list-rename-leave','Close list']];
    if (['Editing','Conflict','Observed'].includes(stage)) specs.push(['slack-list-rename-close',stage==='Editing'?'Cancel':stage==='Conflict'?'Close review':'Done']);
    if (stage==='Editing') specs.push(['slack-list-rename-confirm','Save for everyone']);
    if (['Uncertain','Conflict'].includes(stage)) specs.push(['slack-list-rename-check','Check current views']);
    for (const [i,[id,label]] of specs.entries()) {
      const e = oldButtons.find(e => e.id === id) || new Element(id,panel,[30+i*195,610,180,28]); e.bounds=[30+i*195,610,180,28]; e.textContent = label; buttons.push(e);
      e.onclick = () => {
        if (id==='slack-list-rename-close') { const observed=panel.dataset.renameStage==='Observed';disconnect(panel);panel=null;buttons=[];opener.disabled=observed;byId.get('slack-list-view-move-down').disabled=false;(observed?byId.get('slack-list-close'):opener).focus(); }
        if (id==='slack-list-rename-confirm') { setStage('Saving'); byId.get('slack-list-rename-leave').focus(); }
        if (id==='slack-list-rename-check') { setStage('Checking'); byId.get('slack-list-rename-leave').focus(); }
      };
    }
    oldButtons.filter(e => !buttons.includes(e)).forEach(disconnect);
    if(stage==='Observed'){const e=byId.get('slack-list-view-order-current');e.children.forEach((n,i)=>{n.dataset.viewId=desired[i][0];n.textContent=desired[i][1];});picker.options=[['','All loaded items'],...desired].map(([value,textContent])=>({value,textContent}));}
  }
  newRoot();
  document = {activeElement:null,
    querySelector(selector) {
      if (selector==='.slack-list-dialog') return root.isConnected?root:null;
      if (selector==='.slack-list-find input') return find;
      if (selector.startsWith('#')) return byId.get(selector.slice(1)) || null;
      return null;
    },
    querySelectorAll(selector) {
      if (selector==='[role="dialog"]') return [root,...panel?[panel]:[]];
      if (selector==='.slack-list-grid tbody tr[data-row-id]') return rows;
      if (selector==='.slack-list-grid thead th[data-column-id]') return headers;
      if (selector==='.slack-list-warning') return [partial];
      return [];
    },
    getElementById(id) { return byId.get(id)||null; },
    elementFromPoint(x,y) {
      const candidates = [...byId.values()].filter(e => e.isConnected && !e.hidden && !e.inert && (!panel || panel.contains(e)));
      return candidates.reverse().find(e=>{const b=e.getBoundingClientRect();return x>=b.left&&x<=b.right&&y>=b.top&&y<=b.bottom;}) || root;
    },
  };
  const context = vm.createContext({document,window:{},innerWidth:668,innerHeight:700,HTMLInputElement:Input,HTMLSelectElement:Select,
    Event:class{constructor(type){this.type=type;}},getComputedStyle:e=>({display:'block',visibility:'visible',overflowX:'visible',overflowY:'visible',outlineWidth:'2px',outlineOffset:'0px',outlineStyle:'solid',...e.style})});
  assert.equal(vm.runInContext(source,context).ok,true); const api = context.window.__listViewOrder;
  return {api,state,document,context,byId,root:()=>root,panel:()=>panel,find:()=>find,opener:()=>opener,rows:()=>rows,
    setStage, mount, disconnect, revealHook(fn){pendingRevealHook=fn;},
    open(){assert.equal(api.ready().ok,true);assert.equal(api.query().ok,true);assert.equal(api.bindFind().ok,true);assert.equal(api.rememberMove().ok,true);assert.equal(api.click('#slack-list-view-move-up').ok,true);assert.equal(api.draft().ok,true);},
    model(){const d=api.draft();return {serial:Number(d.serial),visit:Number(d.visit),selection:Number(d.selection),view:'V_ORDER_SELECTED',direction:'up',before:original,after:desired,current:d.stage==='Observed'?desired:original,stage:d.stage,canConfirm:d.canConfirm};},
    reopen(){const oldSerial=panel.dataset.renameSerial;disconnect(root);state.visit++;state.selection++;newRoot();mount(oldSerial);setStage('Checking');byId.get('slack-list-rename-leave').focus();},

  };
}

test('readiness is passive, exact and required before initial query',()=>{
  const h=harness();const before=[h.state.scrolls,h.state.focusCalls,h.state.clicks];
  assert.equal(h.api.ready().ok,true);assert.deepEqual([h.state.scrolls,h.state.focusCalls,h.state.clicks],before);
  h.byId.get('slack-list-view-select').value='wrong';assert.equal(h.api.ready().ok,false);assert.equal(h.api.query().ok,false);
  assert.deepEqual([h.state.scrolls,h.state.focusCalls,h.state.clicks],before);
});
test('initial query cannot claim a ready layout replaced during reveal',()=>{
  const h=harness();h.revealHook(()=>h.root().dataset.viewSelection='new');assert.equal(h.api.query().ok,false);assert.equal(h.state.focusCalls,0);
});
test('actual fixed move opens with exact host draft and Cancel focus',()=>{
  const h=harness();h.open();assert.equal(h.api.sameDraft(h.model()).ok,true);
  const g=h.api.geometry('edit');assert.equal(g.ok,true,JSON.stringify(g));assert.equal(g.focus.id,'slack-list-rename-close');
});
test('full host draft rejects every independently owned model field',()=>{
  const h=harness();h.open();const m=h.model();
  for(const [key,value] of [['serial',99],['visit',99],['selection',99],['view','wrong'],['direction','down'],['before',desired],['after',original],['current',null],['stage','Saving'],['canConfirm',false]])assert.equal(h.api.sameDraft({...m,[key]:value}).ok,false,key);
  assert.equal(h.api.sameDraft({...m,serial:9007199254740992}).ok,false);
  assert.equal(h.api.sameDraft({...m,serial:'010'}).ok,false);
});
test('wrong owner, serial or Find value refuses before any reveal or focus',()=>{
  for(const change of [h=>h.root().dataset.dialogGeneration='99',h=>h.root().dataset.viewSelection='99',h=>h.panel().dataset.renameSerial='99',h=>h.disconnect(h.root()),h=>h.find().value='wrong']){
    const h=harness();h.open();change(h);const before=[h.state.scrolls,h.state.focusCalls,h.state.clicks];assert.equal(h.api.click('#slack-list-rename-confirm').ok,false);assert.deepEqual([h.state.scrolls,h.state.focusCalls,h.state.clicks],before);
  }
});
test('wrong actual focus cannot be repaired by click or geometry',()=>{
  const h=harness();h.open();h.document.activeElement=h.opener();const before=[h.state.scrolls,h.state.focusCalls,h.state.clicks];
  assert.equal(h.api.click('#slack-list-rename-confirm').ok,false);assert.equal(h.api.geometry('edit').ok,false);assert.deepEqual([h.state.scrolls,h.state.focusCalls,h.state.clicks],before);
});
test('owner, active node, Find range or phase changes during reveal refuse action',()=>{
  for(const change of [h=>h.root().dataset.viewSelection='new',h=>h.document.activeElement=h.opener(),h=>h.find().selectionStart=0,h=>h.panel().dataset.renameStage='Uncertain']){
    const h=harness();h.open();const before=h.state.clicks;h.revealHook(()=>change(h));assert.equal(h.api.click('#slack-list-rename-confirm').ok,false);assert.equal(h.state.clicks,before);
  }
});
test('immutable target and exact complete order labels cannot drift',()=>{
  for(const change of [h=>h.panel().dataset.reorderDirection='down',h=>h.panel().dataset.renameView='other',h=>h.byId.get('slack-list-view-order-before').children[0].textContent='Changed',h=>h.byId.get('slack-list-view-order-after').children.reverse(),h=>h.byId.get('slack-list-view-order-current').children.pop()]){
    const h=harness();h.open();change(h);assert.equal(h.api.geometry('edit').ok,false);
  }
});
test('hidden name editor is rejected rather than silently ignored',()=>{
  const h=harness();h.open();const name=Object.assign(Object.create(Object.getPrototypeOf(h.find())),h.find(),{id:'slack-list-rename-name',parentElement:h.panel(),hidden:true});
  h.panel().children.push(name);h.byId.set(name.id,name);assert.equal(h.api.draft().ok,false);
});
test('Cancel invokes no Save and restores the exact original opener',()=>{
  const h=harness();h.open();assert.equal(h.api.click('#slack-list-rename-close').ok,true);assert.equal(h.api.returnedMove().ok,true);
  assert.equal(h.api.rememberMove().ok,true);assert.equal(h.api.click('#slack-list-view-move-up').ok,true);assert.equal(h.api.draft().ok,true);
});
test('Saving requires actual enabled Close list fallback and immutable review',()=>{
  const h=harness();h.open();assert.equal(h.api.click('#slack-list-rename-confirm').ok,true);const g=h.api.geometry('saving');assert.equal(g.ok,true,JSON.stringify(g));
  h.document.activeElement=h.opener();assert.equal(h.api.geometry('saving').ok,false);
});
test('all seven stages retain exact model, focus, new-visit binding and boundary return',()=>{
  const h=harness();h.open();const receipt=stage=>{const g=h.api.geometry(stage);assert.equal(g.ok,true,`${stage}: ${JSON.stringify(g)}`);};
  receipt('edit');assert.equal(h.api.click('#slack-list-rename-confirm').ok,true);receipt('saving');h.setStage('Uncertain');receipt('uncertain');
  assert.equal(h.api.click('#slack-list-rename-check').ok,true);h.setStage('Conflict');receipt('conflict');assert.equal(h.api.click('#slack-list-rename-check').ok,true);
  h.reopen();assert.equal(h.api.ready(true).ok,true);const rebound=h.api.bindFind();assert.equal(rebound.ok,true);assert.equal(rebound.findContinuityClaimed,false);assert.equal(h.api.rememberMove().ok,true);assert.equal(h.api.rememberBusy().ok,true);assert.equal(h.api.sameDraft(h.model()).ok,true);
  h.setStage('Uncertain',true);receipt('retired');assert.equal(h.api.click('#slack-list-rename-check').ok,true);h.setStage('Observed');assert.equal(h.api.sameDraft(h.model()).ok,true);receipt('observed');
  assert.equal(h.api.click('#slack-list-rename-close').ok,true);assert.equal(h.api.returnedMove().ok,true);assert.equal(h.api.returnedMove().fallback,true);receipt('table');
});
test('same live owner cannot rebind away a Find custody failure',()=>{
  const h=harness();h.open();h.find().selectionStart=0;assert.equal(h.api.bindFind().ok,false);assert.equal(h.api.findRetained().ok,false);
});
test('replacement readiness requires exact Checking modal without actions',()=>{
  const h=harness();h.open();h.reopen();const before=[h.state.scrolls,h.state.focusCalls,h.state.clicks];assert.equal(h.api.ready().ok,false);assert.equal(h.api.ready(true).ok,true);
  h.panel().dataset.renameStage='Uncertain';assert.equal(h.api.ready(true).ok,false);assert.deepEqual([h.state.scrolls,h.state.focusCalls,h.state.clicks],before);
});
test('full focus outline clipping is strict even for fractional pixels',()=>{
  for(const mode of ['viewport','ancestor','none']){const h=harness();h.open();const e=h.byId.get('slack-list-rename-close');if(mode==='viewport')e.bounds[0]=1.99999;else if(mode==='ancestor'){h.panel().style.overflowX='hidden';e.bounds[0]=15.99999;}else e.style.outlineStyle='none';assert.equal(h.api.geometry('edit').ok,false);}
});
test('review, heading, notice, message and footer must fit simultaneously without scrolling',()=>{
  for(const id of ['slack-list-view-order-before','slack-list-view-order-after','slack-list-view-order-current','notice','slack-list-rename-message','slack-list-rename-confirm']){
    const h=harness();h.open();h.byId.get(id).bounds[1]=690;const before=[h.state.scrolls,h.state.focusCalls];assert.equal(h.api.geometry('edit').ok,false);assert.deepEqual([h.state.scrolls,h.state.focusCalls],before);
  }
  const h=harness();h.open();h.byId.get('slack-list-view-order-before').previousElementSibling.bounds[1]=690;assert.equal(h.api.geometry('edit').ok,false);
});
test('wrong sharing notice, action copy, status, row order or selected label fails',()=>{
  for(const change of [h=>h.byId.get('notice').textContent='wrong',h=>h.byId.get('slack-list-rename-confirm').textContent='Rename view',h=>h.byId.get('slack-list-rename-message').textContent='wrong',h=>h.rows()[0].dataset.rowId='wrong',h=>h.byId.get('slack-list-view-select').options[2].textContent='wrong']){
    const h=harness();h.open();change(h);assert.equal(h.api.geometry('edit').ok,false);
  }
});
test('new top dialog refuses action before reveal',()=>{
  const h=harness();h.open();const query=h.document.querySelectorAll;h.document.querySelectorAll=s=>s==='[role="dialog"]'?[...query(s),{isConnected:true,closest:()=>null,getBoundingClientRect:()=>({width:10})}]:query(s);const before=h.state.scrolls;assert.equal(h.api.click('#slack-list-rename-confirm').ok,false);assert.equal(h.state.scrolls,before);
});
test('replaced fallback cannot impersonate retained action focus',()=>{
  const h=harness();h.open();h.api.click('#slack-list-rename-confirm');assert.equal(h.api.geometry('saving').ok,true);h.setStage('Uncertain');const old=h.byId.get('slack-list-rename-leave'),replacement=Object.assign(Object.create(Object.getPrototypeOf(old)),old);h.byId.set(old.id,replacement);h.document.activeElement=replacement;assert.equal(h.api.geometry('uncertain').ok,false);
});
test('replaced opener or List Close cannot inherit exact return ownership',()=>{
  for(const id of ['slack-list-view-move-up','slack-list-close']){
    const h=harness();h.open();if(id==='slack-list-close'){h.api.click('#slack-list-rename-confirm');h.api.geometry('saving');h.setStage('Observed');}
    h.api.click('#slack-list-rename-close');const old=h.byId.get(id),replacement=Object.assign(Object.create(Object.getPrototypeOf(old)),old);h.byId.set(id,replacement);h.document.activeElement=replacement;assert.equal(h.api.returnedMove().ok,false);
  }
});
test('immutable order mutation during reveal refuses Save without a write',()=>{
  const h=harness();h.open();const before=h.state.clicks;h.revealHook(()=>h.byId.get('slack-list-view-order-after').children.reverse());assert.equal(h.api.click('#slack-list-rename-confirm').ok,false);assert.equal(h.state.clicks,before);
});
test('a focus side effect that changes owner cannot dispatch Save',()=>{
  const h=harness();h.open();const e=h.byId.get('slack-list-rename-confirm'),focus=e.focus.bind(e);e.focus=()=>{focus();h.root().dataset.viewSelection='new';};const before=h.state.clicks;assert.equal(h.api.click('#slack-list-rename-confirm').ok,false);assert.equal(h.state.clicks,before);
});
test('rebind cannot adopt a different receipt serial or still-live old Find',()=>{
  for(const kind of ['serial','find']){const h=harness();h.open();const old=h.find();h.reopen();if(kind==='serial')h.panel().dataset.renameSerial='999';else old.isConnected=true;assert.equal(h.api.bindFind().ok,false);}
});
test('remember cannot replace exact return nodes after capture',()=>{
  for(const id of ['slack-list-view-move-up','slack-list-close']){const h=harness();h.open();const old=h.byId.get(id),replacement=Object.assign(Object.create(Object.getPrototypeOf(old)),old);h.byId.set(id,replacement);assert.equal(h.api.rememberMove().ok,false);if(id==='slack-list-close')assert.equal(h.api.rememberListClose().ok,false);}
});
test('current-field copy and clipping are checked on the same conflict capture',()=>{
  for(const mode of ['copy','clip']){const h=harness();h.open();h.api.click('#slack-list-rename-confirm');h.api.geometry('saving');h.setStage('Uncertain');h.api.click('#slack-list-rename-check');h.setStage('Conflict');const e=h.byId.get('slack-list-rename-current');if(mode==='copy')e.textContent='wrong';else e.bounds[1]=690;const before=[h.state.scrolls,h.state.focusCalls];assert.equal(h.api.geometry('conflict').ok,false);assert.deepEqual([h.state.scrolls,h.state.focusCalls],before);}
});
