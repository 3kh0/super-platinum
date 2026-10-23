// Actual vendored Delta and installed live converter; host catalogs are explicit
// synthetic observations. These are portable logic tests, not native UI proof.
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const {Delta,clone}=require('./smart_test_helpers.cjs');
const context=vm.createContext();vm.runInContext(fs.readFileSync(__dirname+'/live_emoji.js','utf8'),context);
const row=(name,text,single=false,multiple=false,variants=[])=>({name,appearance:{text},single,multiple,variants});
function observations() {
  return {locale:'en-US',currentLocale:'en-US',names:['smile','thumbsup','handshake','party','alias','foo_bar','foo-bar','123',
      'skin-tone-2','skin-tone-3','skin-tone-4','skin-tone-5','skin-tone-6','dangling'],
    unicode:['\u{1f604}','\u{1f44d}','\u{1f44d}\u{1f3fd}','\u{1f3fd}'],
    unicodeNames:[['1f604','smile'],['1f44d','thumbsup'],['1f44d-1f3fd','thumbsup::skin-tone-4'],['1f3fd','skin-tone-4']],
    rows:[row('smile','\u{1f604}'),row('thumbsup','\u{1f44d}',true,false,[['4',{text:'\u{1f44d}\u{1f3fd}'}]]),
      row('handshake','\u{1f91d}',true,true),row('party','party'),row('alias','party'),row('foo_bar','underscore'),row('foo-bar','hyphen'),row('123','123'),
      ...[2,3,4,5,6].map(n=>row('skin-tone-'+n,String.fromCodePoint(0x1f3f9+n)))]};
}
const emoji=text=>({insert:{slackemoji:{text}}});
function converter(obs=observations()){return context.listCommentDeltaLiveEmoji(Delta,obs)}
function output(input,obs){const ops=typeof input==='string'?[{insert:input}]:input;const result=converter(obs).transform(ops);return clone(new Delta(ops).compose(result.change).ops)}
let groups=0;function test(name,fn){fn();groups++;console.log('PASS '+name)}
test('complete canonical membership ordering, custom alias, negatives and unavailable appearance',()=>{
  assert.deepEqual(output(':SMILE: :foo-bar: :foo_bar: :alias: :unknown:\n'),[
    emoji(':smile:'),{insert:' '},emoji(':foo-bar:'),{insert:' '},emoji(':foo_bar:'),{insert:' '},emoji(':alias:'),{insert:' :unknown:\n'}]);
  assert.deepEqual(output(':smile: :dangling:'),[{insert:':smile: :dangling:'}]);
  const obs=observations();obs.names=obs.names.filter(n=>n!=='foo-bar');obs.rows=obs.rows.filter(r=>r.name!=='foo-bar');
  assert.deepEqual(output(':foo-bar:',obs),[emoji(':foo_bar:')]);
  assert.deepEqual(output('10:123: :123:'),[{insert:'10:123: '},emoji(':123:')]);
});
test('named URL, IPv6 and backticks guards differ from native Unicode replacement',()=>{
  assert.deepEqual(output('https://x.test/:smile: `:smile:` \u{1f604} `\u{1f604}`'),[
    {insert:'https://x.test/:smile: `:smile:` '},emoji(':smile:'),{insert:' `'},emoji(':smile:'),{insert:'`'}]);
  assert.deepEqual(output('2001:123:123:123:123:123:123:123'),[{insert:'2001:123:123:123:123:123:123:123'}]);
  for(const attributes of [{code:true},{link:'https://x.test'},{slackmention:{id:'U1',label:':smile:'}},{slackslug:{id:'F1',label:':smile:'}}])
    assert.deepEqual(output([{insert:':smile: \u{1f604}',attributes}]),[{insert:':smile: \u{1f604}',attributes}]);
  const code=[{insert:':smile: \u{1f604}'},{insert:'\n',attributes:{'code-block':true}}];assert.deepEqual(output(code),code);
});
test('native longest sequence, reverse name and ZWJ/tone guards retain source lengths',()=>{
  assert.deepEqual(output('A\u{1f44d}\u{1f3fd}B\u{1f604}'),[{insert:'A'},emoji(':thumbsup::skin-tone-4:'),{insert:'B'},emoji(':smile:')]);
  assert.deepEqual(output('\u{1f604}\u200d\u{1f44d}'),[{insert:'\u{1f604}\u200d\u{1f44d}'}]);
  // Source charAt sees the preceding low surrogate, not a complete codepoint.
  assert.deepEqual(output('\u{1f604}\u{1f3fd}'),[{insert:'\u{1f604}'},emoji(':skin-tone-4:')]);
});
test('actual team tone support joins preceding embeds and preserves supplied tone identity',()=>{
  assert.deepEqual(output([emoji(':thumbsup:'),{insert:':skin-tone-4:!'}]),[emoji(':thumbsup::skin-tone-4:'),{insert:'!'}]);
  assert.deepEqual(output([emoji(':handshake:'),{insert:':skin-tone-3-4:!'}]),[emoji(':handshake::skin-tone-3-4:'),{insert:'!'}]);
  assert.deepEqual(output([emoji(':party:'),{insert:':skin-tone-3-4:!'}]),[emoji(':party:'),{insert:':skin-tone-3-4:!'}]);
  assert.deepEqual(output(':smile::skin-tone-4:'),[emoji(':smile::skin-tone-4:')]);
  assert.equal(converter().appearance(':smile::skin-tone-4:').text,'\u{1f604}');
});
test('skin-tone click sequence follows single and multi source arithmetic',()=>{
  const c=converter();assert.equal(c.nextTone(':thumbsup:').text,':thumbsup::skin-tone-2:');
  assert.equal(c.nextTone(':thumbsup::skin-tone-4:').text,':thumbsup::skin-tone-5:');
  assert.equal(c.nextTone(':thumbsup::skin-tone-6:').text,':thumbsup:');
  assert.equal(c.nextTone(':handshake::skin-tone-3:').text,':handshake::skin-tone-3-4:');
  assert.equal(c.nextTone(':handshake::skin-tone-3-6:').text,':handshake::skin-tone-4-2:');
  assert.equal(c.nextTone(':handshake::skin-tone-6:').text,':handshake:');assert.equal(c.nextTone(':alias:'),null);
});
test('fullwidth locale is distinct, retained attributes and inputs remain immutable',()=>{
  const obs=observations();obs.currentLocale='ja-JP';
  assert.deepEqual(output('\uff1asmile\uff1a',obs),[emoji(':smile:')]);
  assert.deepEqual(output('\uff1asmile\uff1a'),[{insert:'\uff1asmile\uff1a'}]);
  const input=[{insert:':smile:',attributes:{unlink:true}}],before=JSON.stringify(input),catalog=JSON.stringify(obs);
  assert.deepEqual(output(input,obs),[{...emoji(':smile:'),attributes:{unlink:true}}]);
  assert.equal(JSON.stringify(input),before);assert.equal(JSON.stringify(obs),catalog);
  assert.throws(()=>converter({...obs,locale:null}));assert.throws(()=>converter({...obs,names:[...obs.names,'smile']}));
});
test('retained original bm2k English plain vectors reproduce exact changes',()=>{
  const vectors=JSON.parse(fs.readFileSync(__dirname+'/../../../../../crates/super-platinum-core/src/slack_lists/comments/edit_seed/prepare/emoji/vectors.json','utf8'));
  let count=0;
  for(const [i,v]of vectors.results.entries()) {
    if(!v.plain||v.locale!=='en-US'||v.facts.single.includes(''))continue;
    const observed={names:[...v.names,...[2,3,4,5,6].map(n=>'skin-tone-'+n)],unicode:[],unicodeNames:[],locale:'en-US',currentLocale:v.currentLocale??v.locale,
      rows:[...v.names,...[2,3,4,5,6].map(n=>'skin-tone-'+n)].map(name=>row(name,name,v.facts.single.includes(name),v.facts.multiple.includes(name)))};
    const result=converter(observed).transform(v.input);
    assert.deepEqual(clone(result.change.ops),v.change,'original vector '+i);count++;
  }
  assert(count>=30);console.log('Retained source vectors '+count);
});
console.log(JSON.stringify({status:'PASS',groups,actualVendoredDelta:true,native:false}));
module.exports={observations};
