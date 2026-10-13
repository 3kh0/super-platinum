import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
const context={window:{}};
vm.runInNewContext(readFileSync(new URL('../src/desktop/composer_references.js',import.meta.url),'utf8'),context);
const Text=context.window.superPlatinumReferences.Text;
const labels={'<@U1>':'@Alex','<@U2>':'@Alex','<#C1>':'#design','<!subteam^S1>':'@helpers'};

test('labels remain readable while equal names keep separate wire identities',()=>{
  const text=new Text('Hi <@U1>, <@U2> and @Alex — <#C1>',labels);
  assert.equal(text.text,'Hi @Alex, @Alex and @Alex — #design');
  assert.equal(text.wire(),'Hi <@U1>, <@U2> and @Alex — <#C1>');
  text.edit('Hello 🦊 '+text.text);
  assert.equal(text.wire(),'Hello 🦊 Hi <@U1>, <@U2> and @Alex — <#C1>');
});
test('only an edited reference loses its identity',()=>{
  const text=new Text('<@U1> and <@U2>',labels);
  text.edit('@Al and @Alex');
  assert.equal(text.wire(),'@Al and <@U2>');
  text.edit('@Al and @Alex!');
  assert.equal(text.wire(),'@Al and <@U2>!');
});
test('native undo and redo restore the matching reference snapshot',()=>{
  const text=new Text('<@U1> and <@U2>',labels);
  text.edit('@Al and @Alex');
  text.edit('@Alex and @Alex','historyUndo');
  assert.equal(text.wire(),'<@U1> and <@U2>');
  text.edit('@Al and @Alex','historyRedo');
  assert.equal(text.wire(),'@Al and <@U2>');
});
test('formatting round trips preserve IDs and literal code is left alone',()=>{
  const wire='*<@U1>* `@literal <@U2>` ```\n<#C1>\n``` <!subteam^S1> <!here>';
  const text=new Text(wire,labels);
  assert.equal(text.text,'*@Alex* `@literal <@U2>` ```\n<#C1>\n``` @helpers @here');
  assert.equal(text.wire(),wire);
});
test('selection mapping handles Unicode prefixes and references of different lengths',()=>{
  const text=new Text('🦊 <@U1> <#C1>',labels);
  assert.equal(text.toWire(3),3);
  assert.equal(text.toWire(8),8);
  assert.equal(text.toWire(text.text.length),text.wire().length);
  for(const offset of [0,2,3,8,9,text.text.length]) {
    assert.equal(text.toDisplay(text.toWire(offset)),offset);
  }
});
test('unknown and legacy references retain their exact original token',()=>{
  const wire='<@U9> <#C9|private-room> <!subteam^S9|@old-handle> <https://example.com|link>';
  const text=new Text(wire);
  assert.equal(text.text,'@Unknown person #private-room @old-handle <https://example.com|link>');
  assert.equal(text.wire(),wire);
});
test('reference histories are bounded and literal typing never turns into a mention',()=>{
  const text=new Text('',labels);
  for(let i=0;i<120;i++) text.edit(`@Alex ${i}`);
  assert.equal(text.wire(),'@Alex 119');
  assert.equal(text.history.length,100);
});
test('deleting the first of two equal names keeps the second identity',()=>{
  const text=new Text('<@U1> <@U2>',labels);
  text.edit('@Alex','deleteContentBackward',{start:0,end:6});
  assert.equal(text.wire(),'<@U2>');
  text.edit('@Alex @Alex','historyUndo');
  assert.equal(text.wire(),'<@U1> <@U2>');
  text.edit('@Alex','historyRedo');
  assert.equal(text.wire(),'<@U2>');
});
test('pasting the same visible label over a mention makes it literal',()=>{
  const text=new Text('<@U1>',labels);
  text.edit('@Alex','insertFromPaste',{start:0,end:5});
  assert.equal(text.wire(),'@Alex');
});
test('selection hints preserve exactly the untouched occurrences across repeated text edits',()=>{
  const source='<@U1> <@U2> <@U1>';
  for(let start=0;start<=17;start++) for(let end=start;end<=17;end++) {
    for(const replacement of ['', '@Alex', '🦊']) {
      if(start===end && replacement==='') continue;
      const text=new Text(source,labels),before=text.text;
      const expected=text.spans.filter(span=>span.end<=start || span.start>=end)
        .filter(span=>!(start===end && start>span.start && start<span.end))
        .map(span=>span.wire);
      text.edit(before.slice(0,start)+replacement+before.slice(end),'insertText',{start,end});
      assert.deepEqual(Array.from(text.spans,span=>span.wire),Array.from(expected),`${start}:${end}:${replacement}`);
    }
  }
});
test('selection mapping expands partial selections to full wire IDs including emoji labels',()=>{
  const wire='🦊 <@U01234567890> end',text=new Text(wire,{'<@U01234567890>':'@🦊'});
  assert.equal(text.toWire(4),3);
  assert.equal(text.toWire(4,true),18);
  assert.equal(text.toDisplay(8),3);
  assert.equal(text.toDisplay(8,true),6);
  assert.equal(text.toWire(6),18);
  assert.equal(text.toDisplay(18),6);
});
test('attaching selection identity belongs to the native insertion undo step',()=>{
  const text=new Text('Hi @',labels);
  text.edit('Hi @Alex ','insertText',{start:3,end:4});
  text.set('Hi <@U2> ');
  text.edit('Hi @','historyUndo');
  assert.equal(text.wire(),'Hi @');
  text.edit('Hi @Alex ','historyRedo');
  assert.equal(text.wire(),'Hi <@U2> ');
});

