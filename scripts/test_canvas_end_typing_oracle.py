"""Portable typed receipts with actual original-runtime Automerge documents."""
import base64
import copy
from functools import lru_cache
import html
import json
from pathlib import Path
import subprocess
import unittest

import canvas_end_typing_oracle as T
import canvas_split_oracle as S
import test_canvas_end_split_oracle as E
import test_canvas_end_reuse_oracle as R
from test_canvas_split_oracle import f, b64, body, blocks, replace, remove, merge, inventory, acknowledgement, native, SESSION, CHILD, TITLE


def literals(corpus, typing):
    if typing == "erase": return ["x", ""]
    return ["New", "New draftx", "New draft", "New paragraph"] if corpus == "ascii" else ["New", "New 日本😀 draftx", "New 日本😀 draft", "New 日本😀 paragraph"]


@lru_cache(maxsize=16)
def histories(mode="end", corpus="ascii", typing="correction", updates_override=None):
    # Test builder invokes original CRDT updateText, independently of oracle
    # mutation_states. All final submitted documents are real Automerge bytes.
    script = r'''
import {A,newCrdt} from './tmp/canvas-structure-prep/rich-runtime.mjs';
import fs from 'node:fs';
const p=JSON.parse(fs.readFileSync(0,'utf8')),actor=p.session+'00000000';
const item=newCrdt({id:p.mode==='reuse'?'empty-final':p.child,rtml:'',sequence:p.mode==='reuse'?1000:0,dirty:p.mode!=='reuse',actor});
const source=Buffer.from(item.crdt.encodedAutomergeDoc).toString('base64');
const base=A.getHeads(item.crdt.automergeDoc)[0];
for(const text of p.updates){item.state.rtml=text;await item.crdt.updateText();}
const doc=item.crdt.automergeDoc,final=Buffer.from(item.crdt.encodedAutomergeDoc).toString('base64'),head=A.getHeads(doc)[0];
const continued=A.change(A.clone(doc,{actor:'cccccccccccc00000000'}),d=>A.updateText(d,['text'],p.continued+'\ufeff'));
console.log(JSON.stringify({source,base,final,head,continued:Buffer.from(A.save(continued)).toString('base64'),continuedHead:A.getHeads(continued)[0]}));
'''
    result = subprocess.run(["node", "--input-type=module", "-e", script], input=json.dumps(dict(mode=mode, child=CHILD, session=SESSION,
               updates=literals(corpus, typing) if updates_override is None else updates_override, continued="Continued paragraph" if corpus == "ascii" else "Continued 日本😀 paragraph")),
               cwd=Path(__file__).resolve().parent.parent, text=True, encoding="utf-8", capture_output=True, timeout=30)
    if result.returncode: raise RuntimeError(result.stderr[:1000])
    return json.loads(result.stdout)


def source_and_request(mode="end", corpus="ascii", typing="correction"):
    source = R.initial(corpus) if mode == "reuse" else E.initial(corpus); h = histories(mode, corpus, typing)
    if mode == "reuse":
        row = source["sections"][-1]; raw = base64.b64decode(row["raw"]); crdt = S.field(S.wire(raw), 38, 2)
        crdt = replace(replace(replace(crdt, 5, base64.b64decode(h["source"])), 6, h["base"]), 8, h["base"])
        row["raw"] = b64(replace(raw, 38, crdt))
    write, sent = R.request(source) if mode == "reuse" else E.request(source)
    tail = sent[1]; current = S.field(S.wire(tail), 38, 2)
    current = replace(replace(replace(current, 5, base64.b64decode(h["final"])), 8, h["base"]), 12, h["head"])
    current = remove(current, 6)
    heads = [h["base"], h["head"], h["base"]] if mode == "reuse" else [h["head"]]
    # Preserve encoder's ascending tag order and repeated-head order.
    current = b"".join(span for n, _, _, span in sorted(S.wire(current) + [(6, 2, v.encode(), f(6, v)) for v in heads], key=lambda x:x[0]))
    if mode == "reuse": tail = f(1, "empty-final") + f(2, 1000) + f(9, 0) + f(12, body(literals(corpus, typing)[-1], False)) + remove(remove(tail, 1), 2)
    else: tail = replace(tail, 12, body(literals(corpus, typing)[-1], False))
    tail = replace(tail, 38, current)
    tail = b"".join(span for _, _, _, span in sorted(S.wire(tail), key=lambda x:x[0]))
    if mode == "end":
        # Actual native prepared request keeps creation metadata, then appends
        # the typed Type/Content replacement; title is serialized first.
        parsed = S.wire(tail)
        tail = b"".join(span for tag, _, _, span in parsed if tag not in (9, 12)) + b"".join(span for tag, _, _, span in parsed if tag in (9, 12))
    sent[1] = tail; write["documentData"] = b64(f(14, TITLE) + f(1, sent[0]) + f(1, tail))
    return source, write, sent


