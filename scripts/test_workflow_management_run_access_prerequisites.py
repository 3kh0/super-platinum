"""Forged prerequisite observations; synthetic samples are not native acceptance."""
import copy
import unittest
from pathlib import Path
import workflow_management_run_access_prerequisites as U
import workflow_management_run_access_prerequisites_ledger as L
from workflow_management_run_access_picker import TEAMS, PERSON, CHANNEL
from test_workflow_management_permissions import R, CAPTURES, control, settings, read_args


def source(case,stage):
    state=L.stage_state(case,stage)
    owner=dict(account='synthetic-workflow-run-access',epoch=0,team='TWORKFLOW',user='UWORKFLOW')
    self_user=dict(id='UWORKFLOW',team_id='TWORKFLOW')
    seed=dict(boot=dict(ok=True,team=TEAMS[0],self=self_user,users=[PERSON],channels=[CHANNEL]),
              init=dict(ok=True,team=TEAMS[0],self=self_user,workspaces=TEAMS,onboarding=dict(user_data=dict(onboarding_complete=True)),user_date_created=123),
              canonicalTeams={v['id']:v for v in TEAMS})
    value=dict(original=L.original(case),current=L.original(case),owner=owner,ownerCurrent=True,networkCalls=0,
               requests=[],responses=[],settlements=[],releases=[],held=None,completed=0,cancelled=0,seed=seed)
    events=[('read',m) for m in state['reads'][:12]]+[('prerequisite',k) for k in state['kinds']]
    if state['sent']:events += [('read',m) for m in state['reads'][12:23]]+[('mutation',L.RUN_METHOD),('followup',L.RESTRICTION_METHOD)]+[('read',m) for m in state['reads'][23:]]
    count=0
    for serial,(phase,name) in enumerate(events,1):
        witness=dict(owner,visit=10,document=11,revision=12)
        request=dict(serial=serial,phase=phase,method=name,owner=witness,retrySafe=False,applied=False)
        if phase=='prerequisite':
            count+=1
            if name=='Discovery':
                fields=[['include_external_workspaces','true'],['_x_reason','client-needs-valid-flannel-url']]
                request.update(phase='read',method='api.getFlannelHttpUrl',fields=fields,args=dict(fields))
                raw,error=L.DISCOVERY,None
            else:
                request.update(phase='picker',method=L.METHOD[name],kind=name,body=copy.deepcopy(L.BODY[name]),query=copy.deepcopy(L.BODY[name]),retrySafe=name=='TeamsById')
                raw,error=(None,'offline') if case=='run-cold-targets' and count==2 else (L.PAGES[name],None)
            result=dict(raw=copy.deepcopy(raw),error=error)
        else:
            if phase=='mutation':
                args=L.arguments(case);fields=[[k,','.join(v) if isinstance(v,list) else v] for k,v in sorted(args.items())]+[['_x_reason','set-trigger-run-permission']]
                result=dict(raw=L.ack(case),error=None);request['applied']=True
                value['current']['run']=dict(ok=True,**L.desired(case))
            elif phase=='followup':
                fields=[['workflow_id','WF_REVIEW'],['_x_reason','set-trigger-run-permission']];result=dict(raw=copy.deepcopy(L.RESTRICTION),error=None)
            else:
                fields=list(map(list,read_args(name,serial).items()));result=L.read_result(name,value['current'])
            request.update(fields=fields,args=dict(fields))
        value['requests'].append(request);value['responses'].append(dict(serial=serial,result=copy.deepcopy(result)))
    held=(16 if case=='run-cold-targets' else 14) if state['pickerHeld'] else None
    value['held']=held;value['settlements']=[dict(serial=n,returned=True) for n in range(1,len(events)+1) if n!=held]
    value['completed']=len(value['settlements'])
    if state['released']:
        serial=16 if case=='run-cold-targets' else 14
        value['releases']=[dict(serial=serial,receiverRetired=False,delivered=True,response=copy.deepcopy(value['responses'][serial-1]))]
    return value,state


