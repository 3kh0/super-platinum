import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import test from 'node:test';

const source = fs.readFileSync(path.join(process.argv[2] || '.', 'src/desktop/slack_lists/check/field_delete.js'), 'utf8');
const fields = [['C_CHECK','Done'],['C_NUMBER','Estimate'],['C_RICH','Notes'],['C_SELECT','Status'],['C_USER','Assignee'],
  ['C_CHANNEL','Channel'],['C_LINK','Reference'],['C_CURRENCY','Budget'],['C_MULTI','Teams'],['C_HIDDEN','Hidden flag'],['C_ABSENT','Absent notes']];
const warning = 'Permanently delete the selected field from every view? This app does not offer Undo for field deletion.';
const messages = {
  Editing: warning, Saving: 'Deleting field...',
  Uncertain: 'Field deletion is not confirmed. Check current fields explicitly. This request will not be sent again.',
  Conflict: 'The returned schema differs from the requested deletion. Review the current fields. The earlier request may still arrive; closing does not repeat or undo it.',
  Observed: 'The field is absent from the current schema and every view. This does not prove which request removed it.',
};
const currentFields = {Conflict: 'Current fields: Item (text), Done (checkbox), Estimate (number), Extended field (future_field), Notes (rich_text), Status (select), Assignee (user), Channel (channel), Reference (link), Budget (currency), Teams (select), Hidden flag (checkbox); views: Deletion table, Other board, Target absent; 1 more fields', Observed: 'Current fields: Item (text), Done (checkbox), Estimate (number), Extended field (future_field), Notes (rich_text), Status (select), Assignee (user), Channel (channel), Reference (link), Budget (currency), Teams (select), Absent notes (text); views: Deletion table, Other board, Target absent'};
function harness() {
  const byId = new Map(), state = {serial: 10, visit: 1, selection: 8, clicks: 0, changes: 0, scrolls: 0, focusCalls: 0, reusePanel: false};
  let document, root, panel = null, find, opener, picker, rows, headers, partial, buttons = [], pendingRevealHook = null;
  class Element {
    constructor(id, parent = null, bounds = [20,20,100,24]) {
      this.id = id; this.parentElement = parent; this.children = []; this.isConnected = true; this.disabled = false;
      this.dataset = {}; this.textContent = ''; this.bounds = bounds; this.style = {}; this.hidden = false;
      if (parent) parent.children.push(this); if (id) byId.set(id, this);
    }
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
      if (selector === '.slack-list-cell-text') return this.children.find(e => e.cellText) || null;
      if (selector === 'input[type="checkbox"]') return this.children.find(e => e.checkbox) || null;
      return null;
    }
    querySelectorAll(selector) {
      if (selector === 'button') return buttons.filter(e => this.contains(e) && e.isConnected);
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
    root = new Element('root', null, [4,4,652,372]); root.dataset = {dialogGeneration:String(state.visit),viewSelection:String(state.selection)};
    find = new Input('find', root, [20,65,180,24]); find.value = 'Launch item 0';
    opener = new Element('slack-list-delete-field', root, [220,20,110,24]); opener.dataset.deleteFieldSelection = String(state.selection); opener.textContent = 'Delete field'; opener.onclick = () => mount('C_CHECK');
    picker = new Select('slack-list-view-select', root, [350,20,140,24]); picker.options = [{value:'V_FIELD_DELETE',textContent:'Deletion table'}]; picker.value = 'V_FIELD_DELETE';
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
  function mount(column, serial = ++state.serial) {
    if (panel) disconnect(panel);
    panel = new Element('slack-list-rename',root,[14,14,630,350]); panel.dataset = {renameSerial:String(serial),renameStage:'Editing',deleteColumn:column};
    const title = new Element('slack-list-rename-title',panel,[30,30,400,24]); title.textContent = 'Delete field';
    const chooser = new Select('slack-list-delete-field-choice',panel,[30,65,580,28]); chooser.options = fields.map(([value,textContent]) => ({value,textContent,disabled:false})); chooser.value = column;
    chooser.onchange = () => { if (state.reusePanel) { panel.dataset.renameSerial = String(++state.serial); panel.dataset.deleteColumn = chooser.value; } else mount(chooser.value); };
    const w = new Element('slack-list-delete-field-warning',panel,[30,100,580,42]); w.textContent = warning;
    new Element('slack-list-rename-message',panel,[30,150,580,44]);
    setStage('Editing'); chooser.focus(); opener.disabled = true;
  }
  function setStage(stage, retired = false) {
    panel.dataset.renameStage = stage; const chooser = byId.get('slack-list-delete-field-choice'); chooser.disabled = stage !== 'Editing';
    byId.get('slack-list-rename-message').textContent = retired ? 'Could not confirm the current schema. The earlier request will not be sent again.' : messages[stage] || 'Checking current fields...';
    const oldButtons = buttons.filter(e => e.isConnected); buttons = [];
    const priorCurrent = byId.get('slack-list-rename-current'); if (priorCurrent) disconnect(priorCurrent);
    if (['Conflict','Observed'].includes(stage)) new Element('slack-list-rename-current',panel,[30,205,580,44]).textContent = currentFields[stage];
    const specs = [['slack-list-rename-leave','Close list']];
    if (['Editing','Conflict','Observed'].includes(stage)) specs.push(['slack-list-rename-close',stage==='Editing'?'Cancel':stage==='Conflict'?'Close review':'Done']);
    if (stage==='Editing') specs.push(['slack-list-rename-confirm','Delete field permanently']);
    if (['Uncertain','Conflict'].includes(stage)) specs.push(['slack-list-rename-check','Check current fields']);
    for (const [i,[id,label]] of specs.entries()) {
      const e = oldButtons.find(e => e.id === id) || new Element(id,panel,[30+i*195,310,180,28]); e.bounds=[30+i*195,310,180,28]; e.textContent = label; buttons.push(e);
      e.onclick = () => {
        if (id==='slack-list-rename-close') { disconnect(panel); panel=null; buttons=[]; opener.disabled=false; opener.focus(); }
        if (id==='slack-list-rename-confirm') { setStage('Saving'); byId.get('slack-list-rename-leave').focus(); }
        if (id==='slack-list-rename-check') { setStage('Checking'); byId.get('slack-list-rename-leave').focus(); }
      };
    }
    oldButtons.filter(e => !buttons.includes(e)).forEach(disconnect);
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
      if (selector==='.slack-list-warning') return [partial,...panel?[byId.get('slack-list-delete-field-warning')]:[]];
      return [];
    },
    getElementById(id) { return byId.get(id)||null; },
    elementFromPoint(x,y) {
      const candidates = [...byId.values()].filter(e => e.isConnected && !e.hidden && !e.inert && (!panel || panel.contains(e)));
      return candidates.reverse().find(e=>{const b=e.getBoundingClientRect();return x>=b.left&&x<=b.right&&y>=b.top&&y<=b.bottom;}) || root;
    },
  };
  const context = vm.createContext({document,window:{},innerWidth:668,innerHeight:380,HTMLInputElement:Input,HTMLSelectElement:Select,
    Event:class{constructor(type){this.type=type;}},getComputedStyle:e=>({display:'block',visibility:'visible',overflowX:'visible',overflowY:'visible',outlineWidth:'2px',outlineOffset:'0px',outlineStyle:'solid',...e.style})});
  assert.equal(vm.runInContext(source,context).ok,true); const api = context.window.__listFieldDelete;
  return {api,state,document,context,byId,root:()=>root,panel:()=>panel,find:()=>find,opener:()=>opener,rows:()=>rows,
    setStage, mount, disconnect, revealHook(fn){pendingRevealHook=fn;},
    open(){assert.equal(api.bindFind().ok,true);assert.equal(api.rememberDelete().ok,true);assert.equal(api.click('#slack-list-delete-field').ok,true);assert.equal(api.draft().ok,true);},
    hidden(){this.open();assert.equal(api.choose('C_HIDDEN').ok,true);assert.equal(api.draft().ok,true);},
    model(){const d=api.draft();return {serial:Number(d.serial),visit:Number(d.visit),selection:Number(d.selection),column:d.column,choices:fields.map(f=>f.slice()),stage:d.stage,canConfirm:d.canConfirm};},
    reopen(){const oldSerial=panel.dataset.renameSerial;disconnect(root);state.visit++;state.selection++;newRoot();mount('C_HIDDEN',oldSerial);setStage('Checking');byId.get('slack-list-rename-leave').focus();},
  };
}

