"""Independent native prerequisite observations; original six scenarios unchanged."""
import copy
from pathlib import Path
from workflow_management_delete import TITLE, DESCRIPTION, exact, integer, require
from workflow_management_unpublish import measured_native, slot_proof
from workflow_management_permissions import native_controls, observation, REOPENED, OBSERVED
from workflow_management_run_access import CONFIRMED, UNCONFIRMED, VERIFIED
from workflow_management_run_access_ledger import COPY, EXTERNAL, RESTRICTION, WORKFLOW, TRIGGER, COPY_METHOD, RUN_METHOD, RESTRICTION_METHOD
from workflow_management_run_access_prerequisites_ledger import (STAGES, RUN, RAW_ROWS, COLD_IDS, PAGES, DISCOVERY,
    stage_state, server_proof, desired, arguments, ack)


def attempt_proof(attempt,case,state,current):
    require(isinstance(attempt,dict),'Missing original prerequisite-derived attempt')
    for key in ('id','visit','document','revision'):integer(attempt.get(key),'Missing original '+key,1)
    require(attempt.get('title')==TITLE and attempt.get('description')==DESCRIPTION and attempt.get('workflow')=='WF_REVIEW'
            and exact(attempt.get('originalCopy'),COPY) and attempt.get('copyChanged') is False
            and attempt.get('desired')=='collaborators','Attempt changed original copy/draft')
    for key in ('finished','complete','resolved'):require(attempt.get(key) is state[key],'Wrong attempt '+key)
    require(attempt.get('noSend') is False and attempt.get('primarySucceeded') is False and attempt.get('current') is False,
            'Unchanged copy slot authorized run or original attempt still active')
    slots=attempt.get('slots')
    require(isinstance(slots,list) and len(slots)==1,'Lost unchanged copy plan')
    slot_proof(slots[0],0,COPY_METHOD,dict(workflow_id='WF_REVIEW',permission_type='collaborators'),'pending',None,None)
    primary=dict(method=RUN_METHOD,arguments=arguments(case),outcome='acknowledged',raw=ack(case),failure=None)
    follow=dict(phase='Complete',fullySucceeded=True,restriction=RESTRICTION,slots=[dict(method=RESTRICTION_METHOD,
        arguments=dict(workflow_id='WF_REVIEW'),outcome='acknowledged',raw=RESTRICTION,failure=None)])
    require(exact(attempt.get('runGroup'),dict(complete=True,primary=primary,followup=follow)),'Original run/follow-up ACK lost')
    expected=dict({k:copy.deepcopy(v) for k,v in current.items() if k!='restriction'},matches=True) if state['checked'] else None
    require(exact(attempt.get('readback'),expected),'Readback changed ACK or ignored effective targets')