def run_draft(case,stage):
    cold=case=='run-cold-targets';unknown=stage in ('missing-offline','retry-held')
    mode=None if unknown else 'named_entities' if stage in ('permissions','hydrated') else 'everyone'
    ids=None if unknown else L.COLD_IDS if stage=='hydrated' else [] if cold else ['UWORKFLOW','TRUNEXTER'] if stage in ('permissions','available-held') else ['TRUNEXTER']
    available=['TRUNEXTER'] if not cold and stage=='selected' else None
    receipts=[] if unknown or not cold and stage!='selected' else [L.DISCOVERY,L.PAGES['MemberInfo'],L.PAGES['ChannelInfo'],L.PAGES['TeamsById']] if cold else [L.DISCOVERY,L.PAGES['AvailableExternal']]
    rows=[] if unknown or not cold else [dict(id=raw['id'],kind=kind,label=label,external=False,raw=copy.deepcopy(raw)) for raw,kind,label in zip(L.RAW_ROWS,['User','Channel','Team'],['Cold Runner','cold-run-team','Cold internal'])]
    return dict(epoch=20+L.STAGES[case].index(stage),generation=20,pending=stage in ('retry-held','available-held'),mode=mode,ids=ids,
        notice='Selected reference details could not be loaded. Your settings are retained. Retry to load their details.' if stage=='missing-offline' else None,
        externalEnabled=not cold,query='',externalQuery='',results=[],externalResults=['TRUNEXTER'] if available else [],receipts=copy.deepcopy(receipts),rows=rows,
        scope=dict(currentTeam='TWORKFLOW',topLevel='ERUNORG01',unified=False,collaborators=['UWORKFLOW'],availableExternal=available))


def native_settings(draft,stage):
    v=settings('collaborators');disabled=draft['pending'] or draft['mode'] is None
    v['actions'][1]['disabled']=disabled
    v.update(epoch=str(draft['epoch']),originalControls=True,
        run=dict(control('workflow-permissions-run',disabled=draft['mode'] is None),value=draft['mode'] or '',options=[
            dict(value='everyone',text='Everyone internal in Home fixture',disabled=False),
            dict(value='named_entities',text='Workflow managers, plus specific people, channels, and workspaces',disabled=False),
            dict(value='app_collaborators',text='Workflow managers only',disabled=False)]),
        runLabel=control('','Who can find and use this workflow?'),external=dict(control('workflow-permissions-external'),checked=draft['externalEnabled']),
        query=dict(control('workflow-permissions-target-query'),value='') if draft['mode']=='named_entities' else None,
        externalQuery=dict(control('workflow-permissions-external-query'),value='') if draft['externalEnabled'] else None,
        selected=[dict(control(''),target=id) for id in ((draft['ids'] or []) if draft['mode']=='named_entities' else ['TRUNEXTER'] if draft['externalEnabled'] else [])],
        results=[dict(control(''),target=id) for id in draft['externalResults']])
    messages=[]
    if draft['pending']:messages.append('Loading reference details\u2026')
    if draft['notice']:messages.append(draft['notice'])
    if draft['mode'] is None:messages.append('Current run-access references could not be resolved. Your existing permissions are retained.')
    v['referenceFeedback']=[control('',m) for m in messages]
    return v


def operation(case,state,current):
    return dict(id=13,visit=10,document=11,revision=12,title=U.TITLE,description=U.DESCRIPTION,workflow='WF_REVIEW',originalCopy=copy.deepcopy(U.COPY),
        copyChanged=False,desired='collaborators',finished=True,complete=True,resolved=state['resolved'],noSend=False,primarySucceeded=False,current=False,
        slots=[dict(index=0,method=U.COPY_METHOD,arguments=dict(workflow_id='WF_REVIEW',permission_type='collaborators'),prerequisite=None,outcome='pending',raw=None,failure=None)],
        runGroup=dict(complete=True,primary=dict(method=U.RUN_METHOD,arguments=L.arguments(case),outcome='acknowledged',raw=L.ack(case),failure=None),
            followup=dict(phase='Complete',fullySucceeded=True,restriction=copy.deepcopy(U.RESTRICTION),slots=[dict(method=U.RESTRICTION_METHOD,
                arguments=dict(workflow_id='WF_REVIEW'),outcome='acknowledged',raw=copy.deepcopy(U.RESTRICTION),failure=None)])),
        readback=dict({k:copy.deepcopy(v) for k,v in current.items() if k!='restriction'},matches=True) if state['checked'] else None)


