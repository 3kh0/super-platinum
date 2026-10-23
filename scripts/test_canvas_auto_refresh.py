"""Portable actual-effect receipt regressions; no native execution."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import canvas_auto_refresh as O
from test_canvas_return_body import sample as body, control
from test_canvas_replacement_custody import set_path


def sample(case='success', viewport=(1280,780)):
    receipt=body('success',viewport)
    final=copy.deepcopy(receipt['stages'][-1]);receipt['stages']=receipt['stages'][:9]
    baseline=receipt['stages'][8]
    visit=baseline['returnBody']['source']['owner']['visit']
    empty=dict(case=case,armed=False,current=False,visit=0,events=[],scheduling=[],pairs=[],requests=[],held=None,quiet=None,networkCalls=0,httpCapability=False)
    for row in receipt['stages']:row['autoRefresh']=copy.deepcopy(empty)
    events=[dict(serial=i+1,team='T1',frame=dict(type='file_change',file_id=O.B.FILE),visit=visit,readBefore=1+i//10,millis=[0,400,900][i//10],accepted=True,ingress='protected-post-authentication') for i in range(30)]
    schedule=[dict(kind='scheduled',visit=visit,millis=1),dict(kind='claimed',visit=visit,millis=251,admitted=True,read=2),dict(kind='scheduled',visit=visit,millis=502),dict(kind='claimed',visit=visit,millis=752,admitted=True,read=3)]
    quiet=None
    retry=dict(action='retry',control=control(8,'Retry'),event=dict(type='click',isTrusted=False))
    for name in O.stages(case)[9:]:
        restored=name=='reply-restored';ready=name in ('auto-ready','reply-restored');failed=name in ('auto-failed','failure-latched')
        row=copy.deepcopy(final if restored else baseline)
        row.update(stage=name,ordinal=len(receipt['stages']))
        ec,pc,held,statuses=O.plan(name,case)
        pairs=[]
        for i,status in enumerate(statuses):
            p=dict(serial=i+1,read=i+2,visit=visit,startedMillis=[252,753,1800][i],status=status,readCurrentAtStart=True,releasedMillis=None)
            if status!='pending':
                p.update(finishedMillis=[501,851,1801][i],ownerCurrentAtFinish=True,readCurrentAtFinish=i!=0)
                if i<2:p['releasedMillis']=[500,850][i]
            pairs.append(p)
        a=dict(case=case,armed=True,current=True,visit=visit,events=copy.deepcopy(events[:ec]),scheduling=copy.deepcopy(schedule[:min(pc,2)*2]),pairs=pairs,requests=[r for n in range(1,pc+1) for r in O.requests(n,case)],held=held,quiet=None,networkCalls=0,httpCapability=False)
        if name=='failure-latched':quiet=dict(elapsedMillis=800,before=copy.deepcopy(a),after=copy.deepcopy(a))
        a['quiet']=copy.deepcopy(quiet);row['autoRefresh']=a
        source=row['returnBody']['source'];source['presentation'].update(read=pc+1,loading=name in ('auto-held','latest-pending'),stale=not ready,error=O.B.ERROR if failed else None)
        native=row['returnBody']['native']
        native.update(title=O.TITLE if ready else O.B.TITLE,body=None if restored else O.LATEST_BODY if ready else O.INITIAL_BODY,error=O.B.ERROR if failed else None,status=['Refreshing canvas state\u2026'] if name in ('auto-held','latest-pending') else ['This canvas changed. Refresh to continue editing.'] if name=='newer-event' else [])
        native['controls']['refresh']['disabled']=name in ('auto-held','newer-event','latest-pending') or restored
        native['controls']['retry']=control(8,'Retry') if failed else None
        native['actions']=copy.deepcopy(baseline['returnBody']['native']['actions'])
        if ready and case=='failure':native['actions'].append(copy.deepcopy(retry))
        if restored:native['actions'].append(copy.deepcopy(final['returnBody']['native']['actions'][-1]))
        receipt['stages'].append(row)
    receipt['layout']=copy.deepcopy(receipt['stages'][-1]['layout'])
    return receipt


def wrapper():
    spec=importlib.util.spec_from_file_location('canvas_auto_wrapper',Path(__file__).with_name('check-canvas-auto-refresh-native.py'))
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module


class AutoRefresh(unittest.TestCase):
    def reject(self,path,value,case='success'):
        data=sample(case);set_path(data,path,value)
        with self.assertRaises(RuntimeError):O.validate(data,O.FIXTURES[0],case)

    def test_cases_widths_and_exact_prefix(self):
        for case in O.CASES:
            for fixture,viewport in zip(O.FIXTURES,((1280,780),(668,380))):
                with self.subTest(case=case,fixture=fixture):O.validate(sample(case,viewport),fixture,case)
        self.reject(('stages',8,'returnBody','source','presentation','stale'),True)
        self.reject(('stages',7,'stage'),'body-ready')
        self.reject(('stages',0,'autoRefresh','armed'),True)

    def test_actual_debounce_and_coalescing(self):
        self.reject(('stages',9,'autoRefresh','scheduling',1,'millis'),249)
        self.reject(('stages',9,'autoRefresh','scheduling',1,'admitted'),False)
        self.reject(('stages',11,'autoRefresh','scheduling',3,'read'),2)
        self.reject(('stages',9,'autoRefresh','events',0,'ingress'),'socket')
        self.reject(('stages',10,'autoRefresh','events',19,'readBefore'),1)
        self.reject(('stages',11,'autoRefresh','pairs',0,'releasedMillis'),399)
        self.reject(('stages',11,'autoRefresh','pairs',1,'startedMillis'),500)

    def test_original_owner_and_stale_success_cannot_adopt(self):
        self.reject(('stages',11,'autoRefresh','pairs',0,'readCurrentAtFinish'),True)
        self.reject(('stages',11,'autoRefresh','pairs',0,'ownerCurrentAtFinish'),False)
        self.reject(('stages',11,'autoRefresh','pairs',0,'status'),'failed')
        self.reject(('stages',10,'returnBody','native','title'),O.OLD_TITLE)
        self.reject(('stages',11,'returnBody','source','presentation','stale'),False)
        self.reject(('stages',12,'returnBody','source','presentation','read'),2)
        self.reject(('stages',12,'autoRefresh','current'),False)
        self.reject(('stages',12,'autoRefresh','pairs',1,'readCurrentAtFinish'),False)

    def test_exact_factories_raw_receipts_no_live(self):
        self.reject(('stages',11,'autoRefresh','requests',0,'path'),'/api/chat.postMessage')
        self.reject(('stages',11,'autoRefresh','requests',5,'response','file','editable'),True)
        self.reject(('stages',11,'autoRefresh','requests',9,'response','html'),O.OLD_HTML)
        self.reject(('stages',11,'autoRefresh','httpCapability'),True)
        self.reject(('stages',11,'autoRefresh','networkCalls'),1)
        self.reject(('stages',11,'returnBody','source','pairs',0,'read'),2)

    def test_failed_pair_retains_old_canonical_and_one_error(self):
        for path,value in [ (('source','presentation','snapshot'),None),
                            (('source','presentation','error'),None),
                            (('native','body'),O.LATEST_BODY),
                            (('native','status'),[O.B.ERROR]),
                            (('native','controls','retry','disabled'),True) ]:
            self.reject(('stages',12,'returnBody',*path),value,'failure')
        self.reject(('stages',12,'autoRefresh','requests',9,'response'),{'html':O.HTML},'failure')

    def test_latch_no_automatic_retry_actual_retry_only(self):
        self.reject(('stages',13,'autoRefresh','quiet','elapsedMillis'),100,'failure')
        self.reject(('stages',13,'autoRefresh','quiet','after','held'),3,'failure')
        self.reject(('stages',13,'autoRefresh','pairs',1,'status'),'ready','failure')
        self.reject(('stages',14,'returnBody','native','actions',2,'action'),'refresh','failure')
        self.reject(('stages',14,'returnBody','native','actions',2,'control','node'),999,'failure')
        self.reject(('stages',14,'autoRefresh','pairs',2,'releasedMillis'),1800,'failure')
        self.reject(('stages',14,'autoRefresh','scheduling'),[], 'failure')

    def test_journal_successor_and_old_authority_remain_exact(self):
        self.reject(('stages',11,'retainedEntry','journal','drafts',0,'revision'),3)
        self.reject(('stages',11,'retainedEntry','runtime','sending'),True)
        self.reject(('stages',11,'successor','runtime','text'),'foreign change')
        self.reject(('stages',11,'originalDestinationCurrent'),True)
        self.reject(('stages',11,'shell','noTransport'),False)
        self.reject(('stages',11,'returnBody','native','captures',1,'acks',0,'sequence'),99)

    def test_final_fresh_editor_and_controls(self):
        self.reject(('stages',13,'returnBody','native','currentEditor','sameFinishedNode'),True)
        self.reject(('stages',13,'returnBody','native','currentEditor','blocks'),[])
        self.reject(('stages',13,'returnBody','native','controls','refresh','disabled'),False)
        self.reject(('stages',13,'returnBody','native','actions',2,'event','isTrusted'),True)
        self.reject(('stages',9,'returnBody','native','controls','refresh','disabled'),False)

    def test_wrapper_flags_closed_inventory(self):
        w=wrapper()
        for case,spec in w.SPECS.items():
            receipt=sample(case)
            with tempfile.TemporaryDirectory() as folder,patch('canvas_native.expected_viewport',return_value=[1280,780]):
                base=Path(folder);flags=w.private_checks(spec,base)
                self.assertEqual(flags[spec.flag],case)
                self.assertEqual(flags['SUPER_PLATINUM_CANVAS_RETURN_BODY_CHECK'],'success')
                self.assertEqual(flags['SUPER_PLATINUM_CANVAS_REPLACEMENT_CUSTODY_CHECK'],'1')
                self.assertFalse(spec.synthetic_context)
                w.validate(spec,O.FIXTURES[0],receipt)
                log='\n'.join(O.PREFIX+'-stage: '+json.dumps(s) for s in receipt['stages'])+'\n'+O.PREFIX+'-check: '+json.dumps(receipt)+'\n'
                path=base/'native.log';path.write_text(log,encoding='utf8')
                captured=[{**s,'screenshot':str(i)+'.png'} for i,s in enumerate(receipt['stages'])]
                w.validate_closed(spec,O.FIXTURES[0],path,captured,receipt)
                bad=copy.deepcopy(captured);bad[-1]['autoRefresh']['current']=False
                with self.assertRaises(RuntimeError):w.validate_closed(spec,O.FIXTURES[0],path,bad,receipt)
        with self.assertRaises(RuntimeError):w.case_for(object())

if __name__=='__main__':unittest.main()