def checkpoint(source, write, mode="end", corpus="ascii", typing="correction"):
    c = E.expected(corpus)
    return dict(version=6, scope=S.SCOPE.copy(), thread="thread", document="document", target="paragraph", session=SESSION, sequence=1,
                action={"EndTyping":dict(child=write["child"], position="aag" if mode == "reuse" else "aad:temp", offset_utf16=c.offset,
                       reused=mode == "reuse", child_updates=literals(corpus, typing))}, draft=c.wire,
                sections=[list(base64.b64decode(r["raw"])) for r in source["sections"]], request=list(base64.b64decode(write["documentData"])))


def typed_witness(generation, corpus="ascii", typing="correction"):
    result = E.witness(generation, corpus); before = ""; events = []; c = E.expected(corpus)
    for i, after in enumerate(literals(corpus, typing)):
        if typing == "correction" and i == 3:
            kind, data, start, stop = "insertText", "paragraph", len(before.encode('utf-16-le'))//2-5, len(before.encode('utf-16-le'))//2
            selection = (start, stop)
        elif (typing == "erase" and i == 1) or (typing == "correction" and i == 2):
            kind, data, start, stop = "deleteContentBackward", None, len(before.encode('utf-16-le'))//2-1, len(before.encode('utf-16-le'))//2
            selection = (stop, stop)
        else:
            kind, data, start, stop = "insertText", after[len(before):], len(before.encode('utf-16-le'))//2, len(before.encode('utf-16-le'))//2
            selection = (start, stop)
        canonical = blocks(c.seed, after)
        if not after: canonical[0]["elements"][1]["elements"] = []
        child_html = '<span>' + (html.escape(after) if after else '<br>') + '</span>'
        rendered = '<div class="rich-editor-block"><div class="rich-editor-section"><span>' + html.escape(c.seed) + '</span></div><div class="rich-editor-section">' + child_html + '</div></div>'
        events.append(dict(inputType=kind, data=data, selectionStart=selection[0], selectionEnd=selection[1], start=start, end=stop,
                           beforeText=before, afterText=after, beforeRevision=i+1, revision=i+2, caret=start+len((data or '').encode('utf-16-le'))//2,
                           beforeInputTrusted=True, inputTrusted=True, after=canonical, rawAfter=copy.deepcopy(canonical) if after else blocks(c.seed, '\n'),
                           html=rendered, placeholder=None if after else dict(blockCount=1, sectionCount=2, childHtml=child_html, spanDepth=1, caretOffset=0)))
        before = after
    result['typing'] = {k:copy.deepcopy(events[-1][k]) for k in ('revision','after','rawAfter','html','placeholder')}
    result['typing']['events'] = events
    return result


def authored(sent, source, mode, corpus, typing, continued=False):
    rows = R.authored(sent, source, corpus, continued) if mode == "reuse" else E.authored_rows(sent, base64.b64decode(next(r['raw'] for r in source['sections'] if r['id']==('paragraph' if not continued else CHILD))), corpus, continued)
    result = []
    for row in rows:
        parent = S.text(S.wire(row),1) == 'paragraph'
        content = E.expected(corpus).wire if parent else ('Continued paragraph' if corpus=='ascii' else 'Continued 日本😀 paragraph') if continued else literals(corpus,typing)[-1]
        result.append(replace(row,12,body(content,parent)))
    return result


