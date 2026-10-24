"""Pre-Enter source/live boundary, original wire histories and offline recovery."""
import copy
import json
from pathlib import Path
import subprocess
import unittest

import canvas_sequence_input as I
import canvas_sequence_oracle as N
import canvas_sequence_wire as W
import canvas_sequence_recovery_oracle as R
import test_canvas_sequence_fixtures as F
from test_canvas_sequence_revisit import witness as controller_witness

ROOT=Path(__file__).resolve().parent.parent
VECTORS=json.loads((ROOT/'tmp/canvas-preenter-sequence-20261023/reference/bindings.json').read_text(encoding='utf-8'))['vectors']
PRE=[v for v in VECTORS if v['schedule']=='parent-preenter']


def fixture(v,count):
    e=W.config(count,v['mode'],v['layout'],'preenter-'+v['revisit'].replace('exact-',''),v['corpus'])
    index=next(i for i,s in enumerate(F.SUMMARIES) if s['appendCount']==count and s['mode']==v['mode']
               and s['layout']==v['layout'] and s['corpus']==v['corpus'] and s['cadence']=='straight')
    source,value,sent=F.source_request(index)
    original=W.S.raw(W.S.messages(W.decoded(v['wireBase64']),1)[0])
    session=F.F.replace(W.field(W.wire(original),38,2),1,e.original)
    sent[0]=F.F.replace(F.F.replace(F.F.replace(F.F.replace(original,1,e.original),38,session),33,F.SESSION+'-0'),12,F.F.body(e.parent_wire,True))
    value.update(parentUpdates=list(e.parent_reports),parentFinalRtml=e.parent_wire,
                 documentData=F.F.b64(F.F.f(14,F.F.TITLE)+b''.join(F.F.f(1,row) for row in sent)))
    cp=F.checkpoint(source,value);cp['version']=15
    cp['action']={'EndSequenceParent':dict(parent_updates=list(e.parent_reports),members=cp['action']['EndSequence']['members'])}
    return e,index,source,value,sent,cp


def witness(plan):
    result=controller_witness(plan)
    first=next(e for e in result['sequence']['events'] if e['kind']=='enter')
    result.update(offset=first['start'],revision=first['revision'],after=first['after'],rawAfter=first['rawAfter'])
    return result


