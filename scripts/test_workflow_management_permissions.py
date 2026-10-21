"""Offline receipt mutations only; synthetic samples are not native acceptance."""
import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest

import workflow_management_permissions as U

R = runpy.run_path(str(Path(__file__).with_name('check-workflow-management-native.py')))
CAPTURES = Path('/private/permissions-captures')


def read_args(method, ordinal):
    if method == 'experiments.getByUser': return {'ssb_platform':'windows','ssb_release_channel':'prod'}
    if method == 'client.init': return {'include_relevant_onboarding':'false','_x_reason':'native-workflow-capabilities'}
    if method == 'features.access.policies.list': return {'_x_reason':'native-workflow-capabilities'}
    if method == 'client.extras': return {'extras':'["team_prefs"]','_x_reason':'native-workflow-capabilities'}
    if method == 'workflows.triggers.list': return {'app_id':'A_HOME','_x_reason':'load-workflow-editor'}
    key = 'trigger_id' if method in ('workflows.triggers.info','workflows.triggers.permissions.list') else 'workflow_id'
    return {key: 'TR_REVIEW' if key == 'trigger_id' else 'WF_REVIEW', '_x_reason':'load-workflow-editor'}


def server(case, state, confirmation):
    v=dict(original=copy.deepcopy(U.ORIGINAL),current=copy.deepcopy(U.ORIGINAL),owner=copy.deepcopy(U.OWNER),
           ownerCurrent=not state['retired'],requests=[],responses=[],settlements=[],releases=[],held=None,
           networkCalls=0,completed=0,cancelled=0)
    sequence=state['reads'][:23]+([U.METHOD] if state['writes'] else [])+state['reads'][23:]
    for serial,method in enumerate(sequence,1):
        write=method==U.METHOD
        fields=U.expected_form() if write else list(map(list,read_args(method,serial).items()))
        result=U.result_for(case) if write else U.read_result(method,v['current'])
        applied=write and case!='permissions-rejected'
        if applied: v['current']['copy']={'ok':True,'permission_type':'everyone'}
        v['requests'].append(dict(serial=serial,phase='mutation' if write else 'read',method=method,
            fields=fields,args=dict(fields),retrySafe=not write,applied=applied,
            owner=dict(U.OWNER,**{k:confirmation[k] for k in ('visit','document','revision')})))
        v['responses'].append(dict(serial=serial,result=copy.deepcopy(result)))
        if not state['held'] or serial!=24:v['settlements'].append(dict(serial=serial,returned=True))
    v['completed']=len(v['settlements']);v['held']=24 if state['held'] else None
    if case=='permissions-retirement' and state['finished']:
        v['releases']=[dict(serial=24,receiverRetired=False,delivered=True,response=copy.deepcopy(v['responses'][23]))]
    return v


def attempt(case,state,c,source):
    v={k:copy.deepcopy(value) for k,value in c.items() if k not in ('current','selected')}
    phase=state['phases'][0]
    result=U.result_for(case) if phase!='in-flight' else {'raw':None,'error':None}
    v.update(desired='everyone',finished=state['finished'],complete=state['complete'],resolved=state['resolved'],noSend=False,
        current=not state['finished'] and not state['retired'],primarySucceeded=phase=='acknowledged',
        slots=[dict(index=0,method=U.METHOD,arguments=copy.deepcopy(U.ARGUMENTS),prerequisite=None,outcome=phase,
                    raw=copy.deepcopy(result['raw']),failure='delivery unconfirmed' if phase=='unknown' else None)],readback=None)
    if state['checked']:v['readback']=dict(copy.deepcopy(source),matches=case in ('permissions-success','permissions-unknown'))
    return v


def control(id,text='',disabled=False):
    return dict(id=id,text=text,disabled=disabled,visible=True,hit=True,
                rect=dict(left=10,top=20,right=110,bottom=40,width=100,height=20))


