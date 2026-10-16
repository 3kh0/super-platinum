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
  const body={};const header=element('precision-header'),rename=element('wrong-rename-header'),close=element('list-close'),select=element('precision-select');
  header.dataset={precisionColumn:'C_NUMBER'};rename.dataset={renameColumn:'C_NUMBER'};
  const dialog={...element('dialog'),dataset:{dialogGeneration:'1',viewSelection:'8'},
    querySelector:s=>s==='#slack-list-close'?close:null,
    querySelectorAll:s=>s==='button[data-precision-column]'?[header]:s==='button[data-rename-column]'?[rename]:[],
  };
  let observerDisconnects=0;
  const root={...element('panel'),dataset:{renameSerial:'7',renameColumn:'C_NUMBER',precisionColumn:'C_NUMBER'},
    closest:s=>s==='.slack-list-dialog'?dialog:null,contains:e=>e===select,
    addEventListener:()=>{},querySelector:s=>s==='#slack-list-precision:not([disabled])'&&!select.disabled?select:s==='#slack-list-rename-close:not([disabled])'?close:null,
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
  return {context,root,dialog,document,header,select,calls,
    remove(){panelPresent=false;root.isConnected=false;dialogs=[dialog];},
    top(e){dialogs.push(e);},flush(){while(frames.length)frames.shift()();},
    arm(){vm.runInContext('armListRenameReturn(root)',context);},
    disconnects:()=>observerDisconnects};
}
test('precision close restores exact precision action instead of same-column rename',()=>{
  const h=harness();h.arm();h.remove();h.flush();assert.deepEqual(h.calls,['precision-header']);assert.ok(h.disconnects()>0);
});
test('disabled precision action falls back to current List Close',()=>{
  const h=harness();h.arm();h.header.disabled=true;h.remove();h.flush();assert.deepEqual(h.calls,['list-close']);
});
test('changed visit never restores to replacement List',()=>{
  const h=harness();h.arm();h.remove();h.dialog.dataset.dialogGeneration='2';h.flush();assert.deepEqual(h.calls,[]);
});
test('replacement schema panel serial retires return ownership',()=>{
  const h=harness();h.arm();h.root.dataset.renameSerial='9';h.flush();assert.deepEqual(h.calls,[]);assert.ok(h.disconnects()>0);
});
test('new top dialog wins over precision return',()=>{
  const h=harness();h.arm();h.remove();h.top({...h.header,id:'new-dialog'});h.flush();assert.deepEqual(h.calls,[]);
});
test('intervening live interactive focus is preserved',()=>{
  const h=harness();h.arm();h.remove();h.document.activeElement=h.header;h.flush();assert.deepEqual(h.calls,[]);
});
test('actual focus helper selects editable Decimals',()=>{
  const h=harness();vm.runInContext(focus,h.context);h.flush();assert.deepEqual(h.calls,['precision-select']);
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
  assert.equal(aliases.filter(v=>v==='slack-list-number-precision-narrow').length,1);
  assert.equal(aliases.includes('slack-list-number-precision'),false);
  assert.match(initial,/\{\s*WINDOW_MIN_INNER_SIZE\s*\}\s*else\s*\{\s*WINDOW_DEFAULT_INNER_SIZE\s*\}/);
}
test('named precision fixture is wired through catalog, boot and minimum narrow size',()=>{
  const media=readFileSync(path.join(base,'src/desktop/media.rs'),'utf8');
  assertNarrowBinding(media);
  const boot=readFileSync(path.join(base,'src/desktop/state/boot.rs'),'utf8');
  assert.match(boot,/"slack-list-number-precision"\s*\|\s*"slack-list-number-precision-narrow"\s*=>\s*\{\s*crate::slack_lists::fixture::column_precision::install\(&mut state, name\);/);
  const catalog=readFileSync(path.join(base,'scripts/agent-ui-check.sh'),'utf8');
  assert.match(catalog,/^\s+slack-list-number-precision slack-list-number-precision-narrow\s*$/m);
});
test('source wiring regression rejects the original missing narrow-size branch',()=>{
  const media=readFileSync(path.join(base,'src/desktop/media.rs'),'utf8');
  const missing=media.replace('| "slack-list-number-precision-narrow"','');
  assert.notEqual(media,missing);
  assert.throws(()=>assertNarrowBinding(missing));
});
