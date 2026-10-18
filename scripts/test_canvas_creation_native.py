"""Pure receipt/ownership tests; no native launch or real persistence."""
import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace
from urllib.parse import urlencode

R=runpy.run_path(str(Path(__file__).with_name('check-canvas-creation-native.py')))


def request(body,executed=False):
    q={'_x_id':'1'*32,'_x_csid':'2'*32,'_x_desktop_ia':'true','_x_foreground':'true',
       '_x_frontend_build_type':'current','_x_gantry':'true','_x_num_retries':'0',
       '_x_version_ts':'capture-required','fp':'3'*32,'slack_route':'default'}
    return dict(method='POST',url='https://fixture.slack.com/api/files.createCanvas?'+urlencode(q),
                headers=[['Content-Type','application/x-www-form-urlencoded']],retrySafe=False,executed=executed,
                form=[['token','canvas-fixture-not-a-token'],['title',R['TITLE']],['markdown',body]])


def receipt(directory,scenario='editing',phase='exercise'):
    expected=R['ready_tuple'](R['FIXTURES'][0],'a'*32,123,scenario,phase)
    stages=('restart',) if phase=='restart' else R['STAGES'][scenario]
    edges=[dict(phase=phase,stage=s,edge=e) for s in stages for e in ('before','after')]
    body=R['expected_body'](scenario)
    locked=scenario=='late-result'
    native=dict(title=R['TITLE'],markdown=body,codePoints=list(map(ord,body)),utf16Length=len(body.encode('utf-16-le'))//2,
                titleReadonly=locked,bodyReadonly=locked,titleDisabled=locked,bodyDisabled=locked)
    active=dict(id='draft',owner=R['OWNER'],content=dict(title=R['TITLE'],markdown=body),archived=False,attempt=None)
    inventory=dict(active=active,drafts=[active],attempts=[],conflicts=[],lateResults=[],decisions=[],retryAt=0)
    result={k:True for k in (('restoredFromStore','noReplay') if phase=='restart' else R['REQUIRED'][scenario])}
    result['native']=native
    if scenario in ('known-unsent-conflict','departure') and phase=='exercise':
        result['reviewedNative']={k:dict(markdown=text,readonly=True) for k,text in [('canonical','Other isolated writer; retain this until explicit review.'),('retained',R['OURS'])]}
        result['reviewedInventory']=dict(conflicts=[dict(id='conflict',owner=R['OWNER'],draft_id='draft',base_revision=2,writer='writer',content=dict(title=R['TITLE'],markdown=R['OURS']))])
    if scenario=='late-result':
        active['attempt']='uncertain'
        old=dict(id='old',owner=R['OWNER'],content=dict(title=R['TITLE'],markdown=R['BODY']),archived=True,attempt='created')
        inventory['drafts'].append(old)
        inventory['attempts']=[dict(id='created',owner=R['OWNER'],draft_id='old',frozen=old['content'],delivery=dict(kind='Created',file_id='FCANVASFIXTURE1')),
                               dict(id='uncertain',owner=R['OWNER'],draft_id='draft',frozen=active['content'],delivery=dict(kind='Uncertain'))]
        inventory['decisions']=[dict(operation='archive')]
    prepared=[] if phase=='restart' or scenario in ('editing','departure') else ([request(R['BODY'],True),request(R['AHEAD'],True)] if scenario=='late-result' else [request(R['BODY'])])
    rectangle=dict(left=10,top=10,right=600,bottom=700,width=590,height=690)
    control=dict(left=20,top=650,right=100,bottom=682,width=80,height=32)
    geometry=dict(ok=True,noOverflow=True,footerHitTested=True,dialog=rectangle,viewport=dict(width=1280,height=780),
                  clientWidth=590,scrollWidth=590,controls=[dict(label='Close',hit=True,rect=control)],
                  fields=dict(left=10,top=60,right=600,bottom=500,width=590,height=440),
                  footer=dict(left=10,top=640,right=600,bottom=700,width=590,height=60),
                  feedback=dict(region=None,messages=[]))
    def measured(stage):
        value=copy.deepcopy(geometry)
        messages=R['expected_feedback'](scenario,phase,stage)
        if messages:
            value['feedback']=dict(region=dict(left=10,top=570,right=600,bottom=630,width=590,height=60),
                messages=[dict(role=role,text=text,visible=True,unclipped=True,
                    rect=dict(left=20,top=575+i*20,right=590,bottom=593+i*20,width=570,height=18)) for i,(role,text) in enumerate(messages)])
        return value
    value=dict(expected,ok=True,noSession=True,noTransport=True,noLiveNetwork=True,syntheticInput=True,
               isolatedDatabase=str(directory/'fixture.sqlite'),viewport=dict(width=1280,height=780),
               captures={s:str(directory/f"{expected['fixture']}-{phase}-{s}.png") for s in stages},
               geometry=[measured(stage) for stage in stages if stage!='closed'],result=result,
               model=dict(uncertain=locked,editable=not locked,delivery=dict(kind='Uncertain') if locked else None),
               syntheticIME=scenario in ('editing','departure') and phase=='exercise',preparedRequests=len(prepared),executedRequests=sum(r["executed"] for r in prepared),
               requests=prepared,inventory=inventory)
    if scenario=='departure' and phase=='exercise':
        def native_text(text,visit='second',error=''):
            return dict(title=R['TITLE'],markdown=text,visit=visit,error=error,codePoints=list(map(ord,text)),
                        utf16Length=len(text.encode('utf-16-le'))//2,
                        titleReadonly=False,bodyReadonly=False,titleDisabled=False,bodyDisabled=False)
        palette=copy.deepcopy(geometry)
        palette.update(kind='palette',focused=True,canvasAbsent=True,shortcutCount=1)
        palette['controls'][0]['label']='Channel or person'
        value['geometry'][1]=copy.deepcopy(palette)
        saved=copy.deepcopy(inventory)
        saved['active']['content']=dict(title=R['TITLE'],markdown=R['AHEAD'])
        saved['drafts']=[saved['active']]
        refused=copy.deepcopy(inventory)
        refused['active']['content']=dict(title=R['TITLE'],markdown='Other isolated writer; retain this until explicit review.')
        refused['drafts']=[refused['active']]
        refused['conflicts']=copy.deepcopy(result['reviewedInventory']['conflicts'])
        result['reviewedInventory']=copy.deepcopy(refused)
        capture_error='Finish composing text, then try again.'
        save_error='Your text is retained. Save or review it before navigating.'
        result.update(shortcuts=[dict(count=i,key='k',ctrl=True,meta=False,trusted=False,prevented=True,visit='first' if i==1 else 'second') for i in (1,2,3)],
            beforeGeneration=10,afterGeneration=11,refusalGeneration=12,finalRefusalGeneration=12,
            firstVisit='first',returned=native_text(R['AHEAD']),savedInventory=saved,palette=palette,
            captureRefusal=dict(sameEditor=True,pending=False,closeRequested=False,error=capture_error,
                native=native_text(R['COMPOSED'],error=capture_error),inventory=copy.deepcopy(saved),editorMarkdown=R['AHEAD']),
            saveRefusal=dict(sameEditor=True,pending=False,closeRequested=False,error=save_error,
                native=native_text(R['OURS'],error=save_error),inventory=refused,stableInventory=copy.deepcopy(refused),
                retainedSlots=1,editorFailed=True))
    return expected,value,edges


