"""Receipt corruption tests only; synthetic values are never native evidence."""
from pathlib import Path
import copy
from contextlib import contextmanager
import json
import math
import runpy
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

HERE=Path(__file__).resolve().parent
# Staged tests reuse pinned main helpers; no native process is launched.
if not (HERE/'native_check.py').is_file():
    ROOT=next(p for p in HERE.parents if (p/'scripts/native_check.py').is_file())
    sys.path.append(str(ROOT/'scripts'))
from search_has_receipts import CASES,ACTIONS,FIXTURE,validate,validate_stage,geometry

def receipt(narrow=False):
    fixture=FIXTURE+('-narrow' if narrow else '');viewport=[668,380] if narrow else [1280,780]
    def geometry():return {'rect':[40,80,110,110],'clip':[0,0,*viewport],'point':[75,95],
       'visible':True,'hit':True,'disabled':False,'tabIndex':0}
    def focus():
        return dict(geometry(),focused=True,focusVisible=True,outlineWidth=2,outlineOffset=-2,
            outlineStyle='solid',outlineColor='rgb(96, 165, 250)',outline=[40,80,110,110])
    records=[]
    for i,(name,query,flags,error,revision) in enumerate(CASES):
        controls={k:geometry() for k in ('query','filters')}
        if flags is not None:controls.update({k:geometry() for k in ('file','link','apply','clear')})
        if error:
            controls['alert']=geometry();controls['alert']['tabIndex']=-1
            controls['apply']['focusGeometry']=focus()
        native={'query':query,'revision':1+revision,'panel':flags is not None,'flags':flags,'error':error,
           'results':0,'page':0,'loading':False,'noSession':True,'noTransport':True}
        dom={'ok':True,'viewport':viewport,'query':query,'queryRevision':query,'panel':flags is not None,
          'expanded':str(flags is not None).lower(),'flags':flags,'labels':[] if flags is None else ['Has files','Has links','In threads','Saved items','Pinned messages'],
          'alert':error,'controls':controls,'dialog':[20,20,viewport[0]-20,viewport[1]-20],'horizontalOverflow':False,'results':0}
        records.append({'ok':True,'stage':name,'ordinal':i,'noSession':True,'noTransport':True,'native':native,'dom':dom})
    return {'ok':True,'fixture':fixture,'noSession':True,'noTransport':True,'credentialBoundary':'actual search credentials refused; no raw request ledger',
      'records':records,'actions':[dict(ok=True,kind=k,value=v,focusedAtDispatch=True,**({'focusGeometry':focus()} if k=='click' and v in ('Apply filters','Clear filters') else {})) for k,v in ACTIONS],
      'refusedRevision':4,'layout':{'ok':True,'viewport':viewport}}