def sample(case,fixture='workflow-editor'):
    viewport=R['expected_viewport'](fixture,'win32');rows=[]
    confirming_stages=('permissions','selected','missing-offline','retry-held','hydrated','available-held')
    for stage in L.STAGES[case]:
        server,state=source(case,stage);route='about' if stage in ('about','back') else 'editor';confirming=stage in confirming_stages
        sealed=state['sent'] and not state['resolved'];visit=30 if stage=='reopened' else 10
        revision=16 if state['checked'] else 14 if state['sent'] else 12
        title,description=(U.WORKFLOW['title'],U.WORKFLOW['description']) if stage=='editor' else (U.TITLE,U.DESCRIPTION)
        notice=(U.REOPENED if stage=='reopened' else U.VERIFIED if state['resolved'] else U.CONFIRMED) if state['sent'] else None
        outcomes=['Run access: Acknowledged','Workflow restrictions: Acknowledged'] if sealed else []
        doc=dict(id=11,revision=revision,title=title,description=description,pending=13 if sealed else None,editor=copy.deepcopy(U.WORKFLOW),
            trigger=copy.deepcopy(U.TRIGGER),copy=copy.deepcopy(U.COPY),run=copy.deepcopy(server['current']['run'] if state['resolved'] else L.RUN[case]),
            external=copy.deepcopy(U.EXTERNAL),notice=notice,editable=not confirming and not sealed) if route=='editor' else None
        confirmation=dict(id=13,visit=10,document=11,revision=12,title=U.TITLE,description=U.DESCRIPTION,workflow='WF_REVIEW',
            selected='collaborators',originalCopy=copy.deepcopy(U.COPY),current=True,runDraft=run_draft(case,stage)) if confirming else None
        product=dict(visit=visit,visible=route=='editor',document=doc,confirmation=confirmation,
            attempts=[operation(case,state,server['current'])] if state['sent'] else [])
        model=dict(route=route,visit=visit if route=='editor' else None,busy=False,reads=len(server['requests'])-state['writes'],writes=state['writes'],networkCalls=0,
            retainedDocuments=0 if stage=='about' else 1,retainedAttempts=0,title=title if route=='editor' else None,description=description if route=='editor' else None,
            dirty=route=='editor' and stage!='editor',pending=route=='editor' and sealed,observed=stage=='reopened',notice=notice if route=='editor' else None,outcomes=outcomes if route=='editor' else None)
        native=dict(ok=True,viewport=viewport,noOverflow=True,present=route=='editor',scope=dict(visit=str(visit),document='11',revision=str(revision)) if route=='editor' else None,
            originalNodes=True if route=='editor' else None,originalDetached=stage=='back',composing=False,leasePresent=confirming,
            fields={k:dict(value=v,readonly=confirming or sealed,start=0,end=0) for k,v in [('title',title),('description',description)]} if route=='editor' else None,
            settings=native_settings(confirmation['runDraft'],stage) if confirming else None,controls=[],feedback=[])
        if route!='editor':native['controls']=[control('workflow-configuration','Configuration')]
        else:
            native['controls']=[control('workflow-management-'+k,k,d) for k,d in [('save',stage=='editor' or confirming or sealed),('managed',confirming),('delete',confirming or sealed),('permissions',confirming or sealed)]]
            native['controls'].append(control('workflow-permissions-check' if sealed else 'workflow-management-check','Check',confirming))
            if stage=='reopened':native['controls'].append(control('workflow-management-use-current','Use current workflow'))
            if confirming:native['controls']+=native['settings']['actions']
            if stage=='missing-offline':native['controls'].append(control('workflow-permissions-target-retry','Retry reference details'))
            native['feedback']=[control('',m) for m in ([notice] if notice else [])+outcomes]
        rows.append(dict(stage=stage,model=model,geometry=dict(copy.deepcopy(native),pngDimensions=viewport),proof=dict(permissions=product,server=server,native=native,exercise={})))
    by={r['stage']:r for r in rows}
    for index,row in enumerate(rows):
        stage=row['stage'];current=U.observation(row);prior=U.observation(rows[index-1]) if index else None
        if stage=='missing-offline':exercise=dict(before=current,after=current,observedMs=500)
        elif stage in ('retry-held','available-held'):exercise=dict(admission=prior,quiet=dict(before=current,after=current,observedMs=500))
        elif stage=='hydrated' or stage=='selected' and case=='run-everyone-external':exercise=dict(before=prior,release=current['server']['releases'][-1],after=current)
        elif stage in ('selected','checked','back'):exercise=dict(before=prior,after=current)
        elif stage=='acknowledged':exercise=dict(admission=prior)
        elif stage=='reopened':exercise=dict(before=U.observation(by['checked']),after=current,remount=dict(ok=True,originalDetached=True,newRoot=True,newTitle=True,newDescription=True))
        else:exercise={}
        row['proof']['exercise']=copy.deepcopy(exercise)
    return dict(ok=True,fixture=fixture,scenario='editor',runAccessCase=case,aboutConfiguration=True,syntheticSession=True,noLiveNetwork=True,
        viewport=viewport,stages=rows,captures={s:str(CAPTURES/f'{fixture}-{s}.png') for s in L.STAGES[case]},final=U.observation(rows[-1]),replacement=None)