def settings(selected):
    actions=[control('workflow-permissions-cancel','Cancel'),control('workflow-permissions-save','Save')]
    return dict(id='workflow-permissions',title='Permissions',label=control('','Who can copy this workflow?'),
                copy=dict(control('workflow-permissions-copy'),value=selected,options=[
                    dict(value='everyone',text='Everyone in Home fixture',disabled=False),
                    dict(value='collaborators',text='Workflow managers only',disabled=False)]),actions=actions)


def sample(case='permissions-success',fixture='workflow-editor'):
    viewport=R['expected_viewport'](fixture,'win32');stages=[]
    original=dict(id=13,visit=10,document=11,revision=12,title=U.TITLE,description=U.DESCRIPTION,
                  workflow='WF_REVIEW',selected='everyone',originalCopy=copy.deepcopy(U.COPY),current=True)
    selected=dict(original,id=15,revision=14) if case=='permissions-success' else original
    admitted=dict(original,id=17,revision=16) if case=='permissions-success' else selected
    revision=12
    for name in U.STAGES[case]:
        state=U.stage_state(case,name);busy=state['held'] and not state['retired']
        confirming=name in ('permissions','selected');sealed=bool(state['phases']) and not state['resolved']
        route='about' if name in ('about','back') or state['retired'] else 'editor'
        if name=='unchanged':revision=14
        if name=='cancelled':revision=16
        if state['finished'] and name not in ('back','reopened'):revision+=1
        visit=30 if name=='reopened' else 10
        c=dict(original,selected='collaborators') if name=='permissions' else selected if name=='selected' else admitted
        src=server(case,state,admitted)
        title,description=(U.WORKFLOW['title'],U.WORKFLOW['description']) if name=='editor' else (U.TITLE,U.DESCRIPTION)
        notice=(U.REOPENED if name=='reopened' else U.VERIFIED if state['resolved'] else U.OBSERVED if state['checked']
                else U.CONFIRMED if state['complete'] else U.UNCONFIRMED) if state['finished'] else None
        label={'in-flight':'Waiting for response','acknowledged':'Acknowledged','rejected':'Rejected','unknown':'Unconfirmed'}
        outcomes=['Copy access: '+label[state['phases'][0]]] if route=='editor' and sealed else []
        model=dict(route=route,visit=visit if route=='editor' else None,busy=busy,reads=len(state['reads']),writes=state['writes'],
            networkCalls=0,retainedDocuments=0 if name=='about' else 1,retainedAttempts=0,title=title if route=='editor' else None,
            description=description if route=='editor' else None,dirty=route=='editor' and name!='editor',pending=route=='editor' and sealed,
            observed=route=='editor' and (name=='reopened' or state['checked'] and not state['resolved']),
            notice=notice if route=='editor' else None,outcomes=outcomes if route=='editor' else None)
        document=dict(id=11,revision=revision,title=title,description=description,pending=c['id'] if sealed else None,
            editor=copy.deepcopy(U.WORKFLOW),trigger=copy.deepcopy(U.TRIGGER),
            copy=copy.deepcopy(src['current']['copy'] if state['resolved'] else U.COPY),run=copy.deepcopy(U.RUN),external=copy.deepcopy(U.EXTERNAL),
            notice=notice,editable=not(confirming or busy or sealed)) if route=='editor' else None
        product=dict(visit=visit,visible=route=='editor',confirmation=copy.deepcopy(c) if confirming else None,document=document,
                     attempts=[attempt(case,state,c,src['current'])] if state['phases'] else [])
        native=dict(ok=True,viewport=viewport,noOverflow=True,present=route=='editor',
            scope=dict(visit=str(visit),document='11',revision=str(revision)) if route=='editor' else None,
            originalNodes=True if route=='editor' else None,originalDetached=False if route=='editor' else name!='about',
            fields={key:dict(value=value,readonly=confirming or busy or sealed,start=0,end=0)
                    for key,value in [('title',title),('description',description)]} if route=='editor' else None,
            composing=False,leasePresent=confirming or busy,settings=None,controls=[],feedback=[])
        if route!='editor':native['controls']=[control('workflow-configuration','Configuration')]
        else:
            for key,disabled in [('save',name=='editor' or confirming or busy or sealed),('managed',confirming or busy),
                                 ('delete',confirming or busy or sealed),('permissions',confirming or busy or sealed)]:
                native['controls'].append(control('workflow-management-'+key,key,disabled))
            native['controls'].append(control('workflow-permissions-check' if sealed else 'workflow-management-check','Check current workflow',confirming or busy))
            if name=='reopened':native['controls'].append(control('workflow-management-use-current','Use current workflow'))
            if confirming:
                native['settings']=settings(c['selected']);native['controls']+=native['settings']['actions']
            native['feedback']=[control('',text) for text in ([notice] if notice else [])+outcomes]
        geometry=dict(copy.deepcopy(native),pngDimensions=viewport)
        stages.append(dict(stage=name,model=model,geometry=geometry,proof=dict(permissions=product,server=src,native=native,exercise={})))
    by={r['stage']:r for r in stages};replacement=None
    for i,row in enumerate(stages):
        name=row['stage'];o=U.observation(row)
        if name=='held':e=dict(before=copy.deepcopy(o),after=copy.deepcopy(o),observedMs=500)
        elif name in ('unchanged','cancelled','checked','checked-again','back'):
            e=dict(before=U.observation(stages[i-1]),after=o)
            if name in ('unchanged','cancelled'):e['action']='workflow-permissions-save' if name=='unchanged' else 'workflow-permissions-cancel'
        elif name=='acknowledged':
            admission=copy.deepcopy(U.observation(by['cancelled']));p=admission['permissions'];n=admission['native']
            p['confirmation']=copy.deepcopy(admitted);p['document']['editable']=False
            n['leasePresent']=True;n['settings']=settings('everyone')
            for f in n['fields'].values():f['readonly']=True
            for x in n['controls']:x['disabled']=True
            n['controls']+=n['settings']['actions'];e=dict(admission=admission)
        elif name=='reopened':e=dict(before=U.observation(by['checked']),after=o,remount=dict(ok=True,originalDetached=True,newRoot=True,newTitle=True,newDescription=True))
        elif name=='successor':
            previous=U.observation(stages[i-1]);replacement=dict(before=previous['model'],after=o['model'],previousEpoch=0,currentEpoch=1,foreignDocumentHidden=True)
            e=dict(before=previous,after=o,replacement=replacement)
        elif name=='late-settled':e=dict(before=U.observation(by['successor']),after=o,release=o['server']['releases'][-1],successorUnchanged=True)
        else:e={}
        row['proof']['exercise']=copy.deepcopy(e)
    return dict(ok=True,fixture=fixture,scenario='editor',permissionsCase=case,aboutConfiguration=True,syntheticSession=True,
        noLiveNetwork=True,viewport=viewport,stages=stages,captures={n:str(CAPTURES/f'{fixture}-{n}.png') for n in U.STAGES[case]},
        final=copy.deepcopy(U.observation(stages[-1])),replacement=replacement)