def receipt(scenario="success", corpus="ascii", mode="end", typing="correction"):
    packet = R.receipt(scenario, corpus) if mode == 'reuse' else E.receipt(scenario, corpus)
    source, write, sent = source_and_request(mode, corpus, typing); child=write['child']; c=E.expected(corpus)
    pair = authored(sent,source,mode,corpus,typing)
    originals={r['id']:base64.b64decode(r['raw']) for r in source['sections']}
    def inv(pair): return inventory([originals['title'],originals['neighbor'],*pair] if mode=='reuse' else [originals['title'],*pair,originals['neighbor']])
    readback=inv(pair); h=histories(mode,corpus,typing)
    crdt=remove(S.field(S.wire(pair[1]),38,2),4)
    crdt=replace(replace(crdt,5,base64.b64decode(h['continued'])),12,h['continuedHead'])+f(6,h['continuedHead'])
    continuation='Continued paragraph' if corpus=='ascii' else 'Continued 日本😀 paragraph'
    row=f(1,child)+f(2,2000 if mode=='reuse' else 1000)+f(9,0)+f(12,body(continuation,False))+f(13,b'')+f(19,1)+f(20,1)+f(33,'c'*12+'-0')+f(38,crdt)
    second=dict(kind='text',parent=child,session='c'*12,sequence=1,formTitle='Fixture title',httpFactoryExecuted=False,documentData=b64(f(1,row)+f(14,TITLE)))
    edited=authored([row],readback,mode,corpus,typing,True)[0]; final=inv([pair[0],edited])
    held=dict(scope=S.SCOPE.copy(),receipt=dict(operation='whole-typed-pair',checkpoint=checkpoint(source,write,mode,corpus,typing)))
    original=dict(packet['original'],typing=typing,fresh=source,visible=source)
    packet.update(typing=typing,original=copy.deepcopy(original),cancelledWitness=typed_witness(1,corpus,typing),savedWitness=typed_witness(2,corpus,typing))
    for i,stage in enumerate(packet['stages']):
        if i==0: stage['stage']='typed-child'
        name=stage['stage']; proof=copy.deepcopy(original)
        if i>=2:
            fresh=copy.deepcopy(source)
            if scenario=='stale':
                target=next(r for r in fresh['sections'] if r['id']==('empty-final' if mode=='reuse' else 'paragraph'))
                target.update(sequence=3000,raw=b64(replace(base64.b64decode(target['raw']),2,3000)))
            proof.update(fresh=fresh,preflights=[dict(source=source,fresh=fresh,outcome='Changed' if scenario=='stale' else 'prepared')])
            if scenario!='stale':
                proof.update(writes=[write],reservations=1)
                if i>=3:
                    proof['acknowledgements']=[acknowledgement(pair,scenario=='partial')]
                    if scenario=='success': proof.update(fresh=readback,visible=readback,readbacks=[readback],read=2)
                if i>=5:
                    proof['preflights'].append(dict(source=readback,fresh=readback,outcome='prepared',kind='text'));proof['writes'].append(second)
                    if i==6: proof.update(fresh=final,visible=final,readbacks=[readback,final],read=3,acknowledgements=[acknowledgement(pair),acknowledgement([edited],continued=True)])
        pending=name in ('saving','continued-saving'); uncertain=name=='uncertain'
        proof.update(pending=pending,stale=pending or uncertain,operation=dict(serial=1,stage='Uncertain' if uncertain else 'Saving') if pending or uncertain else None)
        stage['fixture']=copy.deepcopy(proof);stage['journal']=copy.deepcopy(held if name in ('saving','uncertain') else packet['initialJournal'])
        geometry=stage['geometry']; editor=geometry['native']
        if editor is not None:
            if name in ('typed-child','saving','refused','uncertain'):
                w=packet['cancelledWitness'] if i==0 else packet['savedWitness'];typed=w['typing']
                editor.update(generation=w['generation'],visit=w['visit'],blocks=typed['after'],rawBlocks=typed['rawAfter'],placeholder=typed['placeholder'],typing=typed,text=c.seed+literals(corpus,typing)[-1])
            else:
                content=literals(corpus,typing)[-1] if name=='child-editing' else continuation
                snapshot=blocks(content)
                if not content:snapshot[0]['elements'][0]['elements']=[]
                editor.update(blocks=snapshot,rawBlocks=copy.deepcopy(snapshot),placeholder=None,typing=None,text=content)
        else:
            items=[dict(id='paragraph',text=c.seed)]
            if mode=='reuse' or scenario=='success' and i>=3: items.append(dict(id=child,text=continuation if i==6 else literals(corpus,typing)[-1] if scenario=='success' and i>=3 else ''))
            neighbor=dict(id='neighbor',text='Another paragraph');geometry['paragraphs']=[neighbor,*items] if mode=='reuse' else [*items,neighbor]
    packet.update(fixture=packet['stages'][-1]['fixture'],layout=packet['stages'][-1]['geometry'],finalJournal=packet['stages'][-1]['journal'])
    return packet


