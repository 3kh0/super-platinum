"""Pure admission regressions; no app, network, window or process is launched."""
import copy
import json
from pathlib import Path
import runpy
import re
import subprocess
import unittest

H=runpy.run_path(str(Path(__file__).with_name('check-profile-group-recovery-native.py')))
CAPTURES=Path('tmp/pure-group-review').resolve()
G,O=H['GROUP'],H['OTHER']

def rect(): return dict(left=10,top=10,right=110,bottom=30,width=100,height=20)

def geometry(stage,case,viewport):
    replaced=stage=='account-replaced';editor=stage in ('success','reopened')
    waiting=stage in ('ordinary-open','held','malformed','offline','partial','fallback-held','source-refused')
    loading=stage in ('held','fallback-held')
    controls=[] if replaced else ['Back','Save section']+(['Use description','Cancel description edit'] if editor else
        ['Retry description','Cancel description edit'] if waiting else ['Edit description'])
    notices=[dict(text=H['LOADING'] if loading else H['GUIDANCE'],visible=True,rect=rect())] if waiting else []
    if stage=='saved':notices.append(dict(text='Profile fields saved.',visible=True,rect=rect()))
    labels=[H['LABEL'],H['LABEL']]+([] if case=='legacy' else [H['OTHER_LABEL']])
    tokens=[dict(text=label,editable=False,visible=True,rect=rect(),fragments=[rect()]) for label in labels] if editor else []
    captured=dict(snapshot=dict(blocks=H['blocks'](case),composing=False,revision=1,generation='owned'),
                  tokens=[dict(value=t,label=l) for t,l in zip(H['blocks'](case)[0]['elements'][0]['elements'][1:],labels)]) if editor else None
    return dict(ok=True,noOverflow=True,viewport=viewport,dialog=not replaced,dialogRect=None if replaced else rect(),
                controls=[dict(text=c,visible=True,rect=rect(),disabled=(c=='Save section' and (editor or waiting or stage in ('initial','cancelled','saved')))
                               or (c=='Retry description' and loading)) for c in controls],notices=notices,tokens=tokens,
                feedbackRequired=waiting,feedbackVisible=waiting,globalToast=None,captured=captured,
                richEditable=True if editor else None,pngDimensions=[1280,780])

def ledger(case):
    rows=[]
    def add(kind,method,body):
        rows.append(dict(serial=len(rows)+1,kind=kind,method=method,body=body,account=H['ACCOUNT'],team='TINIT0001',epoch=0,released=True))
    for kind,groups in H['expected_reads'](case):
        if kind=='directory':
            add('discovery','api.getFlannelHttpUrl',{'include_external_workspaces':'true','_x_reason':'client-needs-valid-flannel-url'})
            add(kind,'usergroups/info',dict(ids=groups))
        else:
            add(kind,'subteams.info',dict(usergroups=','.join(groups),include_users='false',include_count='true',include_disabled='true',
                                        _x_reason='fallback:fetchRawUserGroupsById'))
    return rows

def group(source=None,legacy=False):
    if source is None:
        native=dict(id=G,handle='Legacy name must not win',users=['UINIT0001','UKEEP001'],members_loaded=True,label_source=None) if legacy else None
        return dict(native=native,source=None,label=None)
    native=dict(id=source['id'],label_source=copy.deepcopy(source),users=['UINIT0001','UKEEP001'],members_loaded=True)
    label=H['LABEL'] if source['id']==G else H['OTHER_LABEL']
    if source.get('handle')=='Socket crew':label='@Socket crew'
    return dict(native=native,source=source,label=label)

def receipt(case='legacy',fixture=None):
    fixture=fixture or H['FIXTURES'][0];viewport=H['expected_viewport'](fixture,'win32')
    result=dict(ok=True,fixture=fixture,case=case,viewport=viewport,captureChecksPassed=True,requests=ledger(case),stages=[],captures={})
    counts={'initial':(0,0),'ordinary-open':(0,0),'held':(1,0),'malformed':(2,1),'offline':(4,2),
            'partial':(1,1),'fallback-held':(3,1),'cancelled':(1,1),'source-refused':(2,2),'account-replaced':(3,3),
            **{s:((5,3) if case=='legacy' else (3,2)) for s in ('success','saved','reopened')}}
    proofkeys=('originalDraft','noForcedRead','explicitRetryRead','duplicateIdsCoalesced','sourceRetained','draftRetained','actualFallback',
        'actualPartialRetained','noPlaceholder','onlyUnresolvedRequested','failedIdsFallback','actualReadAndAdoption','membershipPreserved',
        'actualProfileSaveReadback','rawDuplicatesPreserved','noRedundantRead','lateResultDidNotOpen','actualGroupEvent','oldSourceResultRefused','actualInstallLoaded')
    for stage in H['STAGES'][case]:
        replaced=stage=='account-replaced';held=stage in ('held','fallback-held');saved=stage in ('saved','reopened')
        read_count,finished=counts[stage]
        selected=[];seen=0
        for row in result['requests']:
            if seen==read_count:break
            selected.append(copy.deepcopy(row));seen+=row['kind']!='discovery'
        if held:selected[-1]['released']=False
        groups={g:group(legacy=case=='legacy') for g in H['ids'](case)}
        if stage in ('success','saved','reopened'):groups={g:group(H['raw'](g)) for g in H['ids'](case)}
        if case=='absent' and stage in ('partial','fallback-held'):groups[G]=group(H['raw'](G))
        if stage=='source-refused':
            changed=H['raw'](G);changed['handle']='Socket crew';groups[G]=group(changed)
        if replaced:groups={g:group() for g in H['ids'](case)}
        profile=dict(enabled=not replaced,blocks=H['blocks'](case),dirty=not saved,reads=int(saved),choiceValue='Morning',countryValue='US',
                     attempts=[[dict(element_id='long',long_text=dict(blocks=json.dumps(H['blocks'](case))))]] if saved else [])
        model=dict(profile=profile,account=H['FOREIGN'] if replaced else H['ACCOUNT'],epoch=int(replaced),groups=groups,
                   requests=selected,pending=selected[-1:] if held else [],driver=dict(started=finished+held,finished=finished,active=int(held)))
        proof=dict.fromkeys(proofkeys,True)
        if replaced:proof['before']=dict(profile=dict(blocks=H['blocks'](case)))
        result['stages'].append(dict(stage=stage,model=model,geometry=geometry(stage,case,viewport),proof=proof))
        result['captures'][stage]=str(CAPTURES/f'{fixture}-{stage}.png')
    return result

