"""Original-history positives and adversarial native/v8 custody regressions."""
import base64
import copy
import json
from pathlib import Path
import unittest
import canvas_middle_typing_oracle as O
import canvas_middle_typing_wire as W
import canvas_middle_typing_recovery_oracle as R
import canvas_split_oracle as S
import test_canvas_middle_typing_fixtures as F


class MiddleTyping(unittest.TestCase):
    def test_twelve_original_histories_and_native_both_member_continuations(self):
        for parent in ('bootstrap','retained'):
            for corpus in ('ascii','unicode'):
                for typing in ('correction','erase','restore'):
                    with self.subTest(parent=parent,corpus=corpus,typing=typing):
                        p=F.receipt('success',corpus,typing,parent)
                        O.validate(p,corpus,typing,parent)
                        source,write,sent=F.source_request(corpus,typing,parent);e=W.config(corpus,typing,parent)
                        pc,cc=(S.wire(S.field(S.wire(r),38,2)) for r in sent)
                        lineage=S.wire(S.field(pc,10,2));before=W.T.decode_history(S.field(lineage,7,2))[0]
                        self.assertEqual([v.decode() for tag,_,v,_ in pc if tag==6],before['heads']+[S.text(pc,12)]+before['heads'])
                        history=W.T.decode_history(S.field(cc,5,2))[0]['changes']
                        self.assertEqual(history[0]['text'],'\ufeff')
                        self.assertEqual(history[1]['text'],F.wiretext(F.literals(corpus,typing).tail)+'\ufeff')
                        self.assertEqual([v.decode() for tag,_,v,_ in cc if tag==6],[history[-1]['hash']])
                        self.assertNotEqual(history[1]['hash'],history[-1]['hash'])
                        self.assertEqual(len(p['stages'][-1]['fixture']['writes']),3)

    def test_stale_partial_and_all_inert_recovery_phases(self):
        for parent in ('bootstrap','retained'):
            for scenario in ('stale','partial'):O.validate(F.receipt(scenario,'unicode','erase',parent),'unicode','erase',parent)
            for typing in ('correction','erase','restore'):
                for phase in R.STAGES:
                    with self.subTest(parent=parent,typing=typing,phase=phase):R.validate(phase,*F.recovery(phase,'unicode',typing,parent),'unicode',typing,parent)

    def test_wrong_projection_initial_tail_and_lineage_are_rejected(self):
        source,write,sent=F.source_request('unicode','restore','retained');e=W.config('unicode','restore','retained');original=W.inventory(source,e)['paragraph']
        pc,cc=(S.field(S.wire(r),38,2) for r in sent);lineage=S.field(S.wire(pc),10,2)
        childhistory=W.T.decode_history(S.field(S.wire(cc),5,2))[0]['changes']
        cases=[(F.F.replace(sent[0],38,F.F.remove(pc,6)+F.F.f(6,S.text(S.wire(pc),12))),sent[1]),
               (sent[0],F.F.replace(sent[1],38,cc+F.F.f(6,childhistory[1]['hash']))),
               (F.F.replace(sent[0],38,F.F.replace(pc,10,F.F.replace(lineage,4,11))),sent[1]),
               (F.F.replace(sent[0],38,F.F.replace(pc,10,F.F.replace(lineage,7,S.field(S.wire(cc),5,2)))),sent[1]),
               (sent[0],F.F.replace(sent[1],38,F.F.replace(cc,5,S.field(S.wire(pc),5,2))))]
        for parent,child in cases:
            mutated=dict(write,documentData=F.F.b64(F.F.f(14,F.F.TITLE)+F.F.f(1,parent)+F.F.f(1,child)))
            with self.assertRaises(RuntimeError):W.write(mutated,original,e)
        # Real initial-tail document is valid Automerge with matching restored final body,
        # but lacks the append/backspace history and final head.
        v=F.vector('unicode','restore','retained');initial=base64.b64decode(v['initialTailSession'])
        bad=F.F.replace(sent[1],38,initial)
        with self.assertRaises(RuntimeError):W.write(dict(write,documentData=F.F.b64(F.F.f(14,F.F.TITLE)+F.F.f(1,sent[0])+F.F.f(1,bad))),original,e)

    def test_closed_native_editor_owner_stays_stable_through_save(self):
        path=F.ROOT/'tmp/canvas-middle-typing-20261023/integration/bootstrap-ascii-correction-a/canvas-structure.json'
        closed=json.loads(path.read_text(encoding='utf-8'))
        self.assertIs(closed['ok'],False)  # Preserve failed original harness result.
        self.assertIs(closed['native']['ok'],True)
        self.assertEqual(closed['auditErrors'],[])
        packet=closed['native'];O.validate(packet)
        self.assertEqual([(packet['stages'][a]['geometry']['native']['generation'],packet['stages'][b]['geometry']['native']['generation']) for a,b in [(4,5),(7,8)]],[('3','3'),('4','4')])
        for stage,owner in [(4,'2'),(5,'5'),(7,'3'),(8,'5')]:
            bad=copy.deepcopy(packet);bad['stages'][stage]['geometry']['native']['generation']=owner
            with self.subTest(stage=stage),self.assertRaises(RuntimeError):O.validate(bad)

    def test_continuations_share_session_and_advance_exact_trace(self):
        p=F.receipt();O.validate(p)
        writes=p['fixture']['writes'];self.assertEqual(writes[1]['session'],writes[2]['session'])
        self.assertEqual([w['sequence'] for w in writes[1:]],[1,2])
        source=W.inventory(p['fixture']['readbacks'][1],W.config(),p['original']['fresh'],writes[0]['child'],True)['paragraph']
        for kind in ('sequence','session','trace'):
            bad=copy.deepcopy(writes[2])
            if kind=='sequence':bad['sequence']=1
            elif kind=='session':bad['session']='d'*12
            else:
                data=S.decoded(bad['documentData']);row=S.raw(S.messages(data,1)[0])
                bad['documentData']=F.F.b64(F.F.f(14,F.F.TITLE)+F.F.f(1,F.F.replace(row,33,writes[1]['session']+'-0')))
            with self.subTest(kind=kind),self.assertRaises(RuntimeError):O.continuation(bad,'paragraph',source,'Keep this edited',2,writes[1]['session'])

    def test_unicode_reference_and_fixture_are_literal_utf8_and_offsets_11_15(self):
        e=W.config('unicode');literal=F.literals('unicode','correction')
        self.assertEqual(e.seed,'Caf\u00e9 & \u65e5\u672c\U0001f600')
        self.assertEqual(e.tail,'Second \u65e5\u672c\U0001f600 & paragraph')
        self.assertEqual((e.offset,e.wire_offset),(11,15))
        self.assertEqual((F.utf16(literal.left),F.utf16(F.wiretext(literal.left))),(11,15))
        self.assertEqual(e.full,literal.full)
        for parent in ('bootstrap','retained'):
            v=F.vector('unicode','correction',parent)
            self.assertEqual(v['full'],'Caf\u00e9 &amp; \u65e5\u672c\U0001f600Second \u65e5\u672c\U0001f600 &amp; paragraph')
            self.assertEqual(v['left'],e.left_wire)
            self.assertEqual(v['tail'],e.tail_wire)
            self.assertEqual(v['states'],list(e.reports))

    def test_source_style_and_hidden_document_mutation_are_rejected(self):
        source,_,_=F.source_request();e=W.config()
        source['sections'][1]['raw']=F.F.b64(F.F.replace(base64.b64decode(source['sections'][1]['raw']),10,1))
        with self.assertRaises(RuntimeError):W.inventory(source,e)
        _,_,sent=F.source_request();crdt=S.wire(S.field(S.wire(sent[1]),38,2))
        changes=copy.deepcopy(W.T.decode_history(S.field(crdt,5,2))[0]['changes'])
        changes[-1]['ops'].append(dict(action='makeMap',obj='_root',key='hidden',pred=[]))
        with self.assertRaisesRegex(RuntimeError,'document structure'):W.chain(changes,[],F.SESSION+'00000000')

    def test_native_event_seal_cannot_substitute_tail_or_cross_parent(self):
        p=F.receipt('success','unicode','correction','bootstrap')
        for kind in ('initial-tail','offset','before','range','trust','revision','parent-dom','placeholder','final'):
            bad=copy.deepcopy(p);w=bad['savedWitness'];event=w['typing']['events'][0]
            if kind=='initial-tail':w['after'][0]['elements'][1]['elements'][0]['text']='New'
            elif kind=='offset':w['offset']=15
            elif kind=='before':event['beforeText']=''
            elif kind=='range':event['selectionEnd']-=1
            elif kind=='trust':event['inputTrusted']=False
            elif kind=='revision':event['revision']=4
            elif kind=='parent-dom':event['after'][0]['elements'][0]['elements'][0]['text']='changed'
            elif kind=='placeholder':w['placeholder']={'childHtml':'<span><br></span>'}
            else:w['typing']['after']=copy.deepcopy(w['after'])
            with self.subTest(kind=kind),self.assertRaises(RuntimeError):O.validate(bad,'unicode','correction')

    def test_v8_original_tail_updates_and_wholepair_are_distinct(self):
        stages,terminal,checkpoint=F.recovery('review','unicode','restore','retained')
        for key in ('version','draft','child_draft','child_updates','offset_utf16','sections','request'):
            bad=copy.deepcopy(checkpoint)
            if key=='version':bad[key]=3
            elif key=='draft':bad[key]='final only'
            elif key=='child_draft':bad['action']['SplitTyping'][key]=''
            elif key=='child_updates':bad['action']['SplitTyping'][key]=[F.wiretext(F.literals('unicode','restore').tail)]
            elif key=='offset_utf16':bad['action']['SplitTyping'][key]=15
            elif key=='sections':bad[key].pop()
            else:bad[key]=bad[key][:-1]
            with self.subTest(key=key),self.assertRaises(RuntimeError):R.retained_pair(bad,stages[0]['canonical'],'unicode','restore','retained')
        duplicate=copy.deepcopy(checkpoint);duplicate['sections'][0]=duplicate['sections'][1]
        with self.assertRaises(RuntimeError):R.retained_pair(duplicate,stages[0]['canonical'],'unicode','restore','retained')
        bad=copy.deepcopy(stages);bad[0]['preview']['initialTail']['text']=''
        with self.assertRaises(RuntimeError):R.validate('review',bad,terminal,checkpoint,'unicode','restore','retained')

    def test_ack_readback_both_continuations_and_no_replay_are_required(self):
        p=F.receipt('success','ascii','erase','bootstrap')
        for kind in ('ack-body','ack-lineage','readback','child-continuation','parent-continuation'):
            bad=copy.deepcopy(p)
            if kind in ('ack-body','ack-lineage'):
                rowindex=1 if kind=='ack-body' else 0
                response=bad['stages'][3]['fixture']['acknowledgements'][0]
                outer=S.decoded(response['raw']);data=S.wire(S.field(outer,2,2));rows=S.messages(data,1);row=S.raw(rows[rowindex])
                row=F.F.remove(row,12) if kind=='ack-body' else F.F.replace(row,38,F.F.remove(S.field(S.wire(row),38,2),10))
                rows[rowindex]=S.wire(row);response['raw']=F.F.b64(F.F.f(1,1)+F.F.f(2,F.F.f(15,'document')+b''.join(F.F.f(1,S.raw(r)) for r in rows)))
            elif kind=='readback':bad['stages'][3]['fixture']['readbacks'][0]['sections'].pop()
            elif kind=='child-continuation':bad['stages'][6]['fixture']['readbacks'][1]['sections'][1]['raw']=bad['original']['fresh']['sections'][1]['raw']
            else:bad['stages'].pop()
            with self.subTest(kind=kind),self.assertRaises(RuntimeError):O.validate(bad,typing='erase')
        for phase in ('restart','storage-failure'):
            stages,terminal,cp=F.recovery(phase);stages[0]['dispatches']=1
            with self.assertRaises(RuntimeError):R.validate(phase,stages,terminal,cp)

    def test_four_matched_untyped_reference_parent_head_duplication(self):
        vectors=json.loads((F.REF/'untyped-baseline-result.json').read_text(encoding='utf-8'))['vectors'];self.assertEqual(len(vectors),4)
        for v in vectors:
            parent,child=S.messages(S.decoded(v['wireBase64']),1);pc,cc=(S.wire(S.field(r,38,2)) for r in (parent,child))
            heads=json.loads(S.text(S.wire(S.field(pc,10,2)),5))
            self.assertEqual([f[2].decode() for f in pc if f[0]==6],heads+[S.text(pc,12)]+heads)
            self.assertEqual([f[2].decode() for f in cc if f[0]==6],[S.text(cc,12)])
        old=F.F.receipt();S.validate(old)
        data=S.decoded(old['stages'][2]['fixture']['writes'][0]['documentData']);parent,child=S.messages(data,1);pc=S.wire(S.field(parent,38,2))
        heads=[f[2] for f in pc if f[0]==6]
        raw=F.F.remove(S.raw(pc),6)+b''.join(F.F.f(6,h) for h in heads[:2])
        mutation=F.F.f(1,F.F.replace(S.raw(parent),38,raw))+F.F.f(1,S.raw(child))+F.F.f(14,F.F.TITLE)
        with self.assertRaises(RuntimeError):S.split_write(dict(old['stages'][2]['fixture']['writes'][0],documentData=F.F.b64(mutation)))


if __name__=='__main__':unittest.main()