def validate(value):
    R['validate'](value['fixture'],value,CAPTURES,platform='win32',png=False,permissions_case=value['permissionsCase'])


def row(value,name):return next(x for x in value['stages'] if x['stage']==name)


class PermissionsTests(unittest.TestCase):
    def test_all_cases_both_widths(self):
        for case in U.STAGES:
            for f in ('workflow-editor','workflow-editor-narrow'):
                with self.subTest(case=case,fixture=f):validate(sample(case,f))

    def reject(self,change,case='permissions-success'):
        v=sample(case);change(v)
        with self.assertRaises((RuntimeError,KeyError,TypeError,IndexError)):validate(v)

    def test_use_current_only_resolved_observed_reopen(self):
        for case in U.STAGES:
            for name in U.STAGES[case]:
                if name in ('about','back','successor','late-settled','reopened'):continue
                with self.subTest(case=case,stage=name):
                    def change(v):
                        r=row(v,name)
                        c=control('workflow-management-use-current','Use current workflow')
                        r['proof']['native']['controls'].append(c)
                        r['geometry']['controls'].append(copy.deepcopy(c))
                    self.reject(change,case)

    def test_reopened_requires_adoption_action(self):
        def change(v):
            r=row(v,'reopened')
            for n in (r['proof']['native'],r['geometry']):
                n['controls']=[x for x in n['controls'] if x['id']!='workflow-management-use-current']
        self.reject(change)

    def test_reopened_adoption_requires_actual_observation(self):
        self.reject(lambda v:row(v,'reopened')['model'].update(observed=False))

    def test_reopened_adoption_cannot_remain_pending(self):
        self.reject(lambda v:row(v,'reopened')['model'].update(pending=True))

    def test_reopened_adoption_enabled_with_exact_label(self):
        def change(v):
            r=row(v,'reopened')
            for n in (r['proof']['native'],r['geometry']):
                next(x for x in n['controls'] if x['id']=='workflow-management-use-current')['disabled']=True
        self.reject(change)

    def test_wrong_raw_ack_permission(self):
        self.reject(lambda v:row(v,'acknowledged')['proof']['permissions']['attempts'][0]['slots'][0]['raw'].update(permission_type='collaborators'))
    def test_matching_unknown_cannot_resolve(self):
        self.reject(lambda v:row(v,'checked')['proof']['permissions']['attempts'][0].update(resolved=True),'permissions-unknown')
    def test_unknown_raw_ack_not_invented(self):
        self.reject(lambda v:row(v,'unconfirmed')['proof']['permissions']['attempts'][0]['slots'][0].update(raw=U.ACK),'permissions-unknown')
    def test_opaque_ack_retained(self):
        self.reject(lambda v:row(v,'acknowledged')['proof']['permissions']['attempts'][0]['slots'][0]['raw'].pop('opaque_receipt'))
    def test_run_access_unchanged(self):
        self.reject(lambda v:row(v,'checked')['proof']['server']['current']['run'].update(permission_type='everyone'))
    def test_external_unchanged(self):
        self.reject(lambda v:row(v,'checked')['proof']['permissions']['document']['external'].update(opaque='invented'))
    def test_managers_unchanged(self):
        self.reject(lambda v:row(v,'checked')['proof']['server']['current']['workflow']['collaborators'].append('UOTHER'))
    def test_original_settings_preserved(self):
        self.reject(lambda v:row(v,'selected')['proof']['permissions']['confirmation']['originalCopy'].update(permission_type='everyone'))
    def test_noop_zero_writes(self):
        self.reject(lambda v:row(v,'unchanged')['model'].update(writes=1))
    def test_cancel_zero_reads(self):
        self.reject(lambda v:row(v,'cancelled')['model'].update(reads=23))
    def test_cancel_retains_selection(self):
        self.reject(lambda v:row(v,'cancelled')['proof']['exercise']['before']['native']['fields']['title'].update(start=1))
    def test_changed_save_new_admission(self):
        self.reject(lambda v:row(v,'acknowledged')['proof']['exercise']['admission']['permissions']['confirmation'].update(id=15))
    def test_native_original_node_required(self):
        self.reject(lambda v:row(v,'checked')['proof']['native'].update(originalNodes=False))
    def test_wrong_copy_option(self):
        self.reject(lambda v:row(v,'permissions')['proof']['native']['settings']['copy']['options'][0].update(value='public'))
    def test_occluded_settings(self):
        self.reject(lambda v:row(v,'selected')['proof']['native']['settings']['copy'].update(hit=False))
    def test_check_reads_are_fresh(self):
        self.reject(lambda v:row(v,'checked')['proof']['server']['requests'].pop())
    def test_mutation_reason(self):
        self.reject(lambda v:row(v,'acknowledged')['proof']['server']['requests'][23]['fields'][-1].__setitem__(1,'publish-workflow'))
    def test_rejected_does_not_apply(self):
        self.reject(lambda v:row(v,'rejected')['proof']['server']['requests'][23].update(applied=True),'permissions-rejected')
    def test_retired_owner_required(self):
        self.reject(lambda v:row(v,'successor')['proof']['server'].update(ownerCurrent=True),'permissions-retirement')
    def test_actual_original_release(self):
        self.reject(lambda v:row(v,'late-settled')['proof']['server']['releases'][0].update(delivered=False),'permissions-retirement')
    def test_late_cannot_change_successor(self):
        self.reject(lambda v:row(v,'late-settled')['model'].update(title='foreign'),'permissions-retirement')
    def test_reopen_original_detached(self):
        self.reject(lambda v:row(v,'reopened')['proof']['exercise']['remount'].update(originalDetached=False))
    def test_held_quiet_is_bounded_and_exact(self):
        self.reject(lambda v:row(v,'held')['proof']['exercise'].update(observedMs=0),'permissions-retirement')
    def test_explicit_case_required(self):
        with self.assertRaises(RuntimeError):R['validate']('workflow-editor',sample(),CAPTURES,png=False)
    def test_cross_case_refused(self):
        with self.assertRaises(RuntimeError):R['validate']('workflow-editor',sample(),CAPTURES,png=False,permissions_case='permissions-unknown')
    def test_conflicting_cases(self):
        with self.assertRaises(RuntimeError):R['validate']('workflow-editor',sample(),CAPTURES,png=False,permissions_case='permissions-success',delete_case='delete-success')
    def test_legacy_stage_maps_unchanged(self):
        self.assertEqual(set(R['STAGES']),{'managed','editor','reply-loss'})
        self.assertEqual(len(R['DELETE_STAGES']),6);self.assertEqual(len(R['UNPUBLISH_STAGES']),6)


