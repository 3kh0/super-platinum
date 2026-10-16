import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import test from 'node:test';

const base=process.argv[2]||process.cwd();
const restore=readFileSync(path.join(base,'src/desktop/view/slack_lists/rename_restore.js'),'utf8');
const focus=readFileSync(path.join(base,'src/desktop/view/slack_lists/rename_focus.js'),'utf8');
function harness() {
  const frames=[];let calls=[];let panelPresent=true;let dialogs;
  const element=(id)=>({id,isConnected:true,disabled:false,closest:()=>null,
    getBoundingClientRect:()=>({width:50}),matches:()=>true,focus:()=>calls.push(id)});
  const body={};const header=element('text-opener'),rename=element('wrong-rename-header'),close=element('list-close'),select=element('text-name');
  const kind=element('field-kind');
  header.dataset={newTextFieldView:'V_TABLE'};rename.dataset={renameColumn:'C_NUMBER'};
  const dialog={...element('dialog'),dataset:{dialogGeneration:'1',viewSelection:'8'},
    querySelector:s=>s==='#slack-list-close'?close:s==='#slack-list-new-text-field'?header:s==='#slack-list-view-select'?{value:'V_TABLE'}:null,
    querySelectorAll:s=>s==='button[data-precision-column]'?[header]:s==='button[data-rename-column]'?[rename]:[],
  };
  const originalQuery=dialog.querySelector;
  let opener=header;dialog.querySelector=s=>s==='#slack-list-new-text-field'?opener:originalQuery(s);
  let observerDisconnects=0;
  const root={...element('panel'),dataset:{renameSerial:'7',renameColumn:'',renameView:'V_TABLE',newTextFieldView:'V_TABLE'},
    closest:s=>s==='.slack-list-dialog'?dialog:null,contains:e=>e===select||e===kind,
    addEventListener:()=>{},querySelector:s=>s==='input:not([disabled])'&&!select.disabled?select:s==='#slack-list-rename-close:not([disabled])'?close:null,
  };
  dialogs=[dialog,root];
  const document={body,activeElement:body,addEventListener:()=>{},removeEventListener:()=>{},
    getElementById:id=>id==='slack-list-rename'&&panelPresent?root:null,
    querySelector:s=>s==='.slack-list-dialog'?dialog:s==='#slack-list-rename'&&panelPresent?root:null,
    querySelectorAll:()=>dialogs,
  };
  const context=vm.createContext({document,requestAnimationFrame:fn=>(frames.push(fn),frames.length),cancelAnimationFrame:()=>{},
    setTimeout:()=>1,clearTimeout:()=>{},MutationObserver:class{observe(){}disconnect(){observerDisconnects++;}}});
  vm.runInContext(restore,context);
  context.root=root;
  return {context,root,dialog,document,header,select,kind,calls,
    remove(){panelPresent=false;root.isConnected=false;dialogs=[dialog];},
    top(e){dialogs.push(e);},flush(){while(frames.length)frames.shift()();},
    arm(){vm.runInContext('armListRenameReturn(root)',context);},
    replaceOpener(){opener={...header};},
    disconnects:()=>observerDisconnects};
}
test('Text field close restores Text field opener rather than Rename view',()=>{
  const h=harness();h.arm();h.remove();h.flush();assert.deepEqual(h.calls,['text-opener']);assert.ok(h.disconnects()>0);
});
test('disabled Text field action falls back to current List Close',()=>{
  const h=harness();h.arm();h.header.disabled=true;h.remove();h.flush();assert.deepEqual(h.calls,['list-close']);
});
test('changed visit never restores to replacement List',()=>{
  const h=harness();h.arm();h.remove();h.dialog.dataset.dialogGeneration='2';h.flush();assert.deepEqual(h.calls,[]);
});
test('replacement schema panel serial retires return ownership',()=>{
  const h=harness();h.arm();h.root.dataset.renameSerial='9';h.flush();assert.deepEqual(h.calls,[]);assert.ok(h.disconnects()>0);
});
test('new top dialog wins over Text field return',()=>{
  const h=harness();h.arm();h.remove();h.top({...h.header,id:'new-dialog'});h.flush();assert.deepEqual(h.calls,[]);
});
test('intervening live interactive focus is preserved',()=>{
  const h=harness();h.arm();h.remove();h.document.activeElement=h.header;h.flush();assert.deepEqual(h.calls,[]);
});
test('actual focus helper selects editable Text field name',()=>{
  const h=harness();vm.runInContext(focus,h.context);h.flush();assert.deepEqual(h.calls,['text-name']);
});
test('actual focus helper does not refocus a current select',()=>{
  const h=harness();h.document.activeElement=h.select;vm.runInContext(focus,h.context);h.flush();assert.deepEqual(h.calls,[]);
});
test('actual focus helper retires changed serial before RAF',()=>{
  const h=harness();vm.runInContext(focus,h.context);h.root.dataset.renameSerial='8';h.flush();assert.deepEqual(h.calls,[]);assert.ok(h.disconnects()>0);
});