def draft_proof(confirmation,case,stage):
    draft=confirmation.get('runDraft')
    require(isinstance(draft,dict) and confirmation.get('selected')=='collaborators','Copy changed during run prerequisites')
    for key in ('epoch','generation'):integer(draft.get(key),'Missing settings '+key,1)
    cold=case=='run-cold-targets';unresolved=cold and stage in ('missing-offline','retry-held')
    mode=None if unresolved else 'named_entities' if stage in ('hydrated','permissions') else 'everyone'
    ids=None if unresolved else COLD_IDS if stage=='hydrated' else [] if cold else ['UWORKFLOW','TRUNEXTER'] if stage in ('permissions','available-held') else ['TRUNEXTER']
    pending=stage in ('retry-held','available-held')
    notice='Selected reference details could not be loaded. Your settings are retained. Retry to load their details.' if stage=='missing-offline' else None
    require(draft.get('mode')==mode and exact(draft.get('ids'),ids) and draft.get('pending') is pending
            and draft.get('notice')==notice and draft.get('externalEnabled') is (not cold),
            'Unresolved/held/selected draft readiness was fabricated')
    scope=draft.get('scope',{})
    require(exact({k:scope.get(k) for k in ('currentTeam','topLevel','unified','collaborators')},
        dict(currentTeam='TWORKFLOW',topLevel='ERUNORG01',unified=False,collaborators=['UWORKFLOW'])), 'Scope partition changed')
    available=['TRUNEXTER'] if not cold and stage=='selected' else None
    require(exact(scope.get('availableExternal'),available),'Available organizations inferred before original reply')
    require(draft.get('query')==draft.get('externalQuery')=='' and draft.get('results')==[], 'Unrequested text search')
    require(draft.get('externalResults')==(['TRUNEXTER'] if available else []),'Unrequested or lost external result')
    receipts=[] if unresolved or not cold and stage!='selected' else [DISCOVERY,PAGES['MemberInfo'],PAGES['ChannelInfo'],PAGES['TeamsById']] if cold else [DISCOVERY,PAGES['AvailableExternal']]
    require(exact(draft.get('receipts'),receipts),'Product acquisition receipts fabricated or changed')
    rows=draft.get('rows');require(isinstance(rows,list),'Missing source rows')
    require(len({r.get('id') for r in rows})==len(rows),'Duplicate source row')
    cold_rows=[r for r in rows if r.get('id') in COLD_IDS]
    if unresolved:require(cold_rows==[],'Held/failed identity read created ready rows')
    elif cold:
        require(len(cold_rows)==3,'Hydration omitted original grants')
        for raw,kind,label in zip(RAW_ROWS,['User','Channel','Team'],['Cold Runner','cold-run-team','Cold internal']):
            require(exact(next((r for r in cold_rows if r.get('id')==raw['id']),None),
                dict(id=raw['id'],kind=kind,label=label,external=False,raw=raw)),'Canonical identity raw/label changed')


def settings_native(native,controls,confirmation,case,stage,viewport,rectangle):
    setting=native.get('settings');draft=confirmation['runDraft']
    require(isinstance(setting,dict) and setting.get('id')=='workflow-permissions' and setting.get('title')=='Permissions'
            and setting.get('originalControls') is True and setting.get('epoch')==str(draft['epoch']),'Original settings witness changed')
    label,select=setting.get('label',{}),setting.get('copy',{})
    require(label.get('text')=='Who can copy this workflow?' and select.get('id')=='workflow-permissions-copy'
            and select.get('value')=='collaborators' and exact(select.get('options'),[
                dict(value='everyone',text='Everyone in Home fixture',disabled=False),dict(value='collaborators',text='Workflow managers only',disabled=False)]), 'Copy control changed')
    disabled=draft['pending'] or draft['mode'] is None
    actions=setting.get('actions')
    require(exact(actions,[controls.get('workflow-permissions-cancel'),controls.get('workflow-permissions-save')])
            and [a.get('text') for a in actions]==['Cancel','Save'] and actions[0].get('disabled') is False
            and actions[1].get('disabled') is disabled,'Unsafe Save before prerequisite completion')
    run=setting.get('run',{});unresolved=draft['mode'] is None
    require(run.get('id')=='workflow-permissions-run' and run.get('value')==('' if unresolved else draft['mode'])
            and run.get('disabled') is unresolved and exact(run.get('options'),[
                dict(value='everyone',text='Everyone internal in Home fixture',disabled=False),
                dict(value='named_entities',text='Workflow managers, plus specific people, channels, and workspaces',disabled=False),
                dict(value='app_collaborators',text='Workflow managers only',disabled=False)]),'Wrong actual run selector')
    require(setting.get('runLabel',{}).get('text')=='Who can find and use this workflow?','Wrong run label')
    visible_ids=(draft['ids'] or []) if draft['mode']=='named_entities' else ['TRUNEXTER'] if draft['externalEnabled'] else []
    require([v.get('target') for v in setting.get('selected',[])]==visible_ids,'Visible target chips do not match retained grants')
    require([v.get('target') for v in setting.get('results',[])]==draft['externalResults'],'Unrequested result chips')
    external=setting.get('external');require(isinstance(external,dict) and external.get('checked') is draft['externalEnabled'], 'Wrong external option')
    for key,shown in [('query',draft['mode']=='named_entities'),('externalQuery',draft['externalEnabled'])]:
        node=setting.get(key);require((node is not None) is shown,'Unexpected query control')
        if shown:require(node.get('value')=='','Hidden/unrequested query text')
    samples=[label,select,*actions,run,setting.get('runLabel'),external,setting.get('query'),setting.get('externalQuery')]+setting.get('selected',[])+setting.get('results',[])
    for node in filter(lambda v:v is not None,samples):
        require(node.get('visible') is True and node.get('hit') is True,'Original control not independently reachable')
        rectangle(node.get('rect'),viewport)
        if node is not run and node is not actions[1]:require(node.get('disabled') is False,'Unexpected disabled control')
    messages=[]
    if draft['pending']:messages.append('Loading reference details\u2026')
    if draft['notice']:messages.append(draft['notice'])
    if unresolved:messages.append('Current run-access references could not be resolved. Your existing permissions are retained.')
    feedback=setting.get('referenceFeedback')
    require(isinstance(feedback,list) and [f.get('text') for f in feedback]==messages,'Required prerequisite status missing/repeated')
    for node in feedback:
        require(node.get('visible') is True and node.get('hit') is True,'Prerequisite feedback unreachable');rectangle(node.get('rect'),viewport)
    retry=controls.get('workflow-permissions-target-retry')
    require((retry is not None) is (stage=='missing-offline'),'Wrong explicit Retry visibility')
    if retry:require(retry.get('text')=='Retry reference details' and retry.get('disabled') is False,'Retry unavailable')