def stage(value,name):return next(s for s in value['stages'] if s['stage']==name)

# Actual A terminal initial-stage measurement; source log SHA256 7711740182bc3b8246a5c64a0acebd243687cf2e08a01612a616dadfe79d8480
ACTUAL_INITIAL = json.loads(r'''{"case":"legacy","stage":"initial","canSave":false,"geometry":{"captured":null,"choiceValue":"Morning","controls":[{"disabled":false,"rect":{"bottom":584.6458740234375,"height":32,"left":440.6666870117187,"right":552.1250228881836,"top":552.6458740234375,"width":111.45833587646484},"text":"Edit description","visible":true},{"disabled":false,"rect":{"bottom":755.3333740234375,"height":32,"left":685.25,"right":737.375,"top":723.3333740234375,"width":52.125},"text":"Back","visible":true},{"disabled":true,"rect":{"bottom":755.3333740234375,"height":32,"left":745.375,"right":839.3333358764648,"top":723.3333740234375,"width":93.95833587646484},"text":"Save section","visible":true}],"countryValue":"US","dialog":true,"dialogRect":{"bottom":768,"height":756,"left":420,"right":860,"top":12,"width":440},"feedbackRequired":false,"feedbackVisible":false,"globalToast":null,"longDisabled":false,"managedFieldPreserved":true,"noOverflow":true,"notices":[{"rect":{"bottom":606.6458740234375,"height":18,"left":440.6666870117187,"right":829.3333740234375,"top":588.6458740234375,"width":388.6666870117187},"text":"Check this field's value or length.","visible":true}],"ok":true,"pngDimensions":[1942,1256],"richEditable":null,"tokens":[],"viewport":[1280,780]}}''')

