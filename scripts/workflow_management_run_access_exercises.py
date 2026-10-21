"""Original interaction boundaries, including real target selection and retirement."""
from workflow_management_delete import TITLE, DESCRIPTION, exact, integer, require
from workflow_management_permissions import observation
from workflow_management_unpublish import native_fields
from workflow_management_run_access_ledger import COPY, combined, stage_state, server_proof
from workflow_management_run_access_picker import picker_proof, PAGES, DISCOVERY


def unchanged_buffers(before,after):
    for key in ('title','description'):
        require(exact({k:v for k,v in before['native']['fields'][key].items() if k!='readonly'},
                      {k:v for k,v in after['native']['fields'][key].items() if k!='readonly'}),
                'Settings action changed original native buffer/selection')


def selected(observation_value, ids, *, case, kinds, viewport, rectangle):
    from workflow_management_run_access import settings_native
    require(isinstance(observation_value,dict),'Missing actual target interaction observation')
    product=observation_value.get('permissions',{});native=observation_value.get('native',{})
    confirmation=product.get('confirmation',{});draft=confirmation.get('runDraft',{})
    require(confirmation.get('current') is True and draft.get('ids')==ids and product.get('attempts')==[],
            'Target selection is not an original unsent draft')
    counts=server_proof(observation_value.get('server'),case,stage_state(case,'targets'),
                       picker_proof=lambda p,s:picker_proof(p,s,kinds))
    model=observation_value.get('model',{})
    require(model.get('reads')==counts['reads'] and model.get('writes')==0,'Picker observation differs from actual ledger')
    native_fields(native,TITLE,DESCRIPTION,True)
    require(native.get('ok') is True and native.get('leasePresent') is True and native.get('composing') is False
            and native.get('viewport')==viewport and native.get('noOverflow') is True,'Picker lost original native lease')
    controls={c['id']:c for c in native.get('controls',[])}
    require(len(controls)==len(native['controls']),'Duplicate native control identity')
    settings_native(native,controls,confirmation,viewport,rectangle)


def target_exercise(row,case,viewport,rectangle):
    exercise=row['proof']['exercise'];current=observation(row)
    require(isinstance(exercise,dict),'Missing actual picker exercise')
    initial=exercise.get('initial');queried=exercise.get('queried')
    ids=['UWORKFLOW'];kinds=['Discovery','Members','Channels','InternalTeams']
    selected(initial,ids,case=case,kinds=[],viewport=viewport,rectangle=rectangle)
    selected(queried,ids,case=case,kinds=kinds,viewport=viewport,rectangle=rectangle)
    require(queried['permissions']['confirmation']['runDraft']['query']=='run','Actual internal query absent')
    require(queried['permissions']['confirmation']['runDraft']['receipts']==[DISCOVERY,PAGES['Members'],PAGES['Channels'],PAGES['InternalTeams']],
            'Product discarded/replaced original internal acquisition receipts')
    steps=exercise.get('selections')
    require(isinstance(steps,list) and len(steps)==3,'Missing actual person/channel/workspace clicks')
    prior=queried
    for step,(target,kind,label) in zip(steps,[('URUNADA01','User','Ada Runner'),('CRUNTEAM1','Channel','run-team'),('TRUNINNER','Team','Run internal')]):
        require(exact(step.get('before'),prior),'Target click did not start from original observation')
        require(exact(step.get('clicked'),dict(ok=True,target=target,kind=kind,label=label,connected=True,insideOriginalPanel=True,hit=True)),
                'Missing original result button/hit evidence')
        after=step.get('after');ids=ids+[target]
        selected(after,ids,case=case,kinds=kinds,viewport=viewport,rectangle=rectangle)
        require(exact(prior['server'],after['server']),'Target click dispatched or altered raw source')
        unchanged_buffers(prior,after);prior=after
    require(exact(exercise.get('beforeExternal'),prior),'External toggle lost original internal selection')
    external=exercise.get('external')
    selected(external,ids,case=case,kinds=kinds,viewport=viewport,rectangle=rectangle)
    require(external['permissions']['confirmation']['runDraft']['externalEnabled'] is True
            and exact(prior['server'],external['server']),'External toggle dispatched or lost choice')
    external_queried=exercise.get('externalQueried');kinds+=['Discovery','AvailableExternal','ExternalTeams']
    selected(external_queried,ids,case=case,kinds=kinds,viewport=viewport,rectangle=rectangle)
    require(external_queried['permissions']['confirmation']['runDraft']['externalQuery']=='run'
            and external_queried['permissions']['confirmation']['runDraft']['scope']['availableExternal']==['TRUNEXTER'],
            'Available organizations were fabricated from cached teams')
    step=exercise.get('externalSelected',{})
    require(exact(step.get('before'),external_queried)
            and exact(step.get('clicked'),dict(ok=True,target='TRUNEXTER',kind='Team',label='Run partner',connected=True,insideOriginalPanel=True,hit=True)),
            'External selected target not actual result')
    after=step.get('after');ids+=['TRUNEXTER']
    selected(after,ids,case=case,kinds=kinds,viewport=viewport,rectangle=rectangle)
    require(exact(after['server'],external_queried['server']),'External result click dispatched')
    require(exact(exercise.get('after'),current) and exact(current['server'],after['server'])
            and current['permissions']['confirmation']['selected']=='everyone','Final copy+run selection not original')
    unchanged_buffers(initial,current)


