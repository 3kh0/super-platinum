const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const table=JSON.parse(fs.readFileSync(`${__dirname}/emoticons.json`,'utf8'));
const create=vm.runInNewContext(`${fs.readFileSync(`${__dirname}/emoticons.js`,'utf8')};listCommentDeltaEmoticons`);
const moduleUnderTest=create(table);
let groups=0;
function test(name,fn){fn();groups++;console.log(`PASS ${name}`);}
function quill(text,previous){return {
 getText(start,length){return text.slice(start,start+length);},
 getContents(start,length){return {ops:[previous||{insert:text.slice(start,start+length)}]};},
};}
function plan(text,{prefix=text,index=text.length,format={},previous,suffix=' '}={}){
 return moduleUnderTest.plan(quill(text,previous),{index,length:0},{prefix,format},suffix);
}
test('all46 original table entries and UTF16 prefix position',()=>{
 assert.equal(Object.keys(table).length,46);
 for(const [input,name]of Object.entries(table)){
  const text=`\u{1f604} ${input}`,result=plan(text);
  assert.equal(result.text,`:${name}:`);assert.equal(result.start,3);
  assert.equal(result.length,input.length);assert.equal(result.selection,5);
 }
 assert.equal(plan(':)').text,':slightly_smiling_face:');
});
test('exact boundaries and last match in prefix, not caret-adjacent',()=>{
 for(const text of ['x:)','x<3',':)x',':d',':Pz'])assert.equal(plan(text),null,text);
 for(const suffix of ['',' ','?','.',',','!','|'])assert.equal(plan(`:)${suffix}`).text,':slightly_smiling_face:');
 for(const suffix of [':',';',')','x'])assert.equal(plan(`:)${suffix}`),null);
 const last=plan(':) :D hello');assert.equal(last.text,':smile:');assert.equal(last.start,3);
 assert.equal(plan(':) :D hello').selection,11);
});
test('format/code/fences and exact source numbered-list exclusion',()=>{
 for(const key of ['link','code','code-block'])assert.equal(plan(':)',{format:{[key]:true}}),null);
 assert.equal(plan('` :)'),null);assert.equal(plan('```\n:)'),null);
 assert.equal(plan('`x` :)').text,':slightly_smiling_face:');
 assert.equal(plan('```x``` :)').text,':slightly_smiling_face:');
 assert.equal(plan('7) seven\n8)',{prefix:'8)'}),null);
 assert.equal(plan('6) six\n8)',{prefix:'8)'}).text,':sunglasses:');
 assert.equal(plan('7)\n8)',{prefix:'8)'}).text,':sunglasses:', 'source strict start>3');
});
test('original prefix-local URL ranges and preceding semantic operation',()=>{
 // Deliberately different prefix/document offsets pin source coordinate use.
 const text='\n\n:)';
 assert.equal(plan(text,{prefix:'http://example.com :D',index:4}),null);
 assert.equal(plan('x :)',{previous:{insert:'x'}}),null);
 assert.equal(plan('x :)',{previous:{insert:{slackemoji:{text:':smile:'}}}}).text,':slightly_smiling_face:');
 assert.equal(plan('http://example.com :D').text,':smile:');
});
test('owned codec admission precedes source API edits, no added history boundary',()=>{
 const calls=[],token={private:true};
 const q={...quill('Happy :)'),
  deleteText(...args){calls.push(['delete',...args]);},
  insertEmbed(...args){calls.push(['embed',...args]);},
  insertText(...args){calls.push(['text',...args]);},
  setSelection(...args){calls.push(['selection',...args]);},
  history:{cutoff(){throw Error('unexpected cutoff');}}};
 const codec={prepare(ops){assert.equal(ops[0].insert.slackemoji.text,':slightly_smiling_face:');return {ops:[{insert:{slackemoji:token}}],commit(){calls.push(['commit']);}};}};
 assert.equal(moduleUnderTest.convert(q,codec,{index:8,length:0},{prefix:'Happy :)',format:{}},' '),true);
 assert.deepEqual(calls,[['commit'],['delete',6,2],['embed',6,'slackemoji',token],['text',7,' '],['selection',8]]);
 calls.length=0;
 assert.equal(moduleUnderTest.convert(q,codec,{index:8,length:0},{prefix:'Happy :)',format:{}}),true);
 assert.deepEqual(calls,[['commit'],['delete',6,2],['embed',6,'slackemoji',token],['selection',7]]);
 calls.length=0;
 assert.throws(()=>moduleUnderTest.convert(q,{prepare(){throw Error('missing appearance');}},{index:8,length:0},{prefix:'Happy :)'}));
 assert.deepEqual(calls,[]);
});
test('enabled preparation requires every positive finite appearance once',()=>{
 let count=0;
 moduleUnderTest.prepare({prepare(ops){assert.equal(new Set(ops.map(op=>op.insert.slackemoji.text)).size,ops.length);count=ops.length;return {commit(){}};}});
 assert.equal(count,new Set(Object.values(table)).size);
 assert.throws(()=>moduleUnderTest.prepare({prepare(){throw Error('unavailable');}}));
});
console.log(JSON.stringify({status:'PASS',groups,source:'actual installed emoticons.js',runtime:'modeled Quill/codec; no native'}));