class ClosedLogTests(unittest.TestCase):
    def make_native(self, directory, value):
        from types import SimpleNamespace
        ready=R['ready_tuple'](value['fixture'],'a'*32,1234)
        value['readiness']=ready
        edges=[dict(stage=stage,edge=edge) for stage in U.STAGES[value['permissionsCase']] for edge in ('before','after')]
        log=directory/'native.log'
        log.write_text(R['WAITING']+json.dumps(ready)+'\n'+''.join(R['EDGE']+json.dumps(e)+'\n' for e in edges)+R['PREFIX']+json.dumps(value)+'\n',encoding='utf-8')
        records={R['WAITING']:[ready],R['PREFIX']:[value],R['EDGE']:edges}
        return SimpleNamespace(log=log,receipts=lambda prefix:records.get(prefix,[])),ready,records

    def test_closed_log_case_and_edges(self):
        with tempfile.TemporaryDirectory() as d:
            for case in U.STAGES:
                value=sample(case)
                native,ready,_=self.make_native(Path(d),value)
                self.assertEqual(R['validate_closed'](native,ready,CAPTURES,value,platform='win32',png=False,permissions_case=case),value)

    def test_closed_trailing_partial_refused(self):
        with tempfile.TemporaryDirectory() as d:
            value=sample();native,ready,_=self.make_native(Path(d),value)
            with native.log.open('a',encoding='utf-8') as out:out.write(R['PREFIX']+'{')
            with self.assertRaises(RuntimeError):
                R['validate_closed'](native,ready,CAPTURES,value,platform='win32',png=False,permissions_case=value['permissionsCase'])

    def test_missing_capture_edge_refused(self):
        with tempfile.TemporaryDirectory() as d:
            value=sample();native,_,records=self.make_native(Path(d),value)
            records[R['EDGE']].pop()
            with self.assertRaises(RuntimeError):
                R['validate_log'](native,value['fixture'],CAPTURES,'win32',False,permissions_case=value['permissionsCase'])

    def test_owned_ready_tuple_required(self):
        with tempfile.TemporaryDirectory() as d:
            value=sample();native,ready,_=self.make_native(Path(d),value)
            wrong=dict(ready,pid=999)
            with self.assertRaises(RuntimeError):
                R['validate_closed'](native,wrong,CAPTURES,value,platform='win32',png=False,permissions_case=value['permissionsCase'])


if __name__=='__main__':unittest.main()