test('actual checker opens, adopts a true chooser remount, and validates the exact host draft',()=>{
  const h=harness();h.open();const old=h.api.draft();assert.equal(h.api.choose('C_HIDDEN').ok,true);const current=h.api.draft();assert.equal(current.ok,true);assert.notEqual(current.serial,old.serial);
  assert.equal(h.api.sameDraft(h.model()).ok,true);const geo=h.api.geometry('edit');assert.equal(geo.ok,true,JSON.stringify(geo));assert.equal(geo.sameActiveElement,true);
});
test('reused panel with a changed serial cannot impersonate the required remount',()=>{
  const h=harness();h.open();h.state.reusePanel=true;assert.equal(h.api.choose('C_HIDDEN').ok,true);assert.equal(h.api.draft().ok,false);assert.equal(h.api.click('#slack-list-rename-confirm').ok,false);
});
test('wrong root, visit, selection, serial and Find node refuse before reveal or action',()=>{
  for(const mutate of [h=>h.root().dataset.dialogGeneration='99',h=>h.root().dataset.viewSelection='99',h=>h.panel().dataset.renameSerial='99',h=>h.disconnect(h.root()),h=>h.find().value='different']){
    const h=harness();h.hidden();mutate(h);const before=[h.state.scrolls,h.state.clicks,h.state.changes,h.state.focusCalls];assert.equal(h.api.click('#slack-list-rename-confirm').ok,false);assert.equal(h.api.choose('C_ABSENT').ok,false);assert.deepEqual([h.state.scrolls,h.state.clicks,h.state.changes,h.state.focusCalls],before);
  }
});
test('owner or action target replacement during reveal refuses the click',()=>{
  const h=harness();h.hidden();h.revealHook(()=>{h.root().dataset.viewSelection='new';});const clicks=h.state.clicks;assert.equal(h.api.click('#slack-list-rename-confirm').ok,false);assert.equal(h.state.clicks,clicks);
});
test('full host draft rejects serial, visit, selection, column, choices, stage and canConfirm mismatches',()=>{
  const h=harness();h.hidden();const model=h.model();
  for(const [key,value] of [['serial',99],['visit',99],['selection',99],['column','C_ABSENT'],['choices',model.choices.slice(1)],['stage','Saving'],['canConfirm',false]]) assert.equal(h.api.sameDraft({...model,[key]:value}).ok,false,key);
  assert.equal(h.api.sameDraft({...model,serial:9007199254740992}).ok,false);
});
test('geometry never manufactures focus or scrolls to conceal warning clipping',()=>{
  const h=harness();h.hidden();const baseline=[h.state.scrolls,h.state.focusCalls];h.byId.get('slack-list-delete-field-warning').bounds[1]=365;
  assert.equal(h.api.geometry('edit').ok,false);assert.deepEqual([h.state.scrolls,h.state.focusCalls],baseline);
});
test('strict whole outline rejects fractional viewport and clipping-ancestor overflow',()=>{
  for(const mode of ['viewport','ancestor']){const h=harness();h.hidden();const e=h.byId.get('slack-list-delete-field-choice');if(mode==='viewport')e.bounds=[1.99999,65,580,28];else{h.panel().style.overflowX='hidden';e.bounds=[15.99999,65,580,28];}assert.equal(h.api.geometry('edit').ok,false);}
});
test('wrong selected option, warning, message, footer or rendered row is rejected',()=>{
  for(const mutate of [h=>h.byId.get('slack-list-delete-field-choice').options[0].textContent='Changed',h=>h.byId.get('slack-list-delete-field-warning').textContent='wrong',h=>h.byId.get('slack-list-rename-message').textContent='wrong',h=>h.byId.get('slack-list-rename-confirm').textContent='Save',h=>h.rows()[0].dataset.rowId='wrong']){const h=harness();h.hidden();mutate(h);assert.equal(h.api.geometry('edit').ok,false);}
});
test('Cancel is zero-submit and returns to the same deletion opener',()=>{
  const h=harness();h.hidden();assert.equal(h.api.click('#slack-list-rename-close').ok,true);assert.equal(h.api.returnedDelete().ok,true);assert.equal(h.api.rememberDelete().ok,true);assert.equal(h.api.click('#slack-list-delete-field').ok,true);assert.equal(h.api.draft().column,'C_CHECK');
});
test('new visit explicitly rebinds Find with no cross-visit continuity claim',()=>{
  const h=harness();h.hidden();const serial=h.api.draft().serial;h.reopen();const rebound=h.api.bindFind();assert.equal(rebound.ok,true);assert.equal(rebound.rebound,true);assert.equal(rebound.findContinuityClaimed,false);assert.equal(h.api.draft().serial,serial);assert.equal(h.api.rememberDelete().ok,true);assert.equal(h.api.rememberBusy().ok,true);assert.equal(h.api.sameDraft(h.model()).ok,true);
  assert.equal(h.api.bindFind().ok,false,'same owner cannot rebind away a failure');
});
test('saving locks target and requires the actual enabled fallback',()=>{
  const h=harness();h.hidden();assert.equal(h.api.click('#slack-list-rename-confirm').ok,true);assert.equal(h.api.choose('C_ABSENT').ok,false);const g=h.api.geometry('saving');assert.equal(g.ok,true,JSON.stringify(g));
  h.document.activeElement=h.opener();assert.equal(h.api.geometry('saving').ok,false);
});
test('all seven actual-helper stages retain exact focus through check, retired visit and Done',()=>{
  const h=harness();h.hidden();const receipt=stage=>{const value=h.api.geometry(stage);assert.equal(value.ok,true,`${stage}: ${JSON.stringify(value)}`);return value;};
  receipt('edit');assert.equal(h.api.click('#slack-list-rename-confirm').ok,true);receipt('saving');
  h.setStage('Uncertain');receipt('uncertain');assert.equal(h.api.click('#slack-list-rename-check').ok,true);
  h.setStage('Conflict');receipt('conflict');assert.equal(h.api.click('#slack-list-rename-check').ok,true);
  h.reopen();assert.equal(h.api.bindFind().ok,true);assert.equal(h.api.rememberDelete().ok,true);assert.equal(h.api.rememberBusy().ok,true);
  h.setStage('Uncertain',true);receipt('retired');assert.equal(h.api.click('#slack-list-rename-check').ok,true);
  h.setStage('Observed');receipt('observed');assert.equal(h.api.click('#slack-list-rename-close').ok,true);assert.equal(h.api.returnedDelete().ok,true);receipt('table');
});
test('changed Find range or phase during reveal cannot proceed to the action',()=>{
  for(const change of [h=>h.find().selectionStart=0,h=>h.panel().dataset.renameStage='Uncertain']){
    const h=harness();h.hidden();const before=h.state.clicks;h.revealHook(()=>change(h));assert.equal(h.api.click('#slack-list-rename-confirm').ok,false);assert.equal(h.state.clicks,before);
  }
});
test('a new visible top dialog refuses mutation before reveal',()=>{
  const h=harness();h.hidden();const query=h.document.querySelectorAll;h.document.querySelectorAll=s=>s==='[role="dialog"]'?[...query(s),{isConnected:true,closest:()=>null,getBoundingClientRect:()=>({width:10})}]:query(s);
  const before=h.state.scrolls;assert.equal(h.api.click('#slack-list-rename-confirm').ok,false);assert.equal(h.api.choose('C_ABSENT').ok,false);assert.equal(h.state.scrolls,before);
});
test('missing outline and capture-time footer or current-field clipping fail without scrolling',()=>{
  for(const mode of ['outline','footer','current']) {
    const h=harness();h.hidden();let stage='edit';
    if(mode==='outline')h.byId.get('slack-list-delete-field-choice').style.outlineStyle='none';
    if(mode==='footer')h.byId.get('slack-list-rename-confirm').bounds[0]=600;
    if(mode==='current'){h.api.click('#slack-list-rename-confirm');h.api.geometry('saving');h.setStage('Uncertain');h.api.click('#slack-list-rename-check');h.setStage('Conflict');h.byId.get('slack-list-rename-current').bounds[1]=370;stage='conflict';}
    const before=[h.state.scrolls,h.state.focusCalls];assert.equal(h.api.geometry(stage).ok,false);assert.deepEqual([h.state.scrolls,h.state.focusCalls],before);
  }
});
test('same-id replacement of a retained fallback cannot pass phase-focus proof',()=>{
  const h=harness();h.hidden();h.api.click('#slack-list-rename-confirm');assert.equal(h.api.geometry('saving').ok,true);
  h.setStage('Uncertain');const old=h.byId.get('slack-list-rename-leave'), replacement=Object.assign(Object.create(Object.getPrototypeOf(old)),old);
  h.byId.set(old.id,replacement);h.document.activeElement=replacement;
  assert.equal(h.api.geometry('uncertain').ok,false);
});