def validate(fixture,case,receipt,captures,*,viewport,rectangle,validate_png,png=True):
    require(case in STAGES and fixture in ('workflow-editor','workflow-editor-narrow'),'Wrong run-access case/fixture')
    require(receipt.get('ok') is True and receipt.get('fixture')==fixture and receipt.get('scenario')=='editor'
            and receipt.get('runAccessCase')==case,'Wrong or failed run-access receipt')
    require(all(receipt.get(k) is True for k in ('aboutConfiguration','syntheticSession','noLiveNetwork')),
            'Missing actual entry/synthetic isolation')
    require(receipt.get('viewport')==viewport,'Wrong native viewport')
    rows,paths=receipt.get('stages'),receipt.get('captures')
    require(isinstance(rows,list) and [r.get('stage') for r in rows]==list(STAGES[case]),'Wrong native stage order')
    require(isinstance(paths,dict) and set(paths)==set(STAGES[case]),'Wrong original PNG inventory')
    identity=attempt_id=previous=None
    for row in rows:
        stage,model,geometry,proof=(row.get(k) for k in ('stage','model','geometry','proof'))
        require(all(isinstance(v,dict) for v in (model,geometry,proof)),'Missing original stage evidence')
        product,server,native=(proof.get(k) for k in ('permissions','server','native'))
        require(isinstance(product,dict),'Missing actual product observer')
        state=stage_state(case,stage)
        counts=server_proof(server,case,state)
        route='about' if stage in ('about','back') or state['retired'] else 'editor'
        confirming=stage in ('permissions','selected','missing-offline','retry-held','hydrated','available-held')
        busy=state['held'] and not state['retired'];sealed=state['sent'] and not state['resolved']
        for key in ('reads','writes','networkCalls','retainedDocuments','retainedAttempts'):integer(model.get(key),'Invalid actual '+key)
        require(model['reads']==counts['reads'] and model['writes']==counts['writes'] and model['networkCalls']==0
                and model['retainedAttempts']==0,'Model differs from original dispatch ledger')
        require(model.get('route')==route and model.get('busy') is busy
                and model['retainedDocuments']==(0 if stage=='about' else 1),'Wrong route/document custody')
        require(model.get('pending') is (route=='editor' and sealed)
                and model.get('dirty') is (route=='editor' and stage!='editor')
                and model.get('observed') is (route=='editor' and (stage=='reopened' or state['checked'] and not state['resolved'])),
                'Draft/readback custody changed')
        outcomes=[]
        if route=='editor' and sealed:
            outcomes.append('Run access: '+{'in-flight':'Waiting for response','acknowledged':'Acknowledged','rejected':'Rejected','unknown':'Unconfirmed'}[state['phase']])
            if state['follow']:outcomes.append('Workflow restrictions: '+('Not sent' if case=='run-followup-failure' else 'Acknowledged'))
        require(model.get('outcomes')==(outcomes if route=='editor' else None),'Wrong independent visible outcomes')
        require(product.get('visible') is (route=='editor'),'Hidden original visit leaked')
        attempts=product.get('attempts')
        require(isinstance(attempts,list) and len(attempts)==int(state['sent']),'Original attempt lost or replayed')
        if attempts:
            attempt_proof(attempts[0],case,state,server['current'])
            if attempt_id is None:attempt_id=attempts[0]['id']
            require(attempts[0]['id']==attempt_id,'Original attempt identity replaced')
        document=product.get('document')
        if route=='editor':
            require(isinstance(document,dict),'Original draft missing')
            for key in ('id','revision'):integer(document.get(key),'Invalid draft '+key,1)
            if identity is None:identity=(product['visit'],document['id'])
            require(document['id']==identity[1] and (stage=='reopened' or product['visit']==identity[0]),'Original document/visit replaced')
            if stage=='reopened':require(product['visit']!=identity[0],'Reopen reused retired visit')
            require(native.get('scope')==dict(visit=str(product['visit']),document=str(document['id']),revision=str(document['revision'])),
                    'Native nodes escaped document custody')
            expected=(WORKFLOW['title'],WORKFLOW['description']) if stage=='editor' else (TITLE,DESCRIPTION)
            require((document.get('title'),document.get('description'))==expected and (model.get('title'),model.get('description'))==expected,
                    'Permissions implicitly saved/replaced metadata draft')
            require(exact(document.get('editor'),WORKFLOW) and exact(document.get('trigger'),TRIGGER)
                    and exact(document.get('external'),EXTERNAL)
                    and exact(document.get('copy'),server['current']['copy'] if state['resolved'] else COPY)
                    and exact(document.get('run'),server['current']['run'] if state['resolved'] else RUN[case]),
                    'Baseline adopted without matching ACK and readback')
            require(document.get('pending')==(attempt_id if sealed else None)
                    and document.get('editable') is (not confirming and not busy and not sealed),'Unsafe draft unlocking')
            notice=(REOPENED if stage=='reopened' else VERIFIED if state['resolved'] else OBSERVED if state['checked'] else
                    CONFIRMED if state['complete'] else UNCONFIRMED) if state['finished'] else None
            require(document.get('notice')==model.get('notice')==notice,'Wrong operation/readback guidance')
        else:require(document is None and model.get('title') is None and model.get('description') is None,'Hidden draft leaked')
        confirmation=product.get('confirmation')
        if confirming:
            require(isinstance(confirmation,dict) and confirmation.get('current') is True,'Missing original confirmation')
            expected=dict(visit=product['visit'],document=document['id'],revision=document['revision'],title=TITLE,description=DESCRIPTION,
                          workflow='WF_REVIEW',originalCopy=COPY)
            require(all(exact(confirmation.get(k),v) for k,v in expected.items()),'Confirmation escaped original draft')
            integer(confirmation.get('id'),'Missing confirmation identity',1)
            draft_proof(confirmation,case,stage)
        else:require(confirmation is None,'Unexpected confirmation outside admission')
        controls=measured_native(native,geometry,viewport,rectangle)
        native_controls(native,controls,stage,route,confirming=confirming,busy=busy,sealed=sealed,resolved=state['resolved'],
                        viewport=viewport,rectangle=rectangle,
                        settings_validator=lambda n,c,_s,v,r:settings_native(n,c,confirmation,case,stage,v,r))
        messages=[f['text'] for f in native['feedback']]
        for message in ([model['notice']] if model.get('notice') else [])+(model.get('outcomes') or []):
            require(message in messages,'Actual operation feedback absent')
        path=Path(captures)/f'{fixture}-{stage}.png'
        require(paths[stage]==str(path),'Capture escaped owned stage path')
        if png:validate_png(path,geometry['pngDimensions'])
        if previous:
            old=previous['proof']['server']
            for key in ('requests','responses','settlements','releases'):
                require(exact(server[key][:len(old[key])],old[key]),'Original ledger history rewritten')
        previous=row
    require(exact(receipt.get('final'),observation(rows[-1])),'Terminal differs from final captured stage')
    from workflow_management_run_access_prerequisites_exercises import exercises
    exercises(receipt,case,rows,rectangle)