class Receipts(unittest.TestCase):
    def reject(self,change):
        value=receipt();change(value)
        with self.assertRaises((RuntimeError,KeyError,TypeError)):validate(FIXTURE,value,'win32')
    def test_normal_narrow_contract(self):
        for narrow in (False,True):
            value=receipt(narrow);validate(value['fixture'],value,'win32')
    def test_focus_outline_missing_or_forged(self):
        self.reject(lambda v:v['actions'][2].pop('focusGeometry'))
        self.reject(lambda v:v['actions'][4]['focusGeometry'].__setitem__('focusVisible',False))
        self.reject(lambda v:v['records'][5]['dom']['controls']['apply'].pop('focusGeometry'))
        self.reject(lambda v:v['records'][5]['dom']['controls']['apply']['focusGeometry'].__setitem__('focused',False))
        self.reject(lambda v:v['actions'][2]['focusGeometry'].__setitem__('outline',[41,80,110,110]))
    def test_outward_outline_clipping_despite_center_hit(self):
        def corrupt(v):
            g=v['actions'][2]['focusGeometry'];g.update(clip=[40,80,110,110],outlineOffset=2,outline=[36,76,114,114])
        self.reject(corrupt)
        value=receipt(True);value['actions'][2]['focusGeometry']['clip']=[40,80,110,110]
        validate(value['fixture'],value,'win32')
    def test_focus_geometry_cannot_bind_another_control(self):
        def corrupt(v):
            g=v['records'][5]['dom']['controls']['apply']['focusGeometry']
            g.update(rect=[41,80,111,110],point=[76,95],outline=[41,80,111,110])
        self.reject(corrupt)
    def test_outline_style_and_numeric_types_fail_closed(self):
        for key,value in [('outlineWidth',True),('outlineWidth',0),('outlineOffset',float('nan')),('outlineStyle','none'),('outlineColor','transparent')]:
            self.reject(lambda v,k=key,x=value:v['actions'][2]['focusGeometry'].__setitem__(k,x))
    def test_missing_duplicate_order(self):
        self.reject(lambda v:v['records'].pop())
        self.reject(lambda v:v['records'].__setitem__(2,copy.deepcopy(v['records'][1])))
    def test_preserved_syntax_cannot_change_self_consistently(self):
        def corrupt(v):
            row=v['records'][2];row['native']['query']=row['dom']['query']=row['dom']['queryRevision']='launch has:link'
        self.reject(corrupt)
    def test_native_dom_selected_state_must_match(self):
        self.reject(lambda v:v['records'][0]['dom'].__setitem__('flags',[False,False]))
        self.reject(lambda v:v['records'][0]['native'].__setitem__('flags',[1,False]))
    def test_refusal_requires_error_unchanged_query_revision(self):
        self.reject(lambda v:v['records'][5]['dom'].__setitem__('alert',None))
        self.reject(lambda v:v['records'][5]['native'].__setitem__('revision',5))
        self.reject(lambda v:v.__setitem__('refusedRevision',4.0))
    def test_exact_action_keyboard_focus(self):
        self.reject(lambda v:v['actions'][8].__setitem__('kind','click'))
        self.reject(lambda v:v['actions'][8].__setitem__('focusedAtDispatch',False))
        self.reject(lambda v:v['actions'].pop())
    def test_isolation_and_no_fabricated_results(self):
        self.reject(lambda v:v['records'][1]['native'].__setitem__('noTransport',False))
        self.reject(lambda v:v['records'][2]['native'].__setitem__('page',1))
        self.reject(lambda v:v['records'][2]['dom'].__setitem__('results',True))
    def test_geometry_visible_hit_clipping_overflow(self):
        self.reject(lambda v:v['records'][0]['dom']['controls']['file'].__setitem__('hit',False))
        self.reject(lambda v:v['records'][0]['dom']['controls']['file'].__setitem__('rect',[-4,80,110,110]))
        self.reject(lambda v:v['records'][0]['dom']['controls']['file'].__setitem__('point',[float('nan'),95]))
        self.reject(lambda v:v['records'][0]['dom'].__setitem__('horizontalOverflow',True))
    def test_observed_narrow_json_midpoint_rounding(self):
        # Exact retained V16 initial geometry, not fabricated native evidence.
        for rect,point in (
            ([68.66667175292969,144.59375,127.96875762939452,176.59375],[98.31771469116212,160.59375]),
            ([68.66667175292969,94.26042175292967,599.3333587646484,134.59375762939453],[334.00001525878906,114.42708969116212]),
        ):
            g={'rect':rect,'point':point,'clip':[54,45.59375,614,363.9895935058594],
               'visible':True,'hit':True,'disabled':False,'tabIndex':0}
            geometry(g,[668,380])
    def test_midpoint_rounding_is_bounded_to_two_ulps(self):
        g=receipt()['records'][0]['dom']['controls']['query']
        for direction in (-math.inf,math.inf):
            point=75.0
            for _ in range(2):point=math.nextafter(point,direction)
            g['point'][0]=point;geometry(g,[1280,780])
            g['point'][0]=math.nextafter(point,direction)
            with self.assertRaisesRegex(RuntimeError,'Search hit point differs'):geometry(g,[1280,780])
        for point in (75.000001,75.5,76):
            g['point'][0]=point
            with self.assertRaisesRegex(RuntimeError,'Search hit point differs'):geometry(g,[1280,780])
    def test_rounding_cannot_admit_outside_clip_or_missed_hit(self):
        g=receipt()['records'][0]['dom']['controls']['query']
        g['rect']=[74.5,80,75.5,110];g['clip']=[75.25,0,1280,780]
        with self.assertRaisesRegex(RuntimeError,'outside control/clip'):geometry(g,[1280,780])
        g['clip']=[0,0,1280,780];g['hit']=False
        with self.assertRaises(RuntimeError):geometry(g,[1280,780])
    def test_toggle_layout_stable(self):
        def move(v):
            g=v['records'][1]['dom']['controls']['file'];g['rect']=[42,80,112,110];g['point']=[77,95]
        self.reject(move)
    def test_integer_identity_and_expanded(self):
        self.reject(lambda v:v['records'][0].__setitem__('ordinal',False))
        self.reject(lambda v:v['records'][0]['native'].__setitem__('revision',True))
        self.reject(lambda v:v['records'][0]['dom'].__setitem__('expanded','false'))
    def test_stage_early_validation(self):
        row=receipt()['records'][0]
        validate_stage(row,0,FIXTURE,platform='win32')
        row['native']['query']='changed'
        with self.assertRaises(RuntimeError):validate_stage(row,0,FIXTURE,platform='win32')

