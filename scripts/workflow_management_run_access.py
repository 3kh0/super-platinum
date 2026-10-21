"""Independent product/native acceptance for the closed run-access scenarios."""
import copy
from pathlib import Path
from workflow_management_delete import TITLE, DESCRIPTION, exact, integer, require
from workflow_management_unpublish import measured_native, slot_proof
from workflow_management_permissions import (native_controls, settings_proof, observation, REOPENED, OBSERVED)
from workflow_management_run_access_ledger import (STAGES, COPY_METHOD, RUN_METHOD, RESTRICTION_METHOD,
    COPY, RUN, EXTERNAL, RESTRICTION, WORKFLOW, TRIGGER, combined, arguments, desired,
    response_for, stage_state, server_proof)
from workflow_management_run_access_picker import picker_proof

CONFIRMED='Permissions acknowledged. Check current permissions to verify the settings; your local draft is retained.'
UNCONFIRMED='Permissions were rejected or remain unconfirmed. Check current permissions; no request will be replayed. Your draft is retained.'
VERIFIED='Permissions are verified. Your local draft is retained.'
PICKERS=['Discovery','Members','Channels','InternalTeams','Discovery','AvailableExternal','ExternalTeams']


def attempt_proof(attempt,case,state,current):
    require(isinstance(attempt,dict),'Missing original permission attempt')
    for key in ('id','visit','document','revision'):integer(attempt.get(key),'Missing original '+key,1)
    require(attempt.get('title')==TITLE and attempt.get('description')==DESCRIPTION
            and attempt.get('workflow')=='WF_REVIEW' and exact(attempt.get('originalCopy'),COPY),
            'Attempt lost original document/source')
    changed=combined(case)
    require(attempt.get('copyChanged') is changed and attempt.get('desired')==('everyone' if changed else 'collaborators'),
            'Wrong changed-only copy group')
    for key in ('finished','complete','resolved'):
        require(attempt.get(key) is state[key],'Wrong attempt '+key)
    require(attempt.get('noSend') is False and attempt.get('primarySucceeded') is changed
            and attempt.get('current') is (not state['finished'] and not state['retired']),
            'Attempt application/ACK/currentness confused')
    slots=attempt.get('slots')
    require(isinstance(slots,list) and len(slots)==1,'Lost original copy slot')
    slot_proof(slots[0],0,COPY_METHOD,{'workflow_id':'WF_REVIEW','permission_type':'everyone' if changed else 'collaborators'},
               'acknowledged' if changed else 'pending',response_for(case,COPY_METHOD)['raw'] if changed else None,None)
    group=attempt.get('runGroup')
    require(isinstance(group,dict) and group.get('complete') is state['complete'],'Lost independent run group')
    phase=state['phase'];result=response_for(case,RUN_METHOD) if phase!='in-flight' else {'raw':None,'error':None}
    expected=dict(method=RUN_METHOD,arguments=arguments(case,RUN_METHOD),outcome=phase,raw=result['raw'],
                  failure='delivery unconfirmed' if phase=='unknown' else None)
    require(exact(group.get('primary'),expected),'Run response was rewritten or lost')
    follow=group.get('followup')
    if phase=='acknowledged':
        sent=state['follow'];success=sent and case!='run-followup-failure'
        outcome='acknowledged' if success else 'not-sent' if sent else 'pending'
        slot=dict(method=RESTRICTION_METHOD,arguments={'workflow_id':'WF_REVIEW'},outcome=outcome,
                  raw=RESTRICTION if success else None,failure='offline before dispatch' if sent and not success else None)
        require(exact(follow,dict(phase='Complete' if sent else 'Restriction',fullySucceeded=success,
                                 restriction=RESTRICTION if success else None,slots=[slot])),
                'Independent restriction response forged/lost or dispatched after retirement')
    else:require(follow is None,'Unacknowledged run authorized follow-up')
    readback=None
    if state['checked']:
        readback=dict({k:copy.deepcopy(v) for k,v in current.items() if k!='restriction'},matches=case!='run-rejected')
    require(exact(attempt.get('readback'),readback),'Fresh readback replaced ACK or ignored effective target sets')