class Validation(unittest.TestCase):
    def validate(self,expected,value,edges,directory):
        R['validate_native'](expected,directory,value,edges,platform='win32',png=False)

    def test_local_and_restart_receipts(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        for phase in ('exercise','restart'):
            args=receipt(directory,phase=phase)
            self.validate(*args,directory)

    def test_departure_actual_shortcut_and_restart(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        for phase in ('exercise','restart'):
            self.validate(*receipt(directory,'departure',phase),directory)

    def test_departure_captured_palette_focus_and_exact_control(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        for location in ('capture','result'):
            for kind in ('focus','label','count'):
                e,v,s=receipt(directory,'departure')
                palette=v['geometry'][1] if location=='capture' else v['result']['palette']
                if kind=='focus':palette['focused']=False
                elif kind=='label':palette['controls'][0]['label']='Unrelated button'
                else:palette['controls'].append(copy.deepcopy(palette['controls'][0]))
                with self.assertRaises(RuntimeError):self.validate(e,v,s,directory)

    def test_departure_requires_complete_intermediate_inventories(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        for location in ('saved','refused','review'):
            for key in ('active','drafts','attempts','conflicts','lateResults','decisions','retryAt'):
                e,v,s=receipt(directory,'departure');r=v['result']
                if location=='saved':
                    del r['savedInventory'][key]
                    r['captureRefusal']['inventory']=copy.deepcopy(r['savedInventory'])
                elif location=='refused':
                    del r['saveRefusal']['inventory'][key]
                    r['saveRefusal']['stableInventory']=copy.deepcopy(r['saveRefusal']['inventory'])
                else:del r['reviewedInventory'][key]
                with self.assertRaises(RuntimeError):self.validate(e,v,s,directory)

    def test_departure_intermediate_canonical_and_authority_are_coherent(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        for kind in ('missing-row','other-row','duplicate-row','archived','attempt','delivery','result','decision','cooldown','owner'):
            e,v,s=receipt(directory,'departure');r=v['result'];saved=r['savedInventory']
            if kind=='missing-row':saved['drafts']=[]
            elif kind=='other-row':saved['drafts']=[dict(saved['active'],id='other')]
            elif kind=='duplicate-row':saved['drafts'].append(copy.deepcopy(saved['active']))
            elif kind=='archived':saved['active']['archived']=True
            elif kind=='attempt':saved['active']['attempt']='unexpected'
            elif kind=='delivery':saved['attempts']=[{'id':'unexpected'}]
            elif kind=='result':saved['lateResults']=[{'id':'unexpected'}]
            elif kind=='decision':saved['decisions']=[{'id':'unexpected'}]
            elif kind=='cooldown':saved['retryAt']=True
            else:saved['active']['owner']=dict(R['OWNER'],team='T2')
            r['captureRefusal']['inventory']=copy.deepcopy(saved)
            with self.assertRaises(RuntimeError):self.validate(e,v,s,directory)

    def test_departure_rejects_repeated_or_bypassed_shortcut(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        for kind in ('event','generation','palette','visit','pending'):
            e,v,s=receipt(directory,'departure')
            r=v['result']
            if kind=='event':r['shortcuts'][0]['prevented']=False
            elif kind=='generation':r['afterGeneration']+=1
            elif kind=='palette':r['palette']['canvasAbsent']=False
            elif kind=='visit':r['returned']['visit']='first'
            else:r['captureRefusal']['pending']=True
            with self.assertRaises(RuntimeError):self.validate(e,v,s,directory)

    def test_departure_rejects_lost_refused_input_or_storage_custody(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        for kind in ('native','canonical','retained','retry','provisional','locked'):
            e,v,s=receipt(directory,'departure')
            r=v['result']
            if kind=='native':r['saveRefusal']['native']['markdown']='lost'
            elif kind=='canonical':r['saveRefusal']['inventory']['active']['content']['markdown']=R['OURS']
            elif kind=='retained':r['saveRefusal']['retainedSlots']=0
            elif kind=='retry':r['saveRefusal']['stableInventory']['active']['revision']=9
            elif kind=='provisional':r['captureRefusal']['editorMarkdown']=R['COMPOSED']
            else:r['captureRefusal']['native']['bodyReadonly']=True
            with self.assertRaises(RuntimeError):self.validate(e,v,s,directory)

    def test_known_unsent_is_prepared_but_not_executed(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        e,v,s=receipt(directory,'known-unsent-conflict')
        self.validate(e,v,s,directory)
        v['requests'][0]['executed']=True
        with self.assertRaisesRegex(RuntimeError,'dispatch policy'):
            self.validate(e,v,s,directory)

    def test_changed_raw_body_and_duplicate_xparams_refused(self):
        value=request(R['BODY'])
        R['validate_request'](value,R['BODY'],False)
        value['form'][2][1]=R['BODY'].strip()
        with self.assertRaisesRegex(RuntimeError,'ordered content'):
            R['validate_request'](value,R['BODY'],False)
        value=request(R['BODY']);value['url']+='&_x_num_retries=0'
        with self.assertRaisesRegex(RuntimeError,'xparams keys'):
            R['validate_request'](value,R['BODY'],False)

    def test_endpoint_and_replay_flags_refused(self):
        for change in (dict(url='https://slack.com/api/files.createCanvas'),dict(retrySafe=True)):
            value=request(R['BODY']);value.update(change)
            with self.assertRaises(RuntimeError):R['validate_request'](value,R['BODY'],False)

    def test_ready_owner_phase_boolean_and_nonce_refused(self):
        e=R['ready_tuple'](R['FIXTURES'][0],'a'*32,123,'editing','exercise')
        for key,value in [('pid',True),('version',True),('phase','restart'),('nonce','b'*32),('fixture','canvas')]:
            wrong=dict(e);wrong[key]=value
            with self.assertRaises(RuntimeError):R['validate_ready'](wrong,e)

    def test_marker_is_create_new(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp);e=R['ready_tuple'](R['FIXTURES'][0],'a'*32,123,'editing','exercise')
            R['publish_ready'](path,e)
            with self.assertRaises(FileExistsError):R['publish_ready'](path,e)
            self.assertEqual(json.loads((path/'ready-exercise.json').read_text()),e)
            self.assertFalse((path/'ready-exercise.staged').exists())

    def test_capture_order_extra_path_and_bool_count_refused(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        for kind in ('order','path','count'):
            e,v,s=receipt(directory)
            if kind=='order':s=list(reversed(s))
            elif kind=='path':v['captures']['initial']='/other.png'
            else:v['preparedRequests']=False
            with self.assertRaises(RuntimeError):self.validate(e,v,s,directory)

    def test_unicode_damage_not_self_referential(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        e,v,s=receipt(directory)
        v['result']['native']['codePoints']=[63]*len(v['result']['native']['codePoints'])
        with self.assertRaisesRegex(RuntimeError,'Unicode'):
            self.validate(e,v,s,directory)
        self.assertIn(0x1f680,list(map(ord,R['COMPOSED'])))
        self.assertIn(0x301,list(map(ord,R['COMPOSED'])))
        self.assertIn(0x65e5,list(map(ord,R['COMPOSED'])))

    def test_changed_owner_or_canonical_attempt_refused(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        e,v,s=receipt(directory)
        v=copy.deepcopy(v);v['inventory']['active']['owner']['user']='U_OTHER'
        with self.assertRaises(RuntimeError):self.validate(e,v,s,directory)

    def test_closed_log_late_duplicate_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory=Path(tmp);e,v,s=receipt(directory);path=directory/'log'
            def write(extra=''):
                path.write_text(R['WAITING']+json.dumps(e)+'\n'+''.join(R['STAGE']+json.dumps(x)+'\n' for x in s)+R['PREFIX']+json.dumps(v)+'\n'+extra,encoding='utf-8')
            write()
            R['validate_closed_log'](path,e,directory,v,png=False,platform='win32')
            write(R['PREFIX']+json.dumps(v)+'\n')
            with self.assertRaisesRegex(RuntimeError,'extra/missing'):
                R['validate_closed_log'](path,e,directory,v,png=False,platform='win32')

    def test_complete_dictionary_comparison_includes_dispositions(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        _,v,_=receipt(directory)
        db={table:[] for table in R['TABLES']}
        db['drafts']=[('draft',json.dumps(v['inventory']['active']))]
        db['scopes']=[('T1/U00000001',json.dumps(dict(owner=R['OWNER'],active='draft',retry_at=0)))]
        R['validate_snapshot'](db,v)
        db['dispositions']=[('unexpected','{}')]
        with self.assertRaisesRegex(RuntimeError,'disposition'):
            R['validate_snapshot'](db,v)

    def test_readonly_review_disclosures_are_required(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        e,v,s=receipt(directory,'known-unsent-conflict')
        v['result']['reviewedNative']['canonical']['markdown']='Wrong version'
        with self.assertRaisesRegex(RuntimeError,'disclosure'):
            self.validate(e,v,s,directory)

    def test_late_result_both_phases_require_locked_model_and_dom(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        for phase in ('exercise','restart'):
            e,v,s=receipt(directory,'late-result',phase)
            self.validate(e,v,s,directory)
            for key in ('titleReadonly','bodyReadonly','titleDisabled','bodyDisabled'):
                wrong=copy.deepcopy(v);wrong['result']['native'][key]=False
                with self.assertRaisesRegex(RuntimeError,'DOM editability'):self.validate(e,wrong,s,directory)
            wrong=copy.deepcopy(v);wrong['model']['uncertain']=False
            with self.assertRaisesRegex(RuntimeError,'model delivery'):self.validate(e,wrong,s,directory)

    def test_geometry_recomputes_bounds_and_rejects_nonfinite(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        e,v,s=receipt(directory)
        for bad in (float('nan'),float('inf'),-1,1281):
            wrong=copy.deepcopy(v);wrong['geometry'][0]['controls'][0]['rect']['left']=bad
            with self.assertRaises(RuntimeError):self.validate(e,wrong,s,directory)
        wrong=copy.deepcopy(v);wrong['geometry'][0]['scrollWidth']=1000
        with self.assertRaisesRegex(RuntimeError,'overflow'):self.validate(e,wrong,s,directory)
        wrong=copy.deepcopy(v);wrong['geometry'][0]=dict(ok=True,noOverflow=True,footerHitTested=True)
        with self.assertRaises(RuntimeError):self.validate(e,wrong,s,directory)

    def test_required_feedback_rejects_absence_changed_copy_or_role(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        for scenario,phase,stage in (('editing','exercise','composing'),('departure','exercise','save-refused'),
                                     ('late-result','exercise','uncertain'),('late-result','restart','restart')):
            for kind in ('absent','empty','wrong-copy','wrong-role'):
                e,v,s=receipt(directory,scenario,phase)
                stages=('restart',) if phase=='restart' else tuple(x for x in R['STAGES'][scenario] if x!='closed')
                feedback=v['geometry'][stages.index(stage)]['feedback']
                if kind=='absent':feedback.update(region=None,messages=[])
                elif kind=='empty':feedback['messages']=[]
                elif kind=='wrong-copy':feedback['messages'][0]['text']='Unrelated status'
                else:feedback['messages'][0]['role']='status'
                with self.assertRaises(RuntimeError):self.validate(e,v,s,directory)

    def test_feedback_clipping_obscuring_and_footer_overlap_are_rejected(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        for kind in ('hidden','clipped','outside-region','under-footer','over-editor','outside-viewport'):
            e,v,s=receipt(directory,'departure')
            geometry=v['geometry'][R['STAGES']['departure'].index('capture-refused')]
            feedback=geometry['feedback'];message=feedback['messages'][0]
            if kind=='hidden':message['visible']=False
            elif kind=='clipped':message['unclipped']=False
            elif kind=='outside-region':message['rect'].update(top=550,bottom=568)
            elif kind=='under-footer':feedback['region'].update(top=620,bottom=680)
            elif kind=='over-editor':feedback['region'].update(top=490,bottom=550)
            else:message['rect'].update(top=775,bottom=793)
            with self.assertRaises(RuntimeError):self.validate(e,v,s,directory)

    def test_exact_review_disposition_identity(self):
        directory=Path('/private/canvas-creation-'+'a'*32)
        _,v,_=receipt(directory,'known-unsent-conflict')
        db={table:[] for table in R['TABLES']}
        db['drafts']=[('draft',json.dumps(v['inventory']['active']))]
        db['scopes']=[('T1/U00000001',json.dumps(dict(owner=R['OWNER'],active='draft',retry_at=0)))]
        conflict=v['result']['reviewedInventory']['conflicts'][0]
        disposition={k:conflict[k] for k in ('id','owner','draft_id','base_revision','writer')}
        db['dispositions']=[('conflict',json.dumps(disposition))]
        R['validate_snapshot'](db,v)
        for field,value in [('id','other'),('draft_id','other'),('base_revision',3),('writer','other')]:
            wrong=dict(disposition);wrong[field]=value
            db['dispositions']=[('conflict',json.dumps(wrong))]
            with self.assertRaisesRegex(RuntimeError,'exact reviewed'):R['validate_snapshot'](db,v)

    def test_primary_error_survives_cleanup_and_receipt_failures(self):
        G=R['run_phase'].__globals__;observed={}
        class Process:
            pid=123
            code=None
            def poll(self):return self.code
        class Native:
            def __init__(self,*args):self.process=Process();self.log=Path('/log')
            def __enter__(self):return self
            def __exit__(self,*args):self.process.code=-9;raise OSError('shutdown failure')
            def require_running(self):pass
            def receipts(self,prefix):return [R['ready_tuple'](R['FIXTURES'][0],'a'*32,123,'editing','exercise')] if prefix==R['WAITING'] else []
            def capture(self,path):raise OSError('capture failed')
        class API:
            def __init__(self):self.kernel=self;self.calls=0
            def open(self,pid):return 1
            def times(self,handle):
                self.calls+=1
                return dict(creation_filetime=10,exit_filetime=0 if self.calls==1 else 20)
            def CloseHandle(self,handle):observed['closed']=True
        def normalize(native,evidence,*args):
            evidence['pointer']=dict(movement_observed=True,restore_observed=False,restore_attempted=False)
            raise ValueError('primary workflow')
        publish=G['publish_evidence']
        def record(path,evidence):publish(path,evidence);observed['receipt']=copy.deepcopy(evidence)
        with patch.dict(G,NativeFixture=Native,publish_evidence=record,sys=SimpleNamespace(platform="win32")),patch.dict(G['H'],binary_sha=lambda p:'digest',normalize=normalize), \
             patch('windows_process_metrics.WindowsAPI',API),patch.object(Path,'write_text',side_effect=OSError('write failure')), \
             patch('builtins.print',side_effect=OSError('print failure')):
            with self.assertRaisesRegex(ValueError,'primary workflow'):
                R['run_phase'](R['FIXTURES'][0],'editing','exercise','a'*32,Path('/private'),Path('/output'),Path('/binary'),'digest')
        value=observed['receipt']
        self.assertTrue(observed['closed']);self.assertTrue(value['processExited']);self.assertTrue(value['exitIdentityValidated'])
        self.assertFalse(value['ok']);self.assertIn('primary workflow',value['error'])
        self.assertTrue(any('shutdown failure' in e for e in value['cleanupErrors']))
        self.assertTrue(any('Pointer' in e for e in value['cleanupErrors']))
        self.assertEqual(len(value['receiptErrors']),2)

    def test_pointer_cleanup_failure_is_not_success(self):
        with self.assertRaises(RuntimeError):
            R['H']['validate_pointer_cleanup'](dict(movement_observed=True,restore_observed=False,restore_attempted=False))


if __name__=='__main__':unittest.main()
