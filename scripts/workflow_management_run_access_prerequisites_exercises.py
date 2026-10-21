"""Native before/after evidence for actual prerequisite Retry and held release."""
from workflow_management_delete import TITLE, DESCRIPTION, exact, integer, require
from workflow_management_permissions import observation
from workflow_management_unpublish import native_fields
from workflow_management_run_access_exercises import unchanged_buffers


def quiet(value,current):
    require(isinstance(value,dict),'Missing original held/failed interval')
    integer(value.get('observedMs'),'Quiet interval shorter than 500ms',500)
    require(exact(value.get('before'),current) and exact(value.get('after'),current),'Automatic dispatch or replacement during quiet interval')


def exercises(receipt,case,rows,rectangle):
    by={r['stage']:r for r in rows};admission=None
    for index,row in enumerate(rows):
        stage=row['stage'];exercise=row['proof'].get('exercise');current=observation(row)
        require(isinstance(exercise,dict),'Missing actual native interaction')
        before=observation(rows[index-1]) if index else None
        if stage=='missing-offline':quiet(exercise,current)
        elif stage in ('retry-held','available-held'):
            require(set(exercise)=={'admission','quiet'} and exact(exercise.get('admission'),before),'Prerequisite admission not from original controls')
            quiet(exercise['quiet'],current);unchanged_buffers(before,current)
            require(before['permissions']['confirmation']['id']==current['permissions']['confirmation']['id']
                    and exact(before['permissions']['document'],current['permissions']['document']),'Read replaced original admission/draft')
            require(current['server']['held'] is not None and current['model']['writes']==0,'Held read secretly changed permissions')
        elif stage=='hydrated' or stage=='selected' and case=='run-everyone-external':
            require(set(exercise)=={'before','release','after'} and exact(exercise.get('before'),before)
                    and exact(exercise.get('after'),current),'Original held completion observation changed')
            require(exact(exercise.get('release'),current['server']['releases'][-1]),'Release was not actual original sender')
            require(exercise['release']['serial']==before['server']['held'] and exercise['release']['delivered'] is True
                    and exercise['release']['receiverRetired'] is False,'Original live receiver not used')
            unchanged_buffers(before,current)
            require(exact(before['permissions']['document'],current['permissions']['document'])
                    and current['permissions']['attempts']==[],'Read applied settings or changed metadata')
        elif stage=='selected':
            require(case=='run-cold-targets' and set(exercise)=={'before','after'} and exact(exercise.get('before'),before)
                    and exact(exercise.get('after'),current) and exact(before['server'],current['server']),'Mode change dispatched or fabricated targets')
            unchanged_buffers(before,current)
        elif stage=='acknowledged':
            require(set(exercise)=={'admission'} and exact(exercise.get('admission'),before),'Save escaped exact selected original controls')
            admission=exercise['admission']
        elif stage in ('checked','back'):
            require(exact(exercise.get('before'),before) and exact(exercise.get('after'),current),'Missing original Check/Back boundary')
            if stage=='back':
                require(exact(before['server'],current['server']) and current['native'].get('originalDetached') is True
                        and exact(before['permissions']['attempts'],current['permissions']['attempts']),'Back dispatched/lost original attempt')
            else:
                require(current['model']['reads']==before['model']['reads']+11 and current['model']['writes']==before['model']['writes']
                        and exact(before['server']['current'],current['server']['current']),'Check replayed or omitted fresh read')
                a,b=before['permissions']['attempts'][0],current['permissions']['attempts'][0]
                require(exact({k:v for k,v in a.items() if k not in ('resolved','readback')},
                              {k:v for k,v in b.items() if k not in ('resolved','readback')}),'Readback rewrote original receipts')
                require(current['permissions']['document']['revision']>before['permissions']['document']['revision'],'Check not newly owned')
                unchanged_buffers(before,current)
        elif stage=='reopened':
            prior=observation(by['checked'])
            require(exact(exercise.get('before'),prior) and exact(exercise.get('after'),current)
                    and exact(exercise.get('remount'),dict(ok=True,originalDetached=True,newRoot=True,newTitle=True,newDescription=True)),
                    'Reopen not actual original detach/new mount')
            require(exact(prior['permissions']['attempts'],current['permissions']['attempts'])
                    and current['model']['reads']==prior['model']['reads']+12,'Reopen replayed or lost acquisition')
        else:require(exercise=={},'Unexpected interaction proof')
    require(isinstance(admission,dict),'Missing original Save admission')
    confirmation=admission['permissions']['confirmation']
    native_fields(admission['native'],TITLE,DESCRIPTION,True)
    require(admission['native'].get('leasePresent') is True and admission['native'].get('composing') is False,'Save not native-owned')
    for row in rows:
        for attempt in row['proof']['permissions']['attempts']:
            require(all(attempt[k]==confirmation[k] for k in ('id','visit','document','revision','title','description','workflow')),
                    'Attempt not original selected admission')
        for request in row['proof']['server']['requests']:
            if request['phase'] in ('mutation','followup'):
                require(all(request['owner'].get(k)==confirmation[k] for k in ('visit','document','revision')),'Dispatch escaped original native custody')
    require(receipt.get('replacement') is None,'Unrequested owner retirement')
