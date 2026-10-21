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
    replaced=stage=='account-replaced';editor=stage in ('success','edited','reopened')
    waiting=stage in ('ordinary-open','held','malformed','offline','pending','partial','fallback-held','source-refused',
                     'direct-held','limited','reopened-limited','expired','file-held','group-held','retry-file-held')
    loading=stage in ('held','fallback-held','direct-held','file-held','group-held','retry-file-held')
    controls=[] if replaced else ['Back','Save section']+(['Use description','Cancel description edit'] if editor else
        ['Retry description','Cancel description edit'] if waiting else ['Edit description'])
    notice=H['LOADING'] if loading else H['LIMITED'] if stage in ('limited','reopened-limited') else H['RETRY_READY'] if stage=='expired' else H['GUIDANCE']
    if case=='mixed-file-group': notice=H['MIXED_LOADING'] if loading else H['MIXED_GUIDANCE']
    notices=[dict(text=notice,visible=True,rect=rect())] if waiting else []
    if stage=='saved':notices.append(dict(text='Profile fields saved.',visible=True,rect=rect()))
    labels=[H['LABEL'],H['LABEL']]+([] if case=='legacy' else [H['OTHER_LABEL']])
    if case in ('mixed-member','mixed-file-group'): labels.append(H['MEMBER_LABEL'])
    if case=='mixed-file-group': labels.extend(['List & Plan']*2)
    tokens=[dict(text=label,editable=False,visible=True,rect=rect(),fragments=[rect()]) for label in labels] if editor else []
    if case=='mixed-file-group' and editor:
        tokens[-2].update(fontWeight='700',fontStyle='normal')
        tokens[-1].update(fontWeight='400',fontStyle='italic')
    captured=dict(snapshot=dict(blocks=H['blocks'](case,case in H['AUTOMATIC'] and stage in ('edited','reopened')),composing=False,revision=2 if stage=='edited' else 1,generation='owned'),
                  tokens=[dict(value=t,label=l) for t,l in zip(H['blocks'](case)[0]['elements'][0]['elements'][1:],labels)]) if editor else None
    return dict(ok=True,noOverflow=True,viewport=viewport,dialog=not replaced,dialogRect=None if replaced else rect(),
                controls=[dict(text=c,visible=True,rect=rect(),disabled=(c=='Save section' and (editor or waiting or stage in ('initial','cancelled','saved')))
                               or (c=='Retry description' and (loading or stage in ('limited','reopened-limited')))) for c in controls],notices=notices,tokens=tokens,
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
    if case=='retirement':
        for i,row in enumerate(r for r in rows if r['kind']!='discovery'):
            row.update(receiverRetired=i in (0,2),delivered=i==1)
    if case=='rate-limit':
        for i,row in enumerate(r for r in rows if r['kind']!='discovery'):
            row.update(receiverRetired=False,delivered=True,outcome='response' if i==2 else 'rate-limited')
            if i<2:row['retryAfterSeconds']=(8,15)[i]
    if case=='mixed-file-group':
        for i,row in enumerate(r for r in rows if r['kind']!='discovery'):
            row.update(receiverRetired=False,delivered=True,outcome='response' if i==2 else 'offline')
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
    if case in H['AUTOMATIC']:
        total={'automatic':(1,1),'pending':(2,2),'automatic-offline':(3,2),
               'automatic-malformed':(3,2),'ready':(0,0),'mixed-member':(1,1),'rate-limit':(3,2),'mixed-file-group':(3,2)}[case]
        counts.update(pending=(1,1),offline=(2,1),malformed=(2,1))
        counts.update({s:total for s in ('success','edited','saved','reopened')})
        if case=='rate-limit':
            counts.update({'direct-held':(2,0),**{s:(2,1) for s in ('limited','cancelled','reopened-limited','expired')}})
    if case=='mixed-file-group': counts.update({'file-held':(0,0),'group-held':(1,0),'retry-file-held':(2,1)})
    proofkeys=('originalDraft','noForcedRead','explicitRetryRead','duplicateIdsCoalesced','sourceRetained','draftRetained','actualFallback',
        'actualPartialRetained','noPlaceholder','onlyUnresolvedRequested','failedIdsFallback','actualReadAndAdoption','membershipPreserved',
        'actualProfileSaveReadback','rawDuplicatesPreserved','noRedundantRead','lateResultDidNotOpen','actualGroupEvent','oldSourceResultRefused','actualInstallLoaded')
    for stage in H['STAGES'][case]:
        replaced=stage=='account-replaced';held=stage in ('held','fallback-held','direct-held','group-held');saved=stage in ('saved','reopened')
        read_count,finished=counts[stage]
        selected=[];seen=0
        for row in result['requests']:
            if seen==read_count:break
            selected.append(copy.deepcopy(row));seen+=row['kind']!='discovery'
        if held:selected[-1]['released']=False
        if held:
            selected[-1].pop('receiverRetired',None)
            selected[-1].pop('delivered',None)
            selected[-1].pop('outcome',None)
            selected[-1].pop('retryAfterSeconds',None)
        groups={g:group(legacy=case=='legacy') for g in H['ids'](case)}
        if stage in ('success','edited','saved','reopened') or case=='ready':groups={g:group(H['raw'](g)) for g in H['ids'](case)}
        if case=='absent' and stage in ('partial','fallback-held'):groups[G]=group(H['raw'](G))
        if stage=='source-refused':
            changed=H['raw'](G);changed['handle']='Socket crew';groups[G]=group(changed)
        if replaced:groups={g:group() for g in H['ids'](case)}
        expected=H['blocks'](case,case in H['AUTOMATIC'] and saved)
        profile=dict(enabled=not replaced,blocks=expected,dirty=not saved,reads=int(saved),choiceValue='Morning',countryValue='US',
                     attempts=[[dict(element_id='long',long_text=dict(blocks=json.dumps(expected)))]] if saved else [])
        model=dict(profile=profile,account=H['FOREIGN'] if replaced else H['ACCOUNT'],epoch=int(replaced),groups=groups,
                   requests=selected,pending=selected[-1:] if held else [],driver=dict(started=finished+held,finished=finished,active=int(held)))
        if case in ('mixed-member','mixed-file-group'):
            model['mixedMember']=dict(observation=dict(source=H['member_source'](),native=H['member_raw'](),current=True),
                absentDriver=dict(started=0,finished=0,active=0),knownDriver=dict(started=0,finished=0,active=0),
                automaticDriver=dict(pending=[],subscribed=[],stale=[],worker=False,started=0,
                    finished=0,http_finished=0,acknowledgments=0,failures=0))
        proof=dict.fromkeys(proofkeys,True)
        if case!='legacy':
            proof.update(explicitRetryRead=False,automaticRead=True)
        if case in H['AUTOMATIC']:
            proof.update(readyWithoutRead=case=='ready',noAutomaticLoop=True,duplicateOpen=True,
                         quietObservationMs=500,pendingOnly=case=='pending',actualNativeEdit=True)
        if case=='rate-limit':
            first=stage not in ('initial','held')
            second=stage not in ('initial','held','direct-held')
            cooling=stage in ('direct-held','limited','cancelled','reopened-limited')
            model['rate']=dict(scope=dict(account=H['ACCOUNT'],team='TINIT0001',user='UINIT0001'),
                directory=dict(revision=1,retryAfterSecs=8,blocked=cooling) if first else None,
                direct=dict(revision=2,retryAfterSecs=15,blocked=True) if second else None,
                blocked=second and cooling,observed=first)
            proof.update(independentFallback=True,directoryRequest=2,directoryRetryAfterSeconds=8,
                noDispatch=True,debtRetained=True,disabledRetry=dict(ok=True,disabled=True,clicks=2),
                realExpiry=True,noAutomaticRequest=True,explicitRetry=True,
                elapsed=dict(directoryElapsedMs=8500,directElapsedMs=8300))
        if case=='mixed-file-group':
            from profile_group_mixed_file import file_source,file_cache
            fc={'initial':(0,0,False),'file-held':(1,0,True),'group-held':(1,1,False),
                'offline':(1,1,False),'retry-file-held':(2,1,True)}.get(stage,(2,2,False))
            model['mixedFile']=dict(source=file_source(*fc),cache=None if stage in ('initial','file-held') else file_cache(),
                opening=dict(visit=1,serial=1,key=0 if stage=='initial' else 1 if stage in ('file-held','group-held') else 2 if stage=='offline' else 3 if stage=='retry-file-held' else 4,
                             reading=stage in ('file-held','group-held','retry-file-held'),mounted=stage in ('success','edited','reopened')))
            proof.update(automaticRead=True,duplicateOpen=True,fileReady=True,openingRetained=True,
                         memberRetained=True,explicitRetryRead=stage=='retry-file-held',duplicateRetry=True,
                         noAutomaticLoop=True,quietObservationMs=500,
                         fileRelease=dict(delivered=True,cancelledBeforeReply=False))
        layout=geometry(stage,case,viewport)
        if stage=='edited':
            after=copy.deepcopy(layout['captured']);before=copy.deepcopy(after)
            before['snapshot'].update(blocks=H['blocks'](case),revision=1)
            proof['nativeEdit']=dict(ok=True,before=before,after=after)
        if replaced:proof['before']=dict(profile=dict(blocks=H['blocks'](case)))
        if case=='retirement' and stage in ('cancelled','account-replaced'):
            proof['retiredRelease']=dict(serial=selected[-1]['serial'],receiverRetired=True,delivered=False)
        result['stages'].append(dict(stage=stage,model=model,geometry=layout,proof=proof))
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
            if case in H['ABSENT_MEMBER_CASES'] or case in H['KNOWN_FRESHNESS_CASES']:
                continue  # Separate complete samples/corruption suite owns these receipts.
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
    def test_cancelled_receiver_must_refuse_release_not_claim_late_delivery(self):
        self.reject(lambda r:stage(r,'cancelled')['proof']['retiredRelease'].update(delivered=True),'retirement')
        self.reject(lambda r:stage(r,'cancelled')['model']['requests'][-1].update(receiverRetired=False),'retirement')
        self.reject(lambda r:r['requests'][1].update(delivered=True),'retirement')
        self.reject(lambda r:stage(r,'account-replaced')['proof']['retiredRelease'].update(serial=1),'retirement')
        self.reject(lambda r:stage(r,'account-replaced')['model']['requests'][-1].update(delivered=True),'retirement')
    def test_source_change_response_really_delivers_before_refusal(self):
        self.reject(lambda r:stage(r,'source-refused')['model']['requests'][-1].update(delivered=False),'retirement')
        self.reject(lambda r:stage(r,'source-refused')['model']['requests'][-1].update(receiverRetired=True),'retirement')
    def test_settled_before_driver_completion(self):self.reject(lambda r:stage(r,'malformed')['model']['driver'].update(active=1))
    def test_missing_proof(self):self.reject(lambda r:stage(r,'held').update(proof={}))
    def test_automatic_initial_open_never_requires_retry(self):
        for case in ('automatic','absent','retirement','pending','automatic-offline','automatic-malformed','mixed-member'):
            self.assertNotIn('ordinary-open',H['STAGES'][case])
            self.reject(lambda r:stage(r,'held')['proof'].update(automaticRead=False),case)
            self.reject(lambda r:stage(r,'held')['proof'].update(explicitRetryRead=True),case)
            self.reject(lambda r:stage(r,'held')['model']['requests'].append(copy.deepcopy(r['requests'][1])),case)
    def test_legacy_still_has_no_automatic_read(self):
        self.reject(lambda r:stage(r,'ordinary-open')['model']['requests'].append(copy.deepcopy(r['requests'][1])))
        self.assertEqual(H['STAGES']['legacy'][1],'ordinary-open')
    def test_no_loop_is_bounded_original_observation(self):
        for case,name in [('pending','pending'),('automatic-offline','offline'),('automatic-malformed','malformed')]:
            self.reject(lambda r:stage(r,name)['proof'].update(noAutomaticLoop=False),case)
            self.reject(lambda r:stage(r,name)['proof'].update(quietObservationMs=0),case)
            self.reject(lambda r:stage(r,name)['model']['driver'].update(active=1),case)
            self.reject(lambda r:stage(r,name)['model']['groups'].__setitem__(G,group(H['raw'](G))),case)
        self.reject(lambda r:stage(r,'held')['proof'].update(duplicateOpen=False),'automatic')
    def test_pending_only_never_causes_fallback_or_placeholder(self):
        self.assertEqual(H['expected_reads']('pending'),[('directory',[G,O]),('directory',[G,O])])
        self.reject(lambda r:stage(r,'pending')['proof'].update(pendingOnly=False),'pending')
        self.reject(lambda r:stage(r,'pending')['model']['groups'].__setitem__(O,group(H['raw'](O))),'pending')
    def test_actual_edit_unicode_and_preserved_tokens(self):
        self.assertIn('\u4e2d\U0001f98a',H['EDITED'])
        self.assertNotEqual(H['DRAFT'],H['EDITED'])
        self.reject(lambda r:stage(r,'edited')['proof'].update(actualNativeEdit=False),'automatic')
        self.reject(lambda r:stage(r,'edited')['proof']['nativeEdit']['after']['snapshot'].update(blocks=H['blocks']('automatic')),'automatic')
        self.reject(lambda r:stage(r,'edited')['proof']['nativeEdit']['after']['snapshot'].update(revision=1),'automatic')
        self.reject(lambda r:stage(r,'edited')['proof']['nativeEdit']['after']['snapshot'].update(generation='foreign'),'automatic')
        self.reject(lambda r:stage(r,'edited')['proof']['nativeEdit']['after']['tokens'][0]['value']['opaque'].update(keep=False),'automatic')
    def test_form_remains_original_until_use_but_saved_wire_is_native_edit(self):
        self.reject(lambda r:stage(r,'edited')['model']['profile'].update(blocks=H['blocks']('automatic',True)),'automatic')
        self.reject(lambda r:stage(r,'saved')['model']['profile']['attempts'][0][0]['long_text'].update(
            blocks=json.dumps(H['blocks']('automatic'))),'automatic')
        self.reject(lambda r:stage(r,'reopened')['geometry']['captured']['snapshot'].update(blocks=H['blocks']('automatic')),'automatic')
    def test_ready_original_canonical_groups_do_not_read(self):
        self.assertEqual(H['expected_reads']('ready'),[])
        self.reject(lambda r:stage(r,'initial')['model']['groups'].__setitem__(G,group()),'ready')
        self.reject(lambda r:stage(r,'success')['proof'].update(readyWithoutRead=False),'ready')
        self.reject(lambda r:stage(r,'success')['model']['requests'].append(ledger('automatic')[1]),'ready')
        self.reject(lambda r:next(c for c in stage(r,'initial')['geometry']['controls']
                                 if c['text']=='Save section').update(disabled=False),'ready')
    def test_mixed_member_is_original_canonical_without_read(self):
        self.reject(lambda r:stage(r,'held')['model']['mixedMember']['observation'].update(current=False),'mixed-member')
        self.reject(lambda r:stage(r,'held')['model']['mixedMember']['observation']['source']['profile'].update(display_name='guessed'),'mixed-member')
        for key in ('absentDriver','knownDriver','automaticDriver'):
            self.reject(lambda r:stage(r,'success')['model']['mixedMember'][key].update(started=1),'mixed-member')
        self.reject(lambda r:stage(r,'success')['geometry']['tokens'][-1].update(text='@username-must-not-win'),'mixed-member')
        self.reject(lambda r:stage(r,'edited')['proof']['nativeEdit']['after']['tokens'][-1]['value'].update(user_id=G),'mixed-member')
    def test_canonical_member_derived_fields_are_exact_not_ignored(self):
        # Mutate all stages together so baseline equality alone cannot hide a bad projection.
        for mutate in (
            lambda row:row.pop('_email_normalized_lc'),
            lambda row:row.update(_real_name_normalized_lc='real fallback'),
            lambda row:row['profile'].update(first_name='Known'),
            lambda row:row['profile'].pop('display_name_normalized'),
            lambda row:row.update(unobserved=True),
        ):
            self.reject(lambda r:[mutate(s['model']['mixedMember']['observation']['source'])
                                  for s in r['stages']],'mixed-member')
        self.reject(lambda r:[s['model']['mixedMember']['observation'].update(source=H['member_raw']())
                              for s in r['stages']],'mixed-member')
    def test_actual_mixed_member_failed_wrapper_receipt_retains_canonical_source(self):
        base=Path(__file__).resolve().parents[1]/'tmp/profile-absent-groups-20261020/integration/mixed-member-normal-a/mixed-member'
        log=base/'profile-group-recovery.log'
        if not log.exists():
            self.skipTest('Retained original native evidence is local, not checked in')
        receipts=H['H']['closed_receipts'](log,H['PREFIX'])
        self.assertEqual(len(receipts),1)
        value=receipts[0]
        first=value['stages'][0]['model']['mixedMember']
        self.assertTrue(all(s['model']['mixedMember']==first for s in value['stages']))
        self.assertNotEqual(first['observation']['source'],H['member_raw']())
        self.assertEqual(first['observation']['source'],H['member_source']())
        H['validate'](value['fixture'],value,base,platform='win32',png=True)
    def test_actual_ready_initial_metadata_does_not_grant_rich_save_context(self):
        base=Path(__file__).resolve().parents[1]/'tmp/profile-absent-groups-20261020/integration/ready-normal-a/ready'
        log=base/'profile-group-recovery.log'
        if not log.exists():
            self.skipTest('Retained original native evidence is local, not checked in')
        receipts=H['H']['closed_receipts'](log,H['PREFIX'])
        self.assertEqual(len(receipts),1)
        value=receipts[0]
        initial=value['stages'][0]
        self.assertIs(initial['model']['profile']['canSave'],False)
        self.assertIs(next(c for c in initial['geometry']['controls']
                           if c['text']=='Save section')['disabled'],True)
        self.assertEqual(value['requests'],[])
        H['validate'](value['fixture'],value,base,platform='win32',png=True)
    def test_capture_escape(self):self.reject(lambda r:r['captures'].update(initial='/outside.png'))
    def test_rate_original_headers_and_separate_receipts(self):
        for index in (1,2):
            self.reject(lambda r:r['requests'][index].update(retryAfterSeconds=None),'rate-limit')
            self.reject(lambda r:r['requests'][index].update(outcome='offline'),'rate-limit')
        self.reject(lambda r:stage(r,'limited')['model']['rate']['direct'].update(revision=1),'rate-limit')
    def test_rate_scope_does_not_adopt_successor(self):
        for key,value in [('account',H['FOREIGN']),('team','TFOREIGN'),('user','UOTHER')]:
            self.reject(lambda r:stage(r,'reopened-limited')['model']['rate']['scope'].update({key:value}),'rate-limit')
    def test_rate_cancel_reopen_cannot_clear_or_extend_debt(self):
        self.reject(lambda r:stage(r,'cancelled')['model']['rate'].update(direct=None),'rate-limit')
        self.reject(lambda r:stage(r,'reopened-limited')['model']['rate']['directory'].update(revision=3),'rate-limit')
        self.reject(lambda r:stage(r,'reopened-limited')['model']['rate']['direct'].update(retryAfterSecs=30),'rate-limit')
    def test_rate_expiry_requires_real_elapsed_and_independence(self):
        self.reject(lambda r:stage(r,'expired')['proof']['elapsed'].update(directoryElapsedMs=7999),'rate-limit')
        self.reject(lambda r:stage(r,'expired')['proof']['elapsed'].update(directElapsedMs=15000),'rate-limit')
        self.reject(lambda r:stage(r,'expired')['model']['rate']['direct'].update(blocked=False),'rate-limit')
        self.reject(lambda r:stage(r,'expired')['model']['rate'].update(blocked=True),'rate-limit')
    def test_rate_expiry_does_not_dispatch(self):
        self.reject(lambda r:stage(r,'expired')['model']['requests'].extend(r['requests'][-2:]),'rate-limit')
        self.reject(lambda r:stage(r,'reopened-limited')['model']['requests'][1].update(outcome='response'),'rate-limit')
        self.reject(lambda r:stage(r,'expired')['proof'].update(noAutomaticRequest=False),'rate-limit')
    def test_rate_disabled_and_ready_controls_are_actual(self):
        for name in ('limited','reopened-limited'):
            self.reject(lambda r:stage(r,name)['proof']['disabledRetry'].update(clicks=0),'rate-limit')
            self.reject(lambda r:next(c for c in stage(r,name)['geometry']['controls']
                         if c['text']=='Retry description').update(disabled=False),'rate-limit')
        self.reject(lambda r:next(c for c in stage(r,'expired')['geometry']['controls']
                     if c['text']=='Retry description').update(disabled=True),'rate-limit')
    def test_rate_feedback_is_readable_and_phase_specific(self):
        self.reject(lambda r:stage(r,'limited')['geometry']['notices'][0].update(text=H['GUIDANCE']),'rate-limit')
        self.reject(lambda r:stage(r,'expired')['geometry']['notices'][0].update(text=H['LIMITED']),'rate-limit')
        self.reject(lambda r:stage(r,'reopened-limited')['geometry']['notices'][0].update(visible=False),'rate-limit')
    def test_rate_retains_error_receipts_after_success(self):
        self.reject(lambda r:stage(r,'success')['model']['rate'].update(directory=None),'rate-limit')
        self.reject(lambda r:stage(r,'reopened')['model']['rate']['direct'].update(revision=99),'rate-limit')
        self.reject(lambda r:stage(r,'success')['model']['rate']['directory'].update(blocked=True),'rate-limit')
    def test_rate_permits_later_direct_expiry_without_new_request(self):
        value=receipt('rate-limit')
        for name in ('saved','reopened'):
            stage(value,name)['model']['rate']['direct']['blocked']=False
        H['validate'](value['fixture'],value,CAPTURES,platform='win32',png=False)
        stage(value,'reopened')['model']['rate']['direct']['blocked']=True
        with self.assertRaises(RuntimeError):H['validate'](value['fixture'],value,CAPTURES,platform='win32',png=False)
    def test_rate_native_edit_and_save_preserve_opaque_duplicates(self):
        self.reject(lambda r:stage(r,'edited')['proof']['nativeEdit']['after']['snapshot']['blocks'][0]
                    ['elements'][0]['elements'][1]['opaque'].update(keep=False),'rate-limit')
        self.reject(lambda r:stage(r,'saved')['model']['profile']['attempts'][0][0]['long_text'].update(
            blocks=json.dumps(H['blocks']('rate-limit'))),'rate-limit')
    def test_mixed_file_is_cold_until_actual_release(self):
        from profile_group_mixed_file import file_cache
        for name in ('initial','file-held'):
            self.reject(lambda r:stage(r,name)['model']['mixedFile'].update(cache=file_cache()),'mixed-file-group')
        self.reject(lambda r:stage(r,'group-held')['model']['mixedFile'].update(cache=None),'mixed-file-group')
    def test_mixed_file_exact_factory_and_counts(self):
        for field,value in [('file','FFOREIGN'),('page','2'),('count','250'),('truncate','false'),('extra','true')]:
            self.reject(lambda r:stage(r,'file-held')['model']['mixedFile']['source']['requests'][0]['args'].update({field:value}),'mixed-file-group')
        for field,value in [('completed',1),('held',False),('cancelled',1),('networkCalls',1)]:
            self.reject(lambda r:stage(r,'file-held')['model']['mixedFile']['source'].update({field:value}),'mixed-file-group')
    def test_mixed_retry_really_forces_second_file_read(self):
        self.reject(lambda r:stage(r,'retry-file-held')['model']['mixedFile']['source']['requests'].pop(),'mixed-file-group')
        self.reject(lambda r:stage(r,'retry-file-held')['proof'].update(duplicateRetry=False),'mixed-file-group')
        self.reject(lambda r:stage(r,'retry-file-held')['model']['mixedFile']['cache'].update(current=False),'mixed-file-group')
    def test_mixed_healthy_file_not_misidentified_by_guidance(self):
        for name,text in [('group-held','Loading document titles. Your description is unchanged.'),
                          ('offline',H['GUIDANCE']),('retry-file-held',H['LOADING'])]:
            self.reject(lambda r:stage(r,name)['geometry']['notices'][0].update(text=text),'mixed-file-group')
    def test_mixed_waits_for_all_sources_before_editor(self):
        for name in ('file-held','group-held','offline','retry-file-held'):
            self.reject(lambda r:stage(r,name)['model']['mixedFile']['opening'].update(mounted=True),'mixed-file-group')
            self.reject(lambda r:stage(r,name)['geometry'].update(captured=geometry('success','mixed-file-group',[1280,780])['captured']),'mixed-file-group')
    def test_mixed_handoff_keeps_original_opening(self):
        for name in ('group-held','offline'):
            for key in (('visit','serial','key') if name=='group-held' else ('visit','serial')):
                self.reject(lambda r:stage(r,name)['model']['mixedFile']['opening'].update({key:99}),'mixed-file-group')
        self.reject(lambda r:stage(r,'offline')['model']['mixedFile']['opening'].update(key=1),'mixed-file-group')
        self.reject(lambda r:stage(r,'retry-file-held')['model']['mixedFile']['opening'].update(key=2),'mixed-file-group')
        self.reject(lambda r:stage(r,'success')['model']['mixedFile']['opening'].update(key=3),'mixed-file-group')
    def test_mixed_failed_group_receipts_not_rewritten(self):
        self.reject(lambda r:r['requests'][1].update(outcome='response'),'mixed-file-group')
        self.reject(lambda r:stage(r,'success')['model']['requests'][1].update(delivered=False),'mixed-file-group')
        self.reject(lambda r:stage(r,'group-held')['model']['requests'][-1].update(released=True),'mixed-file-group')
    def test_mixed_file_release_and_no_loop_are_observed(self):
        for name in ('group-held','success'):
            self.reject(lambda r:stage(r,name)['proof']['fileRelease'].update(delivered=False),'mixed-file-group')
        for name in ('file-held','group-held','offline','retry-file-held'):
            self.reject(lambda r:stage(r,name)['proof'].update(quietObservationMs=0),'mixed-file-group')
            self.reject(lambda r:stage(r,name)['proof'].update(openingRetained=False),'mixed-file-group')
    def test_mixed_file_raw_and_effective_are_distinct_exact_sources(self):
        for part in ('source','effective'):
            self.reject(lambda r:stage(r,'success')['model']['mixedFile']['cache'][part].update(title='wrong-name-fallback'),'mixed-file-group')
        self.reject(lambda r:stage(r,'success')['model']['mixedFile']['cache']['source'].pop('fixture_opaque'),'mixed-file-group')
        self.reject(lambda r:stage(r,'success')['model']['mixedFile']['cache']['effective'].update(fixture_opaque={'preserve':True}),'mixed-file-group')
        self.reject(lambda r:stage(r,'success')['model']['mixedFile']['cache']['effective'].pop('is_deleted'),'mixed-file-group')
    def test_mixed_file_timestamp_and_tombstone_projection_flags_are_exact(self):
        for key in ('is_deleted','is_tombstoned','isUnknown','isNonExistent'):
            self.reject(lambda r:stage(r,'group-held')['model']['mixedFile']['cache']['effective'].pop(key),'mixed-file-group')
            self.reject(lambda r:stage(r,'success')['model']['mixedFile']['cache']['effective'].update({key:True}),'mixed-file-group')
        self.reject(lambda r:stage(r,'success')['model']['mixedFile']['cache']['source'].update(isUnknown=False),'mixed-file-group')
    def test_retained_mixed_normal_a_failure_contains_canonical_file_projection(self):
        from profile_group_mixed_file import file_cache
        path=Path(__file__).resolve().parents[1]/'tmp/profile-mixed-references-20261020/integration/mixed-normal-a/mixed-file-group/profile-group-recovery.log'
        if not path.exists():self.skipTest('Retained native failure is local evidence')
        rows=H['H']['closed_receipts'](path,H['PREFIX'])
        self.assertEqual(len(rows),1)
        failed=rows[0]
        self.assertFalse(failed['ok'])
        self.assertIn('Original unopened rich source replaced',failed['error'])
        self.assertEqual(failed['model']['mixedFile']['cache'],file_cache())
        actual=failed['model']['mixedFile']['opening']
        self.assertEqual(actual,dict(key=4,mounted=False,reading=False,serial=2,visit=1))
        self.assertEqual(failed['model']['profile']['blocks'],H['blocks']('mixed-file-group'))
        value=receipt('mixed-file-group')
        # Replay actual failed bookkeeping phase; a same-section re-track is
        # expected, while the original raw draft remains unmounted.
        for row in value['stages']:
            opening=row['model']['mixedFile']['opening']
            opening.update(visit=1,serial=2,key=opening['key']+2)
        stage(value,'offline')['model']['mixedFile']=copy.deepcopy(failed['model']['mixedFile'])
        H['validate'](value['fixture'],value,CAPTURES,platform='win32',png=False)
    def test_mixed_known_member_stays_zero_io(self):
        for driver in ('knownDriver','absentDriver','automaticDriver'):
            self.reject(lambda r:stage(r,'offline')['model']['mixedMember'][driver].update(started=1),'mixed-file-group')
        self.reject(lambda r:stage(r,'retry-file-held')['model']['mixedMember']['observation'].update(current=False),'mixed-file-group')
    def test_mixed_file_tokens_preserve_raw_occurrences_styles_and_urls(self):
        for mutate in (lambda t:t.pop('style'),lambda t:t.update(url='https://other.invalid/'),
                       lambda t:t.update(text='List & Plan'),lambda t:t['opaque'].update(keep=[2,'replacement']),
                       lambda t:t.update(fixture_occurrence=1),lambda t:t.update(type='canvas')):
            self.reject(lambda r:mutate(stage(r,'edited')['proof']['nativeEdit']['after']['tokens'][-1]['value']),'mixed-file-group')
        self.reject(lambda r:stage(r,'success')['geometry']['tokens'][-1].update(text='  List &amp; Plan  '),'mixed-file-group')
    def test_mixed_computed_file_styles_are_painted(self):
        self.reject(lambda r:stage(r,'success')['geometry']['tokens'][-2].update(fontWeight='400'),'mixed-file-group')
        self.reject(lambda r:stage(r,'reopened')['geometry']['tokens'][-1].update(fontStyle='normal'),'mixed-file-group')
    def test_mixed_save_cannot_replay_original_or_drop_file(self):
        self.reject(lambda r:stage(r,'saved')['model']['profile']['attempts'][0][0]['long_text'].update(blocks=json.dumps(H['blocks']('mixed-file-group'))),'mixed-file-group')
        self.reject(lambda r:stage(r,'reopened')['geometry']['captured']['snapshot']['blocks'][0]['elements'][0]['elements'].pop(),'mixed-file-group')
        self.reject(lambda r:stage(r,'reopened')['model']['mixedFile']['source'].update(completed=3),'mixed-file-group')
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