class PreenterTests(unittest.TestCase):
    def test_controller_first_enter_uses_live_text_but_retains_original_source(self):
        script="import {actionPlan} from './scripts/canvas_sequence_input.mjs';console.log(JSON.stringify(['ascii','unicode'].flatMap(corpus=>['correction','erase','revert'].flatMap(k=>[1,3,16].map(count=>({corpus,cadence:'preenter-'+k,count,actions:actionPlan(corpus,count,'preenter-'+k)}))))));"
        plans=json.loads(subprocess.run(['node','--input-type=module','-e',script],cwd=ROOT,text=True,encoding='utf-8',capture_output=True,check=True,timeout=30).stdout)
        for plan in plans:
            with self.subTest(corpus=plan['corpus'],cadence=plan['cadence'],count=plan['count']):
                e=W.config(plan['count'],cadence=plan['cadence'],corpus=plan['corpus']);value=witness(plan);N.witness(value,e)
                events=value['sequence']['events'];n=len(e.parent_updates)
                self.assertEqual([r['afterText'] for r in events[:n]],list(e.parent_updates))
                self.assertTrue(all(r['activeIndex']==0 and len(r['sections'])==1 for r in events[:n]))
                self.assertEqual(value['offset'],W.T.utf16(e.parent_final));self.assertEqual(value['revision'],n+1)
                self.assertEqual(value['before'],F.blocks([e.seed]));self.assertEqual(events[n]['beforeText'],e.parent_final)
                for key,bad in [('offset',99),('revision',1),('before',F.blocks([e.parent_final+' forged'])),('after',events[0]['after']),('rawAfter',events[0]['rawAfter'])]:
                    broken=copy.deepcopy(value);broken[key]=bad
                    with self.assertRaises(RuntimeError):N.witness(broken,e)
                for key,bad in [('activeIndex',1),('beforeText','forged'),('beforeInputTrusted',False),('revision',99)]:
                    broken=copy.deepcopy(value);broken['sequence']['events'][0][key]=bad
                    with self.assertRaises(RuntimeError):N.witness(broken,e)
                broken=copy.deepcopy(value);broken['sequence']['events'].pop(0)
                with self.assertRaises(RuntimeError):N.witness(broken,e)
                broken=copy.deepcopy(value);broken['sequence']['events'][n]['sections'][0]['html']='<span>altered</span>'
                with self.assertRaises(RuntimeError):N.witness(broken,e)

    def test_original_parent_history_is_independent_of_member_count_and_enter_timing(self):
        self.assertEqual(len(PRE),24)
        for v in PRE:
            post=next(x for x in VECTORS if all(x[k]==v[k] for k in ('mode','corpus','layout','revisit')) and x['schedule']!='parent-preenter')
            self.assertEqual(v['wireBase64'],post['wireBase64'])
            for count in (1,2,3):
                with self.subTest(vector=v['ordinal'],count=count):
                    e,_,source,value,sent,cp=fixture(v,count);before=W.inventory(source,e)
                    ids,rows=W.write(value,before,e)
                    W.journal(dict(scope=W.S.SCOPE,receipt=dict(operation='inert-preenter',checkpoint=cp)),e,value,source)
                    self.assertEqual(list(e.parent_reports),v['orderedParentChangedReports'])
                    changed=[r for r in v['parentReports'] if not r['unchanged']]
                    self.assertEqual(W.parent_patch_groups(e),[[p for group in r['patches'] for p in group] for r in changed])
                    self.assertEqual(cp['draft'],e.wire_seed)
                    bodies={e.original:e.parent_wire,**dict(zip(ids,map(W.M.rtml,e.final)))}
                    seq={e.original:2000,**{identity:2000 if e.reused and i==0 else 1000 for i,identity in enumerate(ids)}}
                    accepted=[F.authored(raw,W.S.raw(before[W.text(row,1)]) if W.text(row,1) in before else None,bodies[W.text(row,1)],seq[W.text(row,1)],i==0) for i,(raw,row) in enumerate(zip(sent,rows))]
                    W.ack(F.F.acknowledgement(accepted),rows,before,bodies,seq,e)
                    with self.assertRaises(RuntimeError):W.ack(F.F.acknowledgement(accepted[1:]),rows,before,bodies,seq,e)
                    with self.assertRaises(RuntimeError):W.write(dict(value,parentUpdates=[]),before,e)

    def test_v15_recovery_keeps_single_and_multiple_member_parent_reports(self):
        for v in [v for v in PRE if v['mode']=='retained-zero' and v['corpus']=='unicode']:
            for count in (1,3):
                e,index,source,value,_,cp=fixture(v,count)
                for phase in R.STAGES:
                    stages,terminal,_=F.recovery(phase,index)
                    for stage in stages:
                        stage['sequenceCadence']=e.cadence
                        if stage['journal']['active']:stage['journal']['active']['checkpoint']=copy.deepcopy(cp)
                        preview=stage['preview'].get('sequence')
                        if preview:preview.update(parentText=e.parent_final,parentReportCount=len(e.parent_reports),parentEmptyLabel=None if e.parent_final else 'Empty paragraph',historyNotice=True)
                    terminal['sequenceCadence']=e.cadence;terminal['journal']=copy.deepcopy(stages[-1]['journal'])
                    R.validate(phase,stages,terminal,cp,append_count=count,history=e.history,layout=e.layout,cadence=e.cadence,corpus=e.corpus)

    def test_single_member_only_enters_native_preenter_route(self):
        for cadence in I.CADENCES:
            for count in (1,2,3,16,17):
                request=dict(ok=True,fixture='canvas-structure',nonce='a'*32,ordinal=0,corpus='ascii',mode='sequence',typing='none',sequenceHistory='bootstrap',sequenceLayout='no-following',sequenceCount=count,sequenceCadence=cadence,fieldId='canvas-text-draft',seed='Keep this',offset=9,revision=0,generation='1',visit='0')
                allowed=count<=16 and (count>=2 or cadence.startswith('preenter-')) and (count>=3 or not cadence.startswith(('parent-','revisit-')))
                args=('canvas-structure','a'*32,0,'bootstrap','no-following','ascii',count,cadence)
                if allowed:I.validate_request(request,*args)
                else:
                    with self.assertRaises(RuntimeError):I.validate_request(request,*args)
        for script in ('check-canvas-split-native.py','check-canvas-split-recovery-native.py'):
            captures=ROOT/'tmp/must-not-create-preenter-cli'
            common=['--fixture','canvas-structure' if script=='check-canvas-split-native.py' else 'canvas-local-review','--captures',str(captures)]
            if script=='check-canvas-split-native.py':common+=['--scenario','success']
            for options,message in [(['--mode','sequence','--sequence-count','1'],'at least two'),(['--mode','sequence','--sequence-count','17','--sequence-cadence','preenter-erase'],'invalid choice'),(['--mode','middle','--sequence-cadence','preenter-revert'],'require sequence mode')]:
                run=subprocess.run(['python',str(ROOT/'scripts'/script),*common,*options],cwd=ROOT,text=True,encoding='utf-8',capture_output=True,timeout=20)
                self.assertEqual(run.returncode,2);self.assertIn(message,run.stderr)
            self.assertFalse(captures.exists())


if __name__=='__main__':unittest.main()
