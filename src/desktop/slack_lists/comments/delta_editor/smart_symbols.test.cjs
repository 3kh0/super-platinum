const assert=require('node:assert/strict');
const {Delta,load,clone}=require('./smart_test_helpers.cjs');
const {smart,model}=load();let groups=0;
function test(name,fn){fn();groups++;console.log('PASS '+name);}
test('38 original pure source vectors: quotes, placeholders, pipes, prime, UTF16',()=>{
  for(const row of require('./smart_quotes.vectors.json').vectors) {
    const original=new Delta().insert(row.input),before=clone(original.ops);
    const result=original.compose(smart.quotes(original.ops));
    assert.equal(result.ops.map(o=>o.insert).join(''),row.expected,JSON.stringify(row.input));
    assert.deepEqual(clone(original.ops),before);
  }
});
test('semantic projection preserves opaque identity; quote replacement omits attrs',()=>{
  const token={text:':smile:',opaque:{original:true}},mention={id:'U1',label:'@A"B',opaque:7};
  const original=new Delta().insert({slackemoji:token}).insert('@A"B',{slackmention:mention})
    .insert(' "hello"',{unlink:true}).insert('\n');
  const before=clone(original.ops),change=smart.quotes(original.ops),result=original.compose(change);
  assert.deepEqual(clone(result.ops.slice(0,2)),before.slice(0,2));
  const inserts=change.ops.filter(o=>'insert'in o);
  assert.deepEqual(clone(inserts),[{insert:'\u201c'},{insert:'\u201d'}]);
  assert.deepEqual(clone(original.ops),before);
  assert.equal(result.length(),original.length());
});
test('full enabled sanitation before quotes, semantic labels/selectors and formatting-only',()=>{
  const ops=[{insert:'\u200b"a"\ufeff\u2028\u00a0b\u00ad \u2194\ufe0e \u2194\ufe0f\n'},
    {insert:'\u00a0\u2194\ufe0e',attributes:{slackslug:{id:'F1',label:'old',opaque:7}}},
    {insert:'\u2194\ufe0e',attributes:{slackmention:{id:'U1',label:'\u2194\ufe0e'}}},{insert:'\n'}];
  const before=clone(ops),clean=smart.sanitize(ops),repaired=clean.compose(model.brokenMentions(clean.ops,true));
  const result=repaired.compose(smart.quotes(repaired.ops));
  assert.equal(result.ops[0].insert,'\u201ca\u201d\n b \u2194 \u2194\ufe0f\n');
  assert.equal(result.ops[1].attributes.slackslug.label,' \u2194\ufe0e');
  assert.equal(result.ops[1].attributes.slackslug.opaque,7);
  assert.equal(result.ops[2].insert,'\u2194\ufe0e');
  assert.deepEqual(clone(smart.sanitize(ops,true).ops),before);
  assert.deepEqual(clone(ops),before);
});
function quill(text) {
  const calls=[];let doc=new Delta().insert(text+'\n');
  const history={stack:{undo:[],redo:[]},cutoff(){calls.push(['cutoff']);}};
  return {calls,history,get doc(){return doc;},
    getText:(start,length)=>doc.slice(start,start+length).ops.map(o=>typeof o.insert==='string'?o.insert:'').join(''),
    getContents:(start,length)=>doc.slice(start,start+length),
    updateContents(change,source){calls.push(['update',source,clone(change.ops)]);const previous=doc;doc=doc.compose(change);history.stack.undo.push({undo:doc.diff(previous),redo:change});},
    setSelection(...args){calls.push(['selection',...args]);},
  };
}
test('exact Space/Period substitution API order, UTF16, literal Undo and Redo',()=>{
  for(const [input,method,expected,suffix]of [['--','space','\u2014 ',' '],['->','space','\u2192 ',' '],['<-','space','\u2190 ',' '],['<->','space','\u2194 ',' '],['..','period','\u2026','.']]) {
    const prefix='\u{1f604} '+input,q=quill(prefix),format={unlink:true};
    assert.equal(smart[method](q,{index:prefix.length,length:0},{prefix,format}),false);
    assert.equal(q.doc.ops.map(o=>o.insert).join(''),'\u{1f604} '+expected+'\n');
    const item=q.history.stack.undo.at(-1);
    const literal=q.doc.compose(item.undo);
    assert.equal(literal.ops.map(o=>o.insert).join(''),prefix+suffix+'\n');
    assert.deepEqual(clone(literal.compose(item.redo).ops),clone(q.doc.ops));
    assert.deepEqual(q.calls.map(c=>c[0]),['update','cutoff','update','cutoff','selection']);
    assert.equal(q.calls[0][1],undefined);assert.equal(q.calls[2][1],undefined);
    assert.deepEqual(q.calls.at(-1),['selection',3+expected.length,0,'silent']);
  }
});
test('source punctuation/code refusals keep document and history untouched',()=>{
  for(const [text,method]of [['---','space'],['-->','space'],['--->','space'],['...','period'],['`x --','space'],['```\n..','period']]) {
    const q=quill(text),before=clone(q.doc.ops);
    assert.equal(smart[method](q,{index:text.length,length:0},{prefix:text,format:{}}),true,text);
    assert.deepEqual(clone(q.doc.ops),before);assert.deepEqual(q.calls,[]);
  }
  const q=quill('`x` --');assert.equal(smart.space(q,{index:6,length:0},{prefix:'`x` --',format:{}}),false);
});
console.log(JSON.stringify({status:'PASS',groups,originalVectors:38,actualVendoredDelta:true,native:false}));