class TypedOracle(unittest.TestCase):
    def test_singular_field_wire_order_is_not_pair_or_head_order(self):
        for mode in ('end','reuse'):
            source,write,sent=source_and_request(mode);expected=T.config(mode=mode);rows=T.inventory(source,expected)
            self.assertEqual([f[0] for f in S.decoded(write['documentData'])],[14,1,1])
            if mode=='end':self.assertEqual([f[0] for f in S.wire(sent[1])][-2:],[9,12])
            for title_index in range(3):
                parts=[f(1,b''.join(span for _,_,_,span in reversed(S.wire(row)))) for row in sent]
                parts.insert(title_index,f(14,TITLE))
                T.typed_write(dict(write,documentData=b64(b''.join(parts))),rows,expected)
            for parts in ([f(14,TITLE),f(1,sent[1]),f(1,sent[0])],
                          [f(14,TITLE),f(1,sent[0]),f(1,sent[0])],
                          [f(14,TITLE),f(14,TITLE),f(1,sent[0]),f(1,sent[1])],
                          [f(14,TITLE),f(1,sent[0]),f(1,sent[1]),f(99,b'unknown')],
                          [f(14,TITLE),f(1,sent[0]),f(1,sent[1]+f(9,0))]):
                with self.assertRaises(RuntimeError):T.typed_write(dict(write,documentData=b64(b''.join(parts))),rows,expected)

    def test_actual_retained_original_replacement_vectors_match_pinned_patch_sequence(self):
        vectors=json.loads((Path(__file__).resolve().parent.parent/'tmp/canvas-end-typing-20261023/reference/typing-replacement-patches-result.json').read_text())['vectors']
        for vector in vectors:
            child=S.messages(S.decoded(vector['wireBase64']),1)[1]
            current=S.wire(S.field(child,38,2)); report=T.decode_history(S.field(current,5,2))[0]
            updates=literals(vector['corpus'],'correction')
            self.assertEqual([c['text'] for c in report['changes'][1:]],[v+'\ufeff' for v in T.mutation_states(updates)])
            self.assertEqual([c['patches'] for c in report['changes'][1:]],[[p] for p in T.mutation_patches(updates)])

    def test_reused_raw_history_and_claimed_base_head_must_correspond(self):
        source,write,sent=source_and_request('reuse');expected=T.config(mode='reuse')
        changed=copy.deepcopy(source);row=changed['sections'][-1];raw=base64.b64decode(row['raw'])
        crdt=replace(S.field(S.wire(raw),38,2),8,'f'*64);row['raw']=b64(replace(raw,38,crdt))
        current=replace(S.field(S.wire(sent[1]),38,2),8,'f'*64);tail=replace(sent[1],38,current)
        with self.assertRaisesRegex(RuntimeError,'metadata heads'):
            T.typed_write(dict(write,documentData=b64(f(1,sent[0])+f(1,tail)+f(14,TITLE))),T.inventory(changed,expected),expected)

    def test_real_final_equivalent_history_cannot_squash_or_reorder_edits(self):
        source,write,sent=source_and_request();rows=T.inventory(source,T.config())
        for updates in [('New paragraph',),('New','New draft','New draftx','New paragraph')]:
            h=histories(updates_override=updates);crdt=S.field(S.wire(sent[1]),38,2)
            for tag,value in ((5,base64.b64decode(h['final'])),(6,h['head']),(8,h['base']),(12,h['head'])):crdt=replace(crdt,tag,value)
            crdt=b''.join(span for _,_,_,span in sorted(S.wire(crdt),key=lambda x:x[0]))
            tail=replace(sent[1],38,crdt)
            bad=dict(write,documentData=b64(f(1,sent[0])+f(1,tail)+f(14,TITLE)))
            with self.subTest(updates=updates),self.assertRaisesRegex(RuntimeError,'ordered history'):T.typed_write(bad,rows,T.config())

    def test_actual_empty_bootstrap_cannot_hide_type_then_erase(self):
        source,write,sent=source_and_request(typing='erase');rows=T.inventory(source,T.config(typing='erase'))
        h=histories(typing='erase',updates_override=());crdt=S.field(S.wire(sent[1]),38,2)
        for tag,value in ((5,base64.b64decode(h['final'])),(6,h['head']),(8,h['base']),(12,h['head'])):crdt=replace(crdt,tag,value)
        crdt=b''.join(span for _,_,_,span in sorted(S.wire(crdt),key=lambda x:x[0]))
        tail=replace(sent[1],38,crdt)
        with self.assertRaisesRegex(RuntimeError,'effective edits'):T.typed_write(dict(write,documentData=b64(f(1,sent[0])+f(1,tail)+f(14,TITLE))),rows,T.config(typing='erase'))

    def test_complete_lifecycle_both_modes_corpora_and_edit_sequences(self):
        for mode in ('end','reuse'):
            for corpus in ('ascii','unicode'):
                for typing in ('correction','erase'):
                    for scenario in ('success','stale','partial'):
                        with self.subTest(mode=mode,corpus=corpus,typing=typing,scenario=scenario): T.validate(receipt(scenario,corpus,mode,typing),corpus,mode,typing)

    def test_actual_replacement_history_has_all_four_original_patches(self):
        for mode in ('end','reuse'):
            h=histories(mode); decoded=T.decode_history(base64.b64decode(h['final']))[0]
            self.assertEqual([c['text'] for c in decoded['changes']],['\ufeff','New\ufeff','New draftx\ufeff','New draft\ufeff','New paragdraft\ufeff','New paragraft\ufeff','New paragra\ufeff','New paragraph\ufeff'])

    def test_native_order_trust_utf16_range_and_dom_are_independent(self):
        for key,value in [('start',6),('beforeRevision',1),('beforeInputTrusted',False),('afterText','New 日本 draftx'),('data',' draftx')]:
            w=typed_witness(1,'unicode');w['typing']['events'][1][key]=value
            with self.subTest(key=key),self.assertRaises(RuntimeError):T.witness(w,T.config('unicode'))
        w=typed_witness(1,typing='erase');w['typing']['events'][-1]['html']=w['typing']['events'][-1]['html'].replace('<br>','<br><br>')
        with self.assertRaises(RuntimeError):T.witness(w,T.config(typing='erase'))

    def test_durable_updates_cannot_be_collapsed_to_final_or_wire_offset(self):
        source,write,_=source_and_request('reuse','unicode');c=checkpoint(source,write,'reuse','unicode');expected=T.config('unicode','reuse')
        self.assertEqual(c['action']['EndTyping']['offset_utf16'],11)
        for key,value in [('child_updates',['New 日本😀 paragraph']),('offset_utf16',15),('reused',False)]:
            changed=copy.deepcopy(c);changed['action']['EndTyping'][key]=value
            with self.assertRaises(RuntimeError):T.journal(dict(scope=S.SCOPE,receipt=dict(operation='test',checkpoint=changed)),expected,write,source)

    def test_reuse_processed_heads_preserve_duplicate_base_order(self):
        source,write,sent=source_and_request('reuse');rows=T.inventory(source,T.config(mode='reuse'))
        crdt=S.field(S.wire(sent[1]),38,2);h=histories('reuse')
        for heads in ([h['base'],h['head']],[h['head'],h['base'],h['base']]):
            altered=remove(crdt,6)+b''.join(f(6,x) for x in heads);tail=replace(sent[1],38,altered)
            with self.assertRaises(RuntimeError):T.typed_write(dict(write,documentData=b64(f(1,sent[0])+f(1,tail)+f(14,TITLE))),rows,T.config(mode='reuse'))

    def test_partial_ack_and_stale_draft_cannot_be_called_accepted(self):
        for scenario in ('partial','stale'):
            r=receipt(scenario);r['stages'][-1]['fixture']['readbacks']=[r['original']['fresh']]
            with self.assertRaises(RuntimeError):T.validate(r)

    def test_complete_ack_readback_and_checkpoint_custody_are_required(self):
        for target in ('ack','readback','journal'):
            r=receipt()
            if target=='ack':r['stages'][3]['fixture']['acknowledgements'][0]['accepted']=False
            elif target=='readback':r['stages'][3]['fixture']['readbacks'][0]['sections'].pop()
            else:r['stages'][2]['journal']['receipt']['checkpoint']['request']=[]
            with self.subTest(target=target),self.assertRaises(RuntimeError):T.validate(r)


if __name__ == '__main__':unittest.main()