class RunnerFailures(unittest.TestCase):
    def run_case(self,primary=False,ack_failure=False,final_failure=None):
        namespace=runpy.run_path(str(HERE/'check-search-has-native.py'))
        run=namespace['run'];globals_=run.__globals__;actual=receipt();captured={};cleanup=[]
        primary_error=RuntimeError('original native failure')
        class Ack:
            name='unused-private-mock-ack'
            def cleanup(self):
                cleanup.append(True)
                if ack_failure:raise OSError('ack cleanup failed')
        class Native:
            def __init__(self,fixture,log,binary,checks):self.log=log
            def require_running(self):
                if primary:raise primary_error
            def receipts(self,prefix):return actual['records'] if '-stage:' in prefix else [actual]
        class Audit:
            def checkpoint(self,label):pass
            def capture(self,path,label):captured['evidence'].setdefault('captures',[]).append({'path':str(path)})
        @contextmanager
        def audited(native,evidence):
            captured['evidence']=evidence
            yield Audit()
        def capture(native,spec,fixture,row,index,captures,acks,audit):return {**row,'screenshot':str(captures/(str(index)+'.png'))}
        def dumps(value,**kw):
            if final_failure=='serialize':raise TypeError('receipt serialization failed')
            return json.dumps(value,**kw)
        actual_open=Path.open
        def opened(path,*args,**kw):
            if final_failure=='write' and path.name.endswith('.json.staged'):raise OSError('receipt write failed')
            return actual_open(path,*args,**kw)
        replacements={'NativeFixture':Native,'audited_fixture':audited,'capture_stage':capture,
            'validate_stage':lambda *a,**k:None,'validate':lambda *a,**k:None,'validate_closed':lambda *a,**k:None,
            'png_signature':lambda *a,**k:None,'json':SimpleNamespace(dumps=dumps),
            'tempfile':SimpleNamespace(TemporaryDirectory=lambda **kw:Ack())}
        with tempfile.TemporaryDirectory(prefix='sp-search-runner-unit-') as directory:
            output=Path(directory)/'evidence'
            with patch.dict(globals_,replacements),patch.object(Path,'open',opened):
                try:run(FIXTURE,Path('never-launched.exe'),output)
                except BaseException as error:captured['error']=error
            path=output/(FIXTURE+'.json')
            captured['receipt']=json.loads(path.read_text(encoding='utf-8')) if path.exists() else None
            captured['stagedExists']=(output/(FIXTURE+'.json.staged')).exists()
        self.assertEqual(cleanup,[True]);return captured,primary_error
    def test_full_run_ack_failure_preserves_native_primary(self):
        got,primary=self.run_case(primary=True,ack_failure=True)
        self.assertIs(got['error'],primary);self.assertIn('ack cleanup',str(primary.__notes__))
        self.assertFalse(got['receipt']['ok']);self.assertIn('original native failure',got['receipt']['error'])
        self.assertIn('ackCleanupError',got['receipt'])
    def test_full_run_ack_failure_without_primary_is_failure(self):
        got,_=self.run_case(ack_failure=True)
        self.assertIsInstance(got['error'],OSError);self.assertFalse(got['receipt']['ok'])
        self.assertIn('ack cleanup failed',got['receipt']['error'])
    def test_full_run_serialization_failure_preserves_primary(self):
        got,primary=self.run_case(primary=True,final_failure='serialize')
        self.assertIs(got['error'],primary);self.assertFalse(got['evidence']['ok']);self.assertIsNone(got['receipt'])
        self.assertIn('serialization failed',str(primary.__notes__))
    def test_full_run_serialization_failure_without_primary(self):
        got,_=self.run_case(final_failure='serialize')
        self.assertIsInstance(got['error'],TypeError);self.assertFalse(got['evidence']['ok']);self.assertIsNone(got['receipt'])
    def test_full_run_write_failure_preserves_primary(self):
        got,primary=self.run_case(primary=True,final_failure='write')
        self.assertIs(got['error'],primary);self.assertFalse(got['evidence']['ok']);self.assertIsNone(got['receipt'])
        self.assertIn('receipt write failed',str(primary.__notes__))
    def test_full_run_write_failure_without_primary(self):
        got,_=self.run_case(final_failure='write')
        self.assertIsInstance(got['error'],OSError);self.assertFalse(got['evidence']['ok']);self.assertIsNone(got['receipt'])
    def test_full_run_success_publishes_complete_exclusive_receipt(self):
        got,_=self.run_case()
        self.assertNotIn('error',got);self.assertTrue(got['receipt']['ok']);self.assertTrue(got['stagedExists'])
    def test_final_publication_never_overwrites_existing_receipt(self):
        fn=runpy.run_path(str(HERE/'check-search-has-native.py'))['finalize']
        with tempfile.TemporaryDirectory(prefix='sp-search-receipt-unit-') as directory:
            path=Path(directory)/'result.json';path.write_text('previous',encoding='utf-8');evidence={'ok':True}
            with self.assertRaises(FileExistsError):fn(path,evidence,None)
            self.assertEqual(path.read_text(encoding='utf-8'),'previous');self.assertFalse(evidence['ok'])

if __name__=='__main__':unittest.main()
