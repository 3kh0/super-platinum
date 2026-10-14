import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import fs from 'node:fs';
const source = fs.readFileSync(new URL('../src/desktop/view/global_notifications/keywords.js',import.meta.url),'utf8');
function setup() {
  const listeners = new Map(), panelListeners = new Map();
  const input = {id:'global-keywords-input',dataset:{seed:'1'},value:'saved',selectionStart:2,selectionEnd:2,disabled:false,
    addEventListener(name, fn){listeners.set(name, fn);},
    dispatchEvent(e){listeners.get(e.type)?.(e); this.events=(this.events||0)+1;},
  };
  const button={id:'global-keywords-discard',disabled:false,closest(){return this;}};
  const panel={dataset:{visit:'4'},contains(e){return e===input||e===button;},addEventListener(name,fn){panelListeners.set(name,fn);}};
  const document={getElementById(id){return id==='global-keywords-input'?input:id==='global-notifications'?panel:null;}};
  const context={window:{},document,Event:class {constructor(type){this.type=type;}}};
  vm.runInNewContext(source,context);
  const api=context.window.__globalKeywordSeed;api.install(4);
  return {input,panel,document,api,button,click(){panelListeners.get('click')({target:button});},edit(text){input.value=text;input.dispatchEvent({type:'input'});}};
}
test('Discard explicitly seeds once; ordinary typing and caret survive repeated evaluation',()=>{
 const t=setup();t.edit('draft');t.click();t.input.dataset.seed='2';assert.equal(t.api.sync(4,2,'saved'),true);assert.equal(t.input.value,'saved');
 t.edit('new 🦊');t.input.selectionStart=4;t.input.selectionEnd=4;t.api.sync(4,2,'saved');assert.equal(t.input.value,'new 🦊');assert.equal(t.input.selectionStart,4);
});
test('new native input after click supersedes delayed reset and is republished',()=>{
 const t=setup();t.edit('draft');t.click();t.input.dataset.seed='2';t.edit('newest');t.input.selectionStart=3;const events=t.input.events;
 assert.equal(t.api.sync(4,2,'saved'),false);assert.equal(t.input.value,'newest');assert.equal(t.input.selectionStart,3);assert.equal(t.input.events,events+1);assert.equal(t.input.dataset.appliedSeed,'2');
});
test('composition wins even without a value or input event yet',()=>{
 const t=setup();t.edit('draft');t.click();t.input.dataset.seed='2';t.input.dispatchEvent({type:'compositionstart'});
 assert.equal(t.api.sync(4,2,'saved'),false);assert.equal(t.input.value,'draft');assert.equal(t.input.__keywordSeed.composing,true);
});
test('delayed old visit or superseded seed cannot reset a current field',()=>{
 const t=setup();t.edit('draft');t.click();t.input.dataset.seed='3';assert.equal(t.api.sync(4,2,'old'),false);assert.equal(t.input.value,'draft');
 t.panel.dataset.visit='5';assert.equal(t.api.sync(4,3,'old'),false);assert.equal(t.input.value,'draft');
});
test('successful Save applies canonical spacing and retains clean equal-value caret',()=>{
 const t=setup();t.button.id='global-keywords-save';t.edit('a,b');t.click();t.input.dataset.seed='2';assert.equal(t.api.sync(4,2,'a, b'),true);assert.equal(t.input.value,'a, b');
 t.input.selectionStart=1;t.click();t.input.dataset.seed='3';assert.equal(t.api.sync(4,3,'a, b'),true);assert.equal(t.input.selectionStart,1);
});
test('new DOM node is isolated from captured old input action',()=>{
 const t=setup();t.edit('old');t.click();const original=t.document.getElementById.bind(t.document);
 const replacement={id:'global-keywords-input',dataset:{seed:'2'},value:'new native draft',selectionStart:3,selectionEnd:3,addEventListener(){},dispatchEvent(){throw new Error('replacement must not replay old input');}};
 t.document.getElementById=id=>id==='global-keywords-input'?replacement:original(id);
 t.panel.contains=e=>e===replacement||e===t.button;
 assert.equal(t.api.sync(4,2,'saved'),true);assert.equal(replacement.value,'new native draft');assert.equal(replacement.selectionStart,3);assert.equal(replacement.__keywordSeed.action,null);assert.equal(t.input.value,'old');
});
