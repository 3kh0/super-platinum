// Inert replay of retained shipped matcher/ranker methods. No production ranker.
// Input facts are independently checked by the Python receipt validator.
import fs from 'node:fs';
import vm from 'node:vm';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {req,setState,reset,run} from '../tmp/workflow-picker-ranking-v3-prep/loader.mjs';
const base='tmp/workflow-share-search-ranking-20261021/source/';
const pins=JSON.parse(fs.readFileSync(base+'evidence.json','utf8'));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
for(const key of ['hHTo','4ucY','l24D','jhqG','LBiy','ukdy']) {
  const pin=pins.find(x=>x.key===key);assert(pin,'missing source pin '+key);
  const bytes=fs.readFileSync(pin.path);assert.equal(sha(bytes),pin.sha256);
  const original=fs.readFileSync(pin.original);
  assert.equal(sha(original),pin.originalSha256);
  assert.equal(sha(original.subarray(pin.startByte,pin.endByte)),pin.sha256);
}
const source=fs.readFileSync(base+'hHTo.js','utf8');
const cut=(a,b)=>{const start=source.indexOf(a),end=source.indexOf(b,start);assert(start>=0&&end>start);return source.slice(start,end);};
const exactMethods=cut('constructor({query:','filterMembers(')+cut('filterMembers(','filterTeams(')+cut('shouldFilterItem(e,t){','isFileExactMatch(');
const normal=req('hNDQ').B;
const Matcher=vm.runInNewContext('(class {'+exactMethods+'})',{
 r:{A:()=>true},a:{A:(x,p)=>p.split('.').reduce((v,k)=>v?.[k],x)},n:{A:a=>a.length?Math.min(...a):undefined},s:{A:(a,p)=>Object.values(a).some(p)},o:{A:(a,k)=>a.reduce((x,y)=>!x||y[k]<x[k]?y:x,undefined)},c:{lB:()=>false,or:()=>false},u:{Ay:req('jytj').Ay},h:{B:normal},O:{A:()=>[]},B:{t:q=>['me','you'].includes(normal(q))},C:{fy:r=>!!r.facts?.self},I:{j_:r=>r._real_name_normalized_lc,sJ:r=>r._display_name_normalized_lc,ZL:r=>r._email_normalized_lc},k:{z1:r=>!!(r.isUnknown||r.isNonExistent)},f:{cb:()=>false,Zx:r=>!!(r.isUnknown||r.isNonExistent)},v:{jT:r=>!!r.is_file},S:{SB:(_,r)=>r.facts.comparisonName},prependSlashToStr:q=>'/'+q});
const close=(a,b,label)=>assert(Math.abs(a-b)<=1e-8*Math.max(1,Math.abs(a),Math.abs(b)),label+': '+a+' != '+b);
function replay(input){
 const p=input.proof;assert.equal(p.source,'share_dialog_channel_picker');assert.equal(p.passthrough,false);
 const members=p.memberItems,channels=p.channelItems;assert.equal(p.inputCount,members.length+channels.length);
 const m=new Matcher({query:p.query,options:{entities:{members:{matchEmailField:true},channels:{}},allowEmptyQuery:true,fuzzyLimit:3,fuzzyJumpLimit:20,fuzzyCJK:true,fuzzyEmailSearchForwardOnly:true,sort:{preferExactMatch:true,avoidExactMatchForMembers:true}},remoteIds:{},passthroughRemoteFuzzyFiltering:false});
 const matched={members:m.filterMembers({},members,()=>true),channels:m.filterChannels({},channels,()=>true)};
 const expectedMatches=[];
 for(const kind of ['members','channels']){
   assert.deepEqual(Array.from(matched[kind],x=>x.item.id),p.groups[kind].map(x=>x.item.id),'original matcher identities');
   matched[kind].forEach((r,i)=>{
     const actual=p.groups[kind][i];close(r.score,actual.score,'fuzzy score');
     for(const key of ['isExactMatch','isSelf','isFlannelFuzzyPassthrough'])assert.equal(!!actual[key],!!r[key],key);
     assert.deepEqual(actual.item,r.item,'original matched item');
     expectedMatches.push({id:r.item.id,index:i,kind});
   });
 }
 assert.deepEqual(p.matches,expectedMatches,'family index mapping');
 // groups hold the projected source items; replay enriches only source metadata.
 const groups=structuredClone(p.groups);
 for(const kind of ['members','channels'])for(const row of groups[kind])row.source=input.remote[kind].includes(row.item.id)?'remote':'local';
 const state={...p.selectors,now:p.nowMillis,history:{},demotePassthrough:false};
 if(input.model!==null)state.modelWeights=input.model;
 reset();setState(state);
 const out=run(p.query,groups,{source:p.source,limit:24,model:'autocompleteFallback',avoidExactMatchForMembers:true});
 assert.equal(p.results.length,out.result.length,'mixed cutoff');
 out.result.forEach((r,i)=>{
   const a=p.results[i],kind=r.type==='member'?'members':'channels';
   assert.equal(a.id,r.id,'mixed result order');assert.equal(a.kind,kind,'mixed family');
   assert.equal(a.index,p.groups[kind].findIndex(x=>x.item.id===r.id),'original family index');
   close(a.modelScore,r.modelScore,'model score');
   assert.equal(a.features.length,r.features.length,'feature count');
   a.features.forEach((f,j)=>{assert.equal(f[0],r.features[j][0],'feature name');close(f[1],r.features[j][1],'feature value');});
 });
 return {input:p.inputCount,matched:expectedMatches.length,modelInput:Math.min(72,out.ordered.length),output:p.results.length,ids:p.results.map(x=>x.id)};
}
const inputs=JSON.parse(fs.readFileSync(0,'utf8'));
console.log(JSON.stringify({ok:true,scope:'Retained methods with explicitly supplied immutable selectors; synthetic host, not stock DOM/server',matcherMethodsSha256:sha(exactMethods),results:inputs.map(replay)}));