def validate(receipt):
    R['validate'](receipt['fixture'],receipt,CAPTURES,platform='win32',png=False,run_access_case=receipt['runAccessCase'])


class PrerequisiteTests(unittest.TestCase):
    def test_team_ids_use_exact_primitive_csv_form_not_json_array_encoding(self):
        value,state=source('run-cold-targets','hydrated')
        request=value['requests'][17]
        self.assertEqual(request['body'],[['include_unlisted_if_admin','false'],['teams','TMISSING01']])
        L.server_proof(value,'run-cold-targets',state)
        for altered in ['["TMISSING01"]','TMISSING01,','TMISSING01,TFOREIGN1','']:
            changed=copy.deepcopy(value)
            for key in ('body','query'):changed['requests'][17][key][1][1]=altered
            with self.subTest(altered=altered),self.assertRaises(RuntimeError):L.server_proof(changed,'run-cold-targets',state)

    def test_both_complete_original_sequences_at_both_widths(self):
        for case in L.STAGES:
            for fixture in ('workflow-editor','workflow-editor-narrow'):
                with self.subTest(case=case,fixture=fixture):validate(sample(case,fixture))

    def test_exact_original_identity_wire_and_raw_pages(self):
        changes=[lambda s:s['requests'][13]['body'].update(ids=['UFOREIGN1']),
                 lambda s:s['responses'][15]['result']['raw']['can_interact'].update(UMISSING01=False),
                 lambda s:s['responses'][16]['result']['raw'].update(member_channels=[]),
                 lambda s:s['requests'][17]['body'].append(['_x_reason','invented']),
                 lambda s:s['requests'][17].update(retrySafe=False),
                 lambda s:s['requests'][15]['owner'].update(account='replacement'),
                 lambda s:s['seed']['boot']['users'].append(copy.deepcopy(L.PERSON)),
                 lambda s:s['current']['external']['trippable_by_slack_connected_teams'].update(allowed_teams=[])]
        for change in changes:
            s,state=source('run-cold-targets','hydrated');change(s)
            with self.subTest(change=changes.index(change)),self.assertRaises(RuntimeError):L.server_proof(s,'run-cold-targets',state)

    def test_real_held_receipt_and_settlement_required(self):
        for case,stage in [('run-cold-targets','retry-held'),('run-everyone-external','available-held')]:
            s,state=source(case,stage);s['settlements'].append(dict(serial=s['held'],returned=True));s['completed']+=1
            with self.subTest(case=case),self.assertRaises(RuntimeError):L.server_proof(s,case,state)
        for key,value in [('delivered',False),('receiverRetired',True),('serial',17)]:
            s,state=source('run-cold-targets','hydrated');s['releases'][0][key]=value
            with self.subTest(key=key),self.assertRaises(RuntimeError):L.server_proof(s,'run-cold-targets',state)

    def test_everyone_external_needs_actual_available_list_and_exact_wire(self):
        for change in [lambda s:s['responses'][13]['result']['raw'].update(results=[]),
                       lambda s:s['requests'][13]['body'].update(filter='all'),
                       lambda s:s['requests'][25]['fields'].__setitem__(0,['permission_type','everyone']),
                       lambda s:s['current']['run'].update(user_ids=['UWORKFLOW'])]:
            s,state=source('run-everyone-external','checked');change(s)
            with self.subTest(change=change),self.assertRaises(RuntimeError):L.server_proof(s,'run-everyone-external',state)

    def test_no_ready_state_fabricated_before_held_response(self):
        for case,stage,change in [
            ('run-cold-targets','retry-held',lambda d:d.update(mode='named_entities',ids=L.COLD_IDS)),
            ('run-cold-targets','missing-offline',lambda d:d.update(notice=None)),
            ('run-cold-targets','retry-held',lambda d:d['rows'].append(dict(id='UMISSING01',raw=L.PERSON))),
            ('run-everyone-external','available-held',lambda d:d['scope'].update(availableExternal=['TRUNEXTER'])),
            ('run-everyone-external','selected',lambda d:d.update(ids=['TWORKFLOW','TRUNEXTER'])),
        ]:
            c=dict(selected='collaborators',runDraft=run_draft(case,stage));change(c['runDraft'])
            with self.subTest(case=case,stage=stage),self.assertRaises(RuntimeError):U.draft_proof(c,case,stage)

    def test_native_epoch_disabled_save_and_original_controls_are_independent(self):
        for key,value in [('originalControls',False),('epoch','99')]:
            r=sample('run-cold-targets');row=next(x for x in r['stages'] if x['stage']=='retry-held')
            row['proof']['native']['settings'][key]=value;row['geometry']['settings'][key]=value
            with self.subTest(key=key),self.assertRaises(RuntimeError):validate(r)
        r=sample('run-everyone-external');row=next(x for x in r['stages'] if x['stage']=='available-held')
        for n in (row['proof']['native'],row['geometry']):
            n['settings']['actions'][1]['disabled']=False
            next(c for c in n['controls'] if c['id']=='workflow-permissions-save')['disabled']=False
        with self.assertRaises(RuntimeError):validate(r)

    def test_early_automatic_retry_and_rewritten_release_are_rejected(self):
        r=sample('run-cold-targets');row=next(x for x in r['stages'] if x['stage']=='missing-offline')
        row['proof']['exercise']['after']['model']['reads']+=1
        with self.assertRaises(RuntimeError):validate(r)
        r=sample('run-cold-targets');row=next(x for x in r['stages'] if x['stage']=='hydrated')
        row['proof']['exercise']['release']['delivered']=False
        with self.assertRaises(RuntimeError):validate(r)

    def test_ack_readback_cannot_save_metadata_or_lose_other_targets(self):
        for key,value in [('title','overwritten'),('complete',False),('copyChanged',True)]:
            r=sample('run-everyone-external');r['stages'][-1]['proof']['permissions']['attempts'][0][key]=value
            with self.subTest(key=key),self.assertRaises(RuntimeError):validate(r)
        r=sample('run-everyone-external');r['stages'][-1]['proof']['permissions']['attempts'][0]['readback']['run']['channel_ids']=['CFOREIGN1']
        with self.assertRaises(RuntimeError):validate(r)


if __name__=='__main__':unittest.main()