def exercises(receipt,case,rows,rectangle):
    by={r['stage']:r for r in rows};admission=None
    for index,row in enumerate(rows):
        stage=row['stage'];exercise=row['proof'].get('exercise');current=observation(row)
        require(isinstance(exercise,dict),'Missing actual interaction exercise')
        if stage in ('acknowledged','rejected','unconfirmed','followup-failed','held'):
            admission=exercise.get('admission')
            require(isinstance(admission,dict),'Missing final Save admission')
            if stage=='held':
                quiet=exercise.get('quiet',{})
                integer(quiet.get('observedMs'),'Missing bounded held interval',500)
                require(exact(quiet.get('before'),current) and exact(quiet.get('after'),current),'Held operation changed before release')
            else:require(set(exercise)=={'admission'},'Unexpected final Save exercise')
        elif stage in ('unchanged','cancelled','checked','checked-again','back'):
            before=observation(rows[index-1])
            require(exact(exercise.get('before'),before) and exact(exercise.get('after'),current),'Missing original before/after boundary')
            if stage in ('unchanged','cancelled'):
                require(exercise.get('action')==('workflow-permissions-save' if stage=='unchanged' else 'workflow-permissions-cancel')
                        and exact(before['server'],current['server']) and before['permissions']['attempts']==current['permissions']['attempts']==[],
                        'No-op/Cancel dispatched settings work')
                unchanged_buffers(before,current)
                a,b=before['permissions']['document'],current['permissions']['document']
                require(b['revision']>a['revision'] and exact({k:v for k,v in a.items() if k not in ('revision','editable')},
                        {k:v for k,v in b.items() if k not in ('revision','editable')}),'No-op/Cancel replaced original draft')
            elif stage=='back':
                require(exact(before['server'],current['server']) and current['native'].get('originalDetached') is True
                        and exact(before['permissions']['attempts'],current['permissions']['attempts']),'Back dispatched or lost custody')
            else:
                require(current['model']['reads']==before['model']['reads']+11 and current['model']['writes']==before['model']['writes'],
                        'Check lacks fresh read or replayed a mutation')
                a,b=before['permissions']['attempts'][0],current['permissions']['attempts'][0]
                require(exact({k:v for k,v in a.items() if k not in ('resolved','readback')},
                              {k:v for k,v in b.items() if k not in ('resolved','readback')}),'Readback rewrote original ACK/failure')
                require(exact(before['server']['current'],current['server']['current']),'Check changed server settings')
                require(current['permissions']['document']['revision']>before['permissions']['document']['revision'],'Check not a new owned observation')
                unchanged_buffers(before,current)
        elif stage=='targets':target_exercise(row,case,receipt['viewport'],rectangle)
        elif stage=='reopened':
            before=observation(by['checked'])
            require(exact(exercise.get('before'),before) and exact(exercise.get('after'),current)
                    and exact(exercise.get('remount'),dict(ok=True,originalDetached=True,newRoot=True,newTitle=True,newDescription=True)),
                    'Missing actual detached/remounted node evidence')
            require(exact(before['permissions']['attempts'],current['permissions']['attempts'])
                    and current['model']['reads']==before['model']['reads']+12,'Reopen rewrote attempt or omitted acquisition')
        elif stage=='successor':
            before=observation(rows[index-1]);replacement=exercise.get('replacement',{})
            require(exact(exercise.get('before'),before) and exact(exercise.get('after'),current)
                    and exact(replacement.get('before'),before['model']) and exact(replacement.get('after'),current['model'])
                    and replacement.get('foreignDocumentHidden') is True,'Missing genuine account retirement')
            integer(replacement.get('previousEpoch'),'Missing original account epoch')
            require(type(replacement.get('currentEpoch')) is int and replacement['currentEpoch']>replacement['previousEpoch']
                    and current['native'].get('originalDetached') is True,'Original owner/nodes not retired')
        elif stage=='late-settled':
            before=observation(by['successor'])
            require(exact(exercise.get('before'),before) and exact(exercise.get('after'),current)
                    and exercise.get('successorUnchanged') is True and exact(before['model'],current['model'])
                    and exact(before['native'],current['native']),'Late original response changed successor')
            require(all(exact(before['server'][k],current['server'][k]) for k in ('requests','responses','current')),
                    'Late response dispatched or altered original server')
            require(exact(exercise.get('release'),current['server']['releases'][-1]),'Release is not original held response')
        else:require(exercise=={},'Unexpected unvalidated exercise')
    require(isinstance(admission,dict),'No original Save admission')
    confirmation=admission.get('permissions',{}).get('confirmation',{})
    document=admission.get('permissions',{}).get('document',{})
    require(confirmation.get('current') is True and exact(confirmation.get('originalCopy'),COPY)
            and all(confirmation.get(k)==v for k,v in dict(visit=admission['permissions']['visit'],document=document['id'],
                revision=document['revision'],title=TITLE,description=DESCRIPTION,workflow='WF_REVIEW').items()),
            'Save escaped original settings admission')
    require(confirmation.get('selected')==('everyone' if combined(case) else 'collaborators'),'Wrong admitted copy change')
    ids=['UWORKFLOW','URUNADA01','CRUNTEAM1','TRUNINNER','TRUNEXTER'] if case=='run-named-combined' else []
    require(confirmation.get('runDraft',{}).get('ids')==ids and confirmation['runDraft'].get('mode')==('named_entities' if ids else 'everyone'),
            'Wrong admitted run targets')
    prior=observation(by['targets'] if case=='run-named-combined' else by['cancelled'] if case=='run-success' else by['selected'])
    require(exact(admission['server'],prior['server']),'Save admission dispatched before activation')
    if case!='run-success':require(exact(admission,prior),'Save differs from actual selected stage')
    else:
        require(confirmation.get('id')!=by['selected']['proof']['permissions']['confirmation']['id'],'Cancelled admission reused')
        unchanged_buffers(prior,admission)
    native_fields(admission['native'],TITLE,DESCRIPTION,True)
    require(admission['native'].get('leasePresent') is True and admission['native'].get('composing') is False,'Save lost native buffer lease')
    for row in rows:
        for attempt in row['proof']['permissions']['attempts']:
            require(all(attempt[k]==confirmation[k] for k in ('id','visit','document','revision','title','description','workflow')),
                    'Attempt escaped original admission')
        for request in row['proof']['server']['requests']:
            if request['phase'] in ('mutation','followup'):
                require(all(request['owner'].get(k)==confirmation[k] for k in ('visit','document','revision')),'Dispatch escaped original native source')
    expected=by['successor']['proof']['exercise']['replacement'] if 'successor' in by else None
    require(exact(receipt.get('replacement'),expected),'Terminal replacement changed')