def settings_native(native,controls,confirmation,viewport,rectangle):
    settings_proof(native,controls,confirmation['selected'],viewport,rectangle)
    value=native['settings'];draft=confirmation.get('runDraft')
    require(value.get('originalControls') is True,'Run controls no longer match original product-held native nodes')
    require(isinstance(draft,dict) and draft.get('pending') is False and draft.get('notice') is None,
            'Selected run draft not acquired')
    integer(draft.get('epoch'),'Missing target epoch',1)
    scope=draft.get('scope',{})
    require(exact({k:scope.get(k) for k in ('currentTeam','topLevel','unified','collaborators')},
                  dict(currentTeam='TWORKFLOW',topLevel='ERUNORG01',unified=False,collaborators=['UWORKFLOW'])),
            'Target partition escaped actual current team/organization')
    require(value.get('epoch')==str(draft['epoch']),'Native target controls escaped source epoch')
    run=value.get('run',{})
    require(run.get('id')=='workflow-permissions-run' and run.get('value')==draft['mode']
            and exact(run.get('options'),[
                {'value':'everyone','text':'Everyone internal in Home fixture','disabled':False},
                {'value':'named_entities','text':'Workflow managers, plus specific people, channels, and workspaces','disabled':False},
                {'value':'app_collaborators','text':'Workflow managers only','disabled':False}]),'Wrong native run modes')
    require(value.get('runLabel',{}).get('text')=='Who can find and use this workflow?','Wrong trigger-specific run label')
    require([r.get('target') for r in value.get('selected',[])]==draft['ids'],'Native selected targets differ from actual draft')
    expected_results=draft['results']+draft['externalResults']
    require([r.get('target') for r in value.get('results',[])]==expected_results,'Native search results differ from acquired targets')
    for key in ('query','externalQuery'):
        control=value.get(key)
        shown=draft['mode']=='named_entities' if key=='query' else draft['externalEnabled']
        require((control is not None) is shown,'Wrong target query visibility')
        if shown:require(control.get('value')==draft[key],'Native target query buffer changed')
    external=value.get('external')
    require((external is not None) is (draft['mode']!='app_collaborators'),'Wrong connected organization choice')
    if external:require(external.get('checked') is draft['externalEnabled'],'External control differs from target draft')
    samples=[value.get('run'),value.get('runLabel'),external,value.get('query'),value.get('externalQuery')]+value.get('selected',[])+value.get('results',[])
    for item in filter(lambda v:v is not None,samples):
        require(item.get('visible') is True and item.get('hit') is True and item.get('disabled') is False,
                'Run/target control hidden or disabled')
        rectangle(item.get('rect'),viewport)


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
        kinds=PICKERS if case=='run-named-combined' and stage not in ('about','editor','draft','named') else []
        counts=server_proof(server,case,state,picker_proof=lambda p,s:picker_proof(p,s,kinds))
        route='about' if stage in ('about','back') or state['retired'] else 'editor'
        confirming=stage in ('permissions','selected','named','targets')
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
            if combined(case):outcomes.append('Copy access: Acknowledged')
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
                    and exact(document.get('run'),server['current']['run'] if state['resolved'] else RUN),
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
            mode='app_collaborators' if stage=='permissions' else 'named_entities' if stage in ('named','targets') else 'everyone'
            ids=['UWORKFLOW'] if stage=='named' else ['UWORKFLOW','URUNADA01','CRUNTEAM1','TRUNINNER','TRUNEXTER'] if stage=='targets' else []
            draft=confirmation.get('runDraft',{})
            require(draft.get('mode')==mode and draft.get('ids')==ids and draft.get('externalEnabled') is (stage=='targets'),
                    'Wrong selected target partition or mode')
            require(confirmation.get('selected')==('everyone' if stage=='targets' or case=='run-rejected' else 'collaborators'),
                    'Wrong changed copy selection')
        else:require(confirmation is None,'Unexpected confirmation outside admission')
        controls=measured_native(native,geometry,viewport,rectangle)
        native_controls(native,controls,stage,route,confirming=confirming,busy=busy,sealed=sealed,resolved=state['resolved'],
                        viewport=viewport,rectangle=rectangle,
                        settings_validator=lambda n,c,_s,v,r:settings_native(n,c,confirmation,v,r))
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
    from workflow_management_run_access_exercises import exercises
    exercises(receipt,case,rows,rectangle)