class Admission(unittest.TestCase):
    def reject(self,mutate,case='legacy'):
        value=receipt(case);mutate(value)
        with self.assertRaises((RuntimeError,ValueError,TypeError,KeyError)):
            H['validate'](value['fixture'],value,CAPTURES,platform='win32',png=False)
    def test_all_cases_and_widths(self):
        for case in H['STAGES']:
            for fixture in H['FIXTURES']:
                value=receipt(case,fixture);H['validate'](fixture,value,CAPTURES,platform='win32',png=False)
    def test_actual_initial_unavailable_draft_disables_save(self):
        actual=copy.deepcopy(ACTUAL_INITIAL)
        self.assertIs(actual['canSave'],False)
        H['layout'](actual['geometry'],[1280,780],actual['stage'],actual['case'])
        next(c for c in actual['geometry']['controls'] if c['text']=='Save section')['disabled']=False
        with self.assertRaisesRegex(RuntimeError,'Wrong action disabled state'):
            H['layout'](actual['geometry'],[1280,780],actual['stage'],actual['case'])
    def test_cancelled_unavailable_draft_disables_save(self):
        self.reject(lambda r:next(c for c in stage(r,'cancelled')['geometry']['controls'] if c['text']=='Save section').update(disabled=False),case='retirement')
    def test_initial_edit_and_back_remain_enabled(self):
        for text in ('Edit description','Back'):
            self.reject(lambda r:next(c for c in stage(r,'initial')['geometry']['controls'] if c['text']==text).update(disabled=True))
    def test_terminal_failure_detail(self):
        with self.assertRaisesRegex(RuntimeError,'specific stage failure'):
            H['validate'](H['FIXTURES'][0],dict(ok=False,error='specific stage failure'),CAPTURES,png=False)
    def test_stage_omission(self):self.reject(lambda r:r['stages'].pop())
    def test_clipped_guidance(self):self.reject(lambda r:stage(r,'malformed')['geometry']['notices'][0]['rect'].update(bottom=900,height=890))
    def test_hidden_feedback(self):self.reject(lambda r:stage(r,'offline')['geometry']['notices'][0].update(visible=False))
    def test_exact_loading_copy(self):self.reject(lambda r:stage(r,'held')['geometry']['notices'][0].update(text='Loading'))
    def test_retry_still_enabled(self):self.reject(lambda r:next(c for c in stage(r,'held')['geometry']['controls'] if c['text']=='Retry description').update(disabled=False))
    def test_directory_extra_flags(self):self.reject(lambda r:r['requests'][1]['body'].update(check_interaction=True))
    def test_directory_duplicates(self):self.reject(lambda r:r['requests'][1]['body']['ids'].append(G))
    def test_wrong_reason_key(self):self.reject(lambda r:r['requests'][2]['body'].update(reason='fallback:fetchRawUserGroupsById'))
    def test_fallback_without_failed_subset(self):self.reject(lambda r:r['requests'][-1]['body'].update(usergroups=G),case='absent')
    def test_partial_fabricated_other(self):self.reject(lambda r:stage(r,'partial')['model']['groups'].__setitem__(O,group(H['raw'](O))),case='absent')
    def test_partial_not_adopted(self):self.reject(lambda r:stage(r,'partial')['model']['groups'].__setitem__(G,group()),case='absent')
    def test_legacy_fabricated_witness(self):self.reject(lambda r:stage(r,'offline')['model']['groups'].__setitem__(G,group(H['raw'](G))))
    def test_membership_erased(self):self.reject(lambda r:stage(r,'success')['model']['groups'][G]['native'].update(users=[]))
    def test_wrong_label_whitespace(self):self.reject(lambda r:stage(r,'success')['geometry']['tokens'][0].update(text=H['LABEL'].strip()))
    def test_opaque_duplicate_lost(self):self.reject(lambda r:stage(r,'success')['geometry']['captured']['snapshot']['blocks'][0]['elements'][0]['elements'].pop())
    def test_raw_source_lost(self):self.reject(lambda r:stage(r,'success')['model']['groups'][G]['source'].pop('fixture_metadata'))
    def test_wrapped_fragment_clipped(self):self.reject(lambda r:stage(r,'success')['geometry']['tokens'][0]['fragments'][0].update(bottom=800,height=790))
    def test_save_wire_changed(self):self.reject(lambda r:stage(r,'saved')['model']['profile']['attempts'][0][0]['long_text'].update(blocks='[]'))
    def test_no_readback(self):self.reject(lambda r:stage(r,'saved')['model']['profile'].update(reads=0))
    def test_cache_reopen_extra_read(self):self.reject(lambda r:stage(r,'reopened')['model']['requests'].append(copy.deepcopy(r['requests'][-1])))
    def test_cancel_late_adoption(self):self.reject(lambda r:stage(r,'cancelled')['model']['groups'].__setitem__(G,group(H['raw'](G))),case='retirement')
    def test_source_event_overwritten(self):self.reject(lambda r:stage(r,'source-refused')['model']['groups'].__setitem__(G,group(H['raw'](G))),case='retirement')
    def test_foreign_account_adoption(self):self.reject(lambda r:stage(r,'account-replaced')['model']['groups'].__setitem__(G,group(H['raw'](G))),case='retirement')
    def test_actual_replacement_missing(self):self.reject(lambda r:stage(r,'account-replaced')['proof'].update(actualInstallLoaded=False),case='retirement')
    def test_settled_before_driver_completion(self):self.reject(lambda r:stage(r,'malformed')['model']['driver'].update(active=1))
    def test_missing_proof(self):self.reject(lambda r:stage(r,'held').update(proof={}))
    def test_capture_escape(self):self.reject(lambda r:r['captures'].update(initial='/outside.png'))
    def test_wrong_readiness_identity(self):
        wanted=H['ready_tuple'](H['FIXTURES'][0],'a'*32,123)
        H['validate_ready'](wanted,wanted)
        for key,value in [('pid',124),('nonce','b'*32),('fixture',H['FIXTURES'][1])]:
            actual=dict(wanted);actual[key]=value
            with self.assertRaises(RuntimeError):H['validate_ready'](actual,wanted)
    def test_embedded_editor_comparison_ignores_object_key_order(self):
        source=(Path(__file__).resolve().parents[1]/'src/desktop/profile_edit/sections/fixture/group_recovery_check/native.rs').read_text()
        expression=re.search(r'const stable=(.*?);return',source).group(1)
        script='const stable='+expression+';'+'''const a={type:'usergroup',opaque:{keep:true},usergroup_id:'SMISSING01'};
const b={usergroup_id:'SMISSING01',opaque:{keep:true},type:'usergroup'};
if(stable(a)!==stable(b))throw Error('key order mismatch');
if(stable([a,a])===stable([a]))throw Error('duplicate token lost');
if(stable(a)===stable({...b,opaque:{keep:false}}))throw Error('opaque mutation hidden');'''
        result=subprocess.run(['node','-e',script],capture_output=True,text=True,timeout=10)
        self.assertEqual(result.returncode,0,result.stderr)

if __name__=='__main__':unittest.main()