const {decodeFragment}=context.window.superPlatinumReferences;
function clipboard(text,start=0,end=text.text.length,workspace='T1') {
  const fragment=text.fragment(start,end);
  return {plain:fragment.text,encoded:JSON.stringify({version:1,workspace,...fragment})};
}
test('clipboard stores equal names by occurrence and leaves literal names unchanged',()=>{
  const source=new Text('<@U1> <@U2> @Alex',labels),clip=clipboard(source);
  assert.equal(clip.plain,'@Alex @Alex @Alex');
  const destination=new Text('🦊 ',labels);
  destination.replaceFragment(3,3,decodeFragment(clip.encoded,clip.plain,'T1'));
  assert.equal(destination.text,'🦊 @Alex @Alex @Alex');
  assert.equal(destination.wire(),'🦊 <@U1> <@U2> @Alex');
});
test('partial reference selection copies exactly the visible substring',()=>{
  const source=new Text('<@U1> <@U2>',labels);
  const clip=clipboard(source,2,11);
  assert.equal(clip.plain,'lex @Alex');
  const destination=new Text('');
  destination.replaceFragment(0,0,decodeFragment(clip.encoded,clip.plain,'T1'));
  assert.equal(destination.wire(),'lex <@U2>');
  assert.equal(clipboard(source,1,4).plain,'Ale');
  assert.equal(JSON.parse(clipboard(source,1,4).encoded).spans.length,0);
});
test('paste replaces only the selected occurrence and keeps neighboring IDs',()=>{
  const source=new Text('<@U2>',labels),clip=clipboard(source);
  const destination=new Text('<@U1> and <@U1>',labels);
  destination.replaceFragment(0,5,decodeFragment(clip.encoded,clip.plain,'T1'));
  assert.equal(destination.wire(),'<@U2> and <@U1>');
  destination.edit('@Alex and @Alex','historyUndo');
  assert.equal(destination.wire(),'<@U1> and <@U1>');
  destination.edit('@Alex and @Alex','historyRedo');
  assert.equal(destination.wire(),'<@U2> and <@U1>');
});
test('references pasted inside code or an escape become literal readable names',()=>{
  const source=new Text('<@U2> <#C1>',labels),clip=clipboard(source);
  for(const prefix of ['`before ','```\nbefore ']) {
    const destination=new Text(prefix);
    destination.replaceFragment(prefix.length,prefix.length,decodeFragment(clip.encoded,clip.plain,'T1'));
    assert.equal(destination.wire(),prefix+'@Alex #design');
  }
  const destination=new Text('\\');
  destination.replaceFragment(1,1,decodeFragment(clip.encoded,clip.plain,'T1'));
  assert.equal(destination.wire(),'\\@Alex <#C1>');
});
test('clipboard import refuses another workspace, mismatched text, or malformed spans',()=>{
  const clip=clipboard(new Text('<@U1>',labels));
  assert.equal(decodeFragment(clip.encoded,clip.plain,'T2'),null);
  assert.equal(decodeFragment(clip.encoded,clip.plain,''),null);
  assert.equal(decodeFragment(clip.encoded,'@Someone else','T1'),null);
  for(const mutation of [
    d=>d.version=2,
    d=>d.spans[0].start=-1,
    d=>d.spans[0].start=0.5,
    d=>d.spans[0].end=100,
    d=>d.spans[0].end=0,
    d=>d.spans.push({...d.spans[0]}),
    d=>d.spans[0].wire='<https://evil.example>',
    d=>d.spans[0].wire='<@U1> <!everyone>',
    d=>d.spans[0].wire='<#C1>',
    d=>d.spans=null,
  ]) {
    const data=JSON.parse(clip.encoded);mutation(data);
    assert.equal(decodeFragment(JSON.stringify(data),clip.plain,'T1'),null);
  }
  for(const malformed of ['null','[]','{','x'.repeat(1048577)]) assert.equal(decodeFragment(malformed,clip.plain,'T1'),null);
});
test('legacy tokens, groups and broadcasts round trip without parsing clipboard HTML',()=>{
  const source=new Text('<#C1|old-name> <!subteam^S1> <!here> <https://example.invalid|link>',labels);
  const clip=clipboard(source),destination=new Text('');
  destination.replaceFragment(0,0,decodeFragment(clip.encoded,clip.plain,'T1'));
  assert.equal(destination.wire(),source.wire());
  assert.equal(destination.text,source.text);
});
