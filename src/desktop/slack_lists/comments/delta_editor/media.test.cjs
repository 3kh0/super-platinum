// Actual codec/blots with a minimal DOM: this checks presentation ownership,
// not native WebView loading or Quill's MutationObserver behavior.
const fs = require('node:fs'), vm = require('node:vm');
const assert = require('node:assert/strict'), {test} = require('node:test');
function harness() {
  const document = {createElement: tag => new Node(tag)};
  class Node {
    constructor(tag = 'div') { this.tagName=tag; this.children=[]; this.attrs={}; this.ownerDocument=document; }
    appendChild(child) { child.parentNode=this; this.children.push(child); return child; }
    contains(child) { return child===this || this.children.some(n=>n.contains(child)); }
    querySelectorAll(tag) { return this.children.flatMap(n=>[...(n.tagName===tag?[n]:[]),...n.querySelectorAll(tag)]); }
    setAttribute(key,value) { this.attrs[key]=String(value); }
    getAttribute(key) { return this.attrs[key]??null; }
  }
  class Blot { static create() { return new Node(this.tagName); } }
  const formats={};
  const Quill={import:()=>Blot,register:rows=>Object.assign(formats,rows)};
  const context=vm.createContext({crypto:require('node:crypto').webcrypto});
  vm.runInContext(fs.readFileSync(__dirname+'/blots.js','utf8'),context);
  const api=context.listCommentDeltaBlots(Quill), Emoji=formats['formats/slackemoji'];
  const uri='http://super-platinum-media.emoji/0123456789abcdef0123456789abcdef';
  const rows=[{name:':custom:',text:':custom:',media:uri+'?v=4'}];
  const codec=api.codec(rows), root=new Node();
  const semantic=[{insert:{slackemoji:{text:':custom:'}}},{insert:'\n'}];
  const ops=codec.encode(semantic), token=ops[0].insert.slackemoji;
  const node=root.appendChild(Emoji.create(token)), image=node.children[0];
  return {api,codec,root,Emoji,Node,uri,rows,semantic,ops,token,node,image};
}
test('refresh changes only issued image URL and preserves exact token and semantic Delta',()=>{
  const h=harness(), before=JSON.stringify(h.codec.decode(h.ops));
  assert.equal(h.codec.refreshMedia(h.root,'5'),true);
  assert.equal(h.image.getAttribute('src'),h.uri+'?v=5');
  assert.equal(h.Emoji.value(h.node),h.token);
  assert.equal(JSON.stringify(h.codec.decode(h.ops)),before);
  assert.equal(h.node.getAttribute('aria-label'),':custom:');
  assert.equal(h.image.getAttribute('alt'),':custom:');
  assert.equal(h.codec.refreshMedia(h.root,'5'),true);
});
test('history recreation and later insertion use latest presentation epoch',()=>{
  const h=harness();h.codec.refreshMedia(h.root,'9');
  assert.equal(h.Emoji.create(h.token).children[0].getAttribute('src'),h.uri+'?v=9');
  const plan=h.codec.prepare([{insert:{slackemoji:{text:':second:'}}}],
    [{name:':second:',text:'second',media:h.uri+'?v=2'}]);
  plan.commit();
  assert.equal(h.Emoji.create(plan.ops[0].insert.slackemoji).children[0].getAttribute('src'),h.uri+'?v=9');
});
test('epoch is canonical u64 and cannot regress or lose precision',()=>{
  const h=harness();
  for(const bad of [5,'05','-1','1.5','1\n','18446744073709551616',null])
    assert.equal(h.codec.refreshMedia(h.root,bad),false);
  assert.equal(h.image.getAttribute('src'),h.uri+'?v=4');
  assert.equal(h.codec.refreshMedia(h.root,'2'),true);
  assert.equal(h.image.getAttribute('src'),h.uri+'?v=4');
  assert.equal(h.codec.refreshMedia(h.root,'18446744073709551615'),true);
  assert.equal(h.image.getAttribute('src'),h.uri+'?v=18446744073709551615');
  assert.equal(h.codec.refreshMedia(h.root,'9007199254740992'),false);
});
test('cloned markup and another codec cannot acquire original image authority',()=>{
  const h=harness(), foreign=h.api.codec(h.rows);
  assert.equal(foreign.refreshMedia(h.root,'8'),true);
  assert.equal(h.image.getAttribute('src'),h.uri+'?v=4');
  const clone=h.root.appendChild(new h.Node('SPAN')), image=clone.appendChild(new h.Node('img'));
  image.setAttribute('src',h.uri+'?v=4');clone.setAttribute('aria-label',':custom:');
  h.codec.refreshMedia(h.root,'8');
  assert.equal(image.getAttribute('src'),h.uri+'?v=4');
  assert.equal(h.image.getAttribute('src'),h.uri+'?v=8');
});
test('detached, transplanted and externally changed images are never rewritten',()=>{
  const h=harness(), other=new h.Node();
  h.codec.refreshMedia(other,'7');
  assert.equal(h.image.getAttribute('src'),h.uri+'?v=4');
  h.node.children=[];h.root.appendChild(h.image);
  h.codec.refreshMedia(h.root,'8');
  assert.equal(h.image.getAttribute('src'),h.uri+'?v=4');
  h.root.children=[h.node];h.node.appendChild(h.image);
  h.image.setAttribute('src','https://untrusted.invalid/image');
  h.codec.refreshMedia(h.root,'9');
  assert.equal(h.image.getAttribute('src'),'https://untrusted.invalid/image');
});
test('presentation refresh does not retire a staged semantic insertion plan',()=>{
  const h=harness(), plan=h.codec.prepare(h.semantic);
  h.codec.refreshMedia(h.root,'10');
  assert.doesNotThrow(()=>plan.commit());
  assert.equal(plan.ops[0].insert.slackemoji,h.token);
  assert.equal(JSON.stringify(h.codec.decode(plan.ops)),JSON.stringify(h.semantic));
});
test('tone lookup requires original node and codec and returns a defensive value',()=>{
  const h=harness(), value=h.codec.emojiAt(h.node);
  assert.equal(value.text,':custom:');value.text='forged';
  assert.equal(h.codec.emojiAt(h.node).text,':custom:');
  assert.equal(h.api.codec(h.rows).emojiAt(h.node),null);
  const clone=new h.Node('SPAN');clone.attrs={...h.node.attrs};
  assert.equal(h.codec.emojiAt(clone),null);
  assert.equal(h.codec.emojiAt(h.image),null);
});