function assertNarrowBinding(source) {
  const initial=source.match(/let initial_size = ([\s\S]*?);\r?\n/)?.[1];
  assert.ok(initial,'initial_size matcher must exist');
  const aliases=[...initial.matchAll(/"([a-z0-9-]+)"/g)].map(m=>m[1]);
  assert.equal(aliases.filter(v=>v==='slack-list-text-field-create-narrow').length,1);
  assert.equal(aliases.includes('slack-list-text-field-create'),false);
  assert.match(initial,/\{\s*WINDOW_MIN_INNER_SIZE\s*\}\s*else\s*\{\s*WINDOW_DEFAULT_INNER_SIZE\s*\}/);
}
test('named Text field fixture is wired through catalog, boot and minimum narrow size',()=>{
  const media=readFileSync(path.join(base,'src/desktop/media.rs'),'utf8');
  assertNarrowBinding(media);
  const boot=readFileSync(path.join(base,'src/desktop/state/boot.rs'),'utf8');
  assert.match(boot,/"slack-list-text-field-create"\s*\|\s*"slack-list-text-field-create-narrow"\s*\|\s*"slack-list-number-field-create"\s*\|\s*"slack-list-number-field-create-narrow"\s*\|\s*"slack-list-checkbox-field-create"\s*\|\s*"slack-list-checkbox-field-create-narrow"\s*\|\s*"slack-list-date-field-create"\s*\|\s*"slack-list-date-field-create-narrow"\s*\|\s*"slack-list-select-field-create"\s*\|\s*"slack-list-select-field-create-narrow"\s*\|\s*"slack-list-multi-select-field-create"\s*\|\s*"slack-list-multi-select-field-create-narrow"\s*=>\s*\{\s*crate::slack_lists::fixture::text_field_create::install\(&mut state, name\);/);
  const catalog=readFileSync(path.join(base,'scripts/agent-ui-check.sh'),'utf8');
  assert.match(catalog,/^\s+slack-list-text-field-create slack-list-text-field-create-narrow\s*$/m);
});
test('source wiring regression rejects the original missing narrow-size branch',()=>{
  const media=readFileSync(path.join(base,'src/desktop/media.rs'),'utf8');
  const missing=media.replace('| "slack-list-text-field-create-narrow"','');
  assert.notEqual(media,missing);
  assert.throws(()=>assertNarrowBinding(missing));
});

test('changed view-selection generation refuses Text field return even on same view',()=>{
 const h=harness();h.arm();h.remove();h.dialog.dataset.viewSelection='9';h.flush();assert.deepEqual(h.calls,[]);
});

test('replacement same-address Text action cannot inherit close focus',()=>{
 const h=harness();h.arm();h.remove();h.replaceOpener();h.flush();assert.deepEqual(h.calls,[]);
});
test('wrong Text action view fails before arming restoration',()=>{
 const h=harness();h.header.dataset.newTextFieldView='V_OTHER';h.arm();h.remove();h.flush();assert.deepEqual(h.calls,[]);
});

test('shared field helper preserves active enabled Number type selector',()=>{const h=harness();h.document.activeElement=h.kind;vm.runInContext(focus,h.context);h.flush();assert.deepEqual(h.calls,[]);});
test('disabling admitted type and name moves focus to current modal Close',()=>{const h=harness();h.document.activeElement=h.kind;h.kind.disabled=true;h.select.disabled=true;vm.runInContext(focus,h.context);h.flush();assert.deepEqual(h.calls,['list-close']);});
test('Number fixture keeps named minimum narrow window and the shared boot route',()=>{
 const media=readFileSync(path.join(base,'src/desktop/media.rs'),'utf8');
 const initial=media.match(/let initial_size = ([\s\S]*?);\r?\n/)?.[1];assert.ok(initial);
 const aliases=[...initial.matchAll(/"([a-z0-9-]+)"/g)].map(m=>m[1]);assert.equal(aliases.filter(v=>v==='slack-list-number-field-create-narrow').length,1);assert.equal(aliases.includes('slack-list-number-field-create'),false);
 const boot=readFileSync(path.join(base,'src/desktop/state/boot.rs'),'utf8');assert.match(boot,/"slack-list-number-field-create"\s*\|\s*"slack-list-number-field-create-narrow"\s*\|\s*"slack-list-checkbox-field-create"\s*\|\s*"slack-list-checkbox-field-create-narrow"\s*\|\s*"slack-list-date-field-create"\s*\|\s*"slack-list-date-field-create-narrow"\s*\|\s*"slack-list-select-field-create"\s*\|\s*"slack-list-select-field-create-narrow"\s*\|\s*"slack-list-multi-select-field-create"\s*\|\s*"slack-list-multi-select-field-create-narrow"\s*=>\s*\{\s*crate::slack_lists::fixture::text_field_create::install/);
 const catalog=readFileSync(path.join(base,'scripts/agent-ui-check.sh'),'utf8');assert.match(catalog,/^\s+slack-list-number-field-create slack-list-number-field-create-narrow\s*$/m);
});

function assertCheckboxWiring(media,boot,catalog,view) {
 const initial=media.match(/let initial_size = ([\s\S]*?);\r?\n/)?.[1];assert.ok(initial);
 const aliases=[...initial.matchAll(/"([a-z0-9-]+)"/g)].map(m=>m[1]);
 assert.equal(aliases.filter(v=>v==='slack-list-checkbox-field-create-narrow').length,1);
 assert.equal(aliases.includes('slack-list-checkbox-field-create'),false);
 assert.match(boot,/"slack-list-checkbox-field-create"\s*\|\s*"slack-list-checkbox-field-create-narrow"\s*\|\s*"slack-list-date-field-create"\s*\|\s*"slack-list-date-field-create-narrow"\s*\|\s*"slack-list-select-field-create"\s*\|\s*"slack-list-select-field-create-narrow"\s*\|\s*"slack-list-multi-select-field-create"\s*\|\s*"slack-list-multi-select-field-create-narrow"\s*=>\s*\{\s*crate::slack_lists::fixture::text_field_create::install/);
 assert.match(catalog,/^\s+slack-list-checkbox-field-create slack-list-checkbox-field-create-narrow\s*$/m);
 assert.match(view,/option\s*\{\s*value:\s*"checkbox",\s*selected:\s*field_kind\s*==\s*"checkbox",\s*"Checkbox"\s*\}/);
}
test('Checkbox uses the shared selector, static route and exact minimum narrow size',()=>{
 const read=p=>readFileSync(path.join(base,p),'utf8');
 const inputs=[read('src/desktop/media.rs'),read('src/desktop/state/boot.rs'),read('scripts/agent-ui-check.sh'),read('src/desktop/view/slack_lists/rename.rs')];
 assertCheckboxWiring(...inputs);
 const missing=inputs[0].replace('| "slack-list-checkbox-field-create-narrow"','');
 assert.notEqual(missing,inputs[0]);assert.throws(()=>assertCheckboxWiring(missing,...inputs.slice(1)));
 const noBoot=inputs[1].replace('| "slack-list-checkbox-field-create-narrow"','');
 assert.notEqual(noBoot,inputs[1]);assert.throws(()=>assertCheckboxWiring(inputs[0],noBoot,...inputs.slice(2)));
});

function assertSelectWiring(media,boot,catalog) {
 assert.match(media,/\|\s*"slack-list-select-field-create-narrow"/);
 assert.match(boot,/"slack-list-select-field-create"\s*\|\s*"slack-list-select-field-create-narrow"\s*\|\s*"slack-list-multi-select-field-create"\s*\|\s*"slack-list-multi-select-field-create-narrow"\s*=>\s*\{\s*crate::slack_lists::fixture::text_field_create::install/);
 assert.match(catalog,/slack-list-select-field-create slack-list-select-field-create-narrow/);
}
test('Select static aliases use shared Add field route and exact narrow window',()=>{
 const inputs=['src/desktop/media.rs','src/desktop/state/boot.rs','scripts/agent-ui-check.sh'].map(p=>readFileSync(path.join(base,p),'utf8'));
 assertSelectWiring(...inputs);
 const missing=inputs[0].replace(/\s*\|\s*"slack-list-select-field-create-narrow"/,'');assert.notEqual(missing,inputs[0]);assert.throws(()=>assertSelectWiring(missing,...inputs.slice(1)));
 const noBoot=inputs[1].replace('"slack-list-select-field-create-narrow"','"removed-select-alias"');assert.notEqual(noBoot,inputs[1]);assert.throws(()=>assertSelectWiring(inputs[0],noBoot,inputs[2]));
});

test('Multi-select static aliases use only the shared seed and exact narrow mapping',()=>{
 const read=p=>readFileSync(path.join(base,p),'utf8');
 const media=read('src/desktop/media.rs'),boot=read('src/desktop/state/boot.rs'),catalog=read('scripts/agent-ui-check.sh');
 const check=(m,b,c)=>{
  const initial=m.match(/let initial_size = ([\s\S]*?);\r?\n/)?.[1];assert.ok(initial);
  const aliases=[...initial.matchAll(/"([a-z0-9-]+)"/g)].map(x=>x[1]);
  assert.equal(aliases.filter(x=>x==='slack-list-multi-select-field-create-narrow').length,1);
  assert.equal(aliases.includes('slack-list-multi-select-field-create'),false);
  assert.match(b,/"slack-list-multi-select-field-create"\s*\|\s*"slack-list-multi-select-field-create-narrow"\s*=>\s*\{\s*crate::slack_lists::fixture::text_field_create::install/);
  assert.match(c,/^\s+slack-list-multi-select-field-create slack-list-multi-select-field-create-narrow\s*$/m);
 };
 check(media,boot,catalog);
 assert.throws(()=>check(media.replace('| "slack-list-multi-select-field-create-narrow"',''),boot,catalog));
 assert.throws(()=>check(media,boot.replace('"slack-list-multi-select-field-create-narrow"','"other"'),catalog));
 assert.throws(()=>check(media,boot,catalog.replace('slack-list-multi-select-field-create slack-list-multi-select-field-create-narrow','')));
});
