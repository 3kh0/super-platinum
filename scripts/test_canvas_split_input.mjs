// Independent finite request contract, without a browser or input dispatch.
import assert from 'node:assert/strict';
import {validRequest} from './canvas_split_input.mjs';
import {typingPlan} from './canvas_end_typing_input.mjs';
import {typingPlan as middlePlan} from './canvas_middle_typing_input.mjs';

let checks=0;
const check=(value, expected)=>{assert.equal(value,expected);checks++;};
for(const [corpus, parent, offset] of [
  ['ascii','Keep this',9],['unicode','Caf\u00e9 & \u65e5\u672c\u{1f600}',11],
]) {
  for(const mode of ['middle','end','reuse','start']) {
    for(const ordinal of [0,1]) {
      const value={ok:true,fixture:'canvas-structure',nonce:'a'.repeat(32),ordinal,
        fieldId:'canvas-text-draft',generation:'3',visit:'0',corpus,mode,
        seed:parent+(['middle','start'].includes(mode)?'Second paragraph':''),offset:mode==='start'?0:offset,revision:0};
      check(validRequest(value,corpus,mode),true);
      for(const other of ['middle','end','reuse','start'].filter(candidate=>candidate!==mode)) check(validRequest(value,corpus,other),false);
      for(const [key, altered] of [['seed',value.seed+'x'],['offset',offset+1],
          ['ordinal',2],['ordinal',true],['revision',1],['fixture','home'],
          ['mode','unknown'],['corpus','unknown'],['nonce','b'.repeat(31)]]) {
        check(validRequest({...value,[key]:altered},corpus,mode),false);
      }
      check(validRequest(value,corpus,'unknown'),false);
      for(const typing of ['correction','erase','restore']) {
        const typedValue=mode==='middle'&&corpus==='unicode'?{...value,seed:parent+'Second \u65e5\u672c\u{1f600} & paragraph'}:value;
        check(validRequest({...typedValue,typing},corpus,mode,typing),typing!=='restore'||['middle','start'].includes(mode));
        check(validRequest({...value,typing},corpus,mode),false);
        check(validRequest(value,corpus,mode,typing),false);
        check(validRequest({...value,typing:'arbitrary'},corpus,mode,typing),false);
      }
    }
  }
}
for(const corpus of ['ascii','unicode']) {
  const expected=corpus==='ascii'?['New','New draftx','New draft','New draft','New paragraph']
    :['New','New 日本😀 draftx','New 日本😀 draft','New 日本😀 draft','New 日本😀 paragraph'];
  assert.deepEqual(typingPlan(corpus,'correction').map(x=>x.after),expected);checks++;
  assert.deepEqual(typingPlan(corpus,'erase').map(x=>x.after),['x','']);checks++;
  check(typingPlan(corpus,'arbitrary'),null);
}
for(const corpus of ['ascii','unicode']) {
 const tail=corpus==='ascii'?'Second paragraph':'Second \u65e5\u672c\u{1f600} & paragraph';
 assert.deepEqual(middlePlan(corpus,'restore').filter(a=>a.type!=='select').map(a=>a.after),[tail+'x',tail]);checks++;
 assert.deepEqual(middlePlan(corpus,'erase').filter(a=>a.type!=='select').map(a=>a.after),[tail+'x','']);checks++;
 check(middlePlan(corpus,'correction')[0].end,tail.length);
 check(middlePlan(corpus,'unknown'),null);
}
check(typingPlan('arbitrary','correction'),null);
console.log(JSON.stringify({ok:true,checks,scope:'finite request admission only; no browser/input'}));
