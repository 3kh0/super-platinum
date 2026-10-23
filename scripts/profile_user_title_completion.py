"""Independent query-owned Canvas user-title completion acceptance oracle."""
import copy
import json
from pathlib import Path
import sys
import profile_user_title as U
import profile_user_title_completion_source as S

require=U.require
STAGES={
    'known':('opened','query-ready','inserted','saved'),
    'hidden':('opened','query-ready','inserted','saved'),
    'recovery':('opened','member-discovery','member-held','unavailable','retry-member-discovery','retry-member-held','query-ready','inserted','saved'),
    'retirement':('opened','member-discovery','member-held','query-replaced','late-settled','replacement-member-discovery','replacement-member-held','cancelled'),
}
SERIALS={'member-discovery':1,'member-held':2,'retry-member-discovery':3,'retry-member-held':4,
         'replacement-member-discovery':3,'replacement-member-held':4}


def native(value,initial,token,label,stage,term):
    if stage=='cancelled':require(value is None,'Cancelled editor retained native authority');return None
    require(isinstance(value,dict),'Missing original editor capture')
    snap=value.get('snapshot',{})
    require(snap.get('composing') is False and type(snap.get('revision')) is int and snap['revision']>=0
        and isinstance(snap.get('generation'),str) and snap['generation'].isdigit(),'Original native editor custody missing')
    blocks=snap.get('blocks')
    require(isinstance(blocks,list) and len(blocks)==1 and blocks[0].get('type')=='rich_text','Wrong native rich tree')
    sections=blocks[0].get('elements')
    require(isinstance(sections,list) and len(sections)==1 and sections[0].get('type')=='rich_text_section','Wrong native paragraph')
    values=sections[0].get('elements');original=initial[0]['elements'][0]['elements']
    require(isinstance(values,list) and values[:3]==original,'Original text/opaque member occurrences changed')
    inserted=stage in ('inserted','saved')
    tokens=[v for v in values if v.get('type')!='text']
    require(tokens==original[1:]+([token] if inserted else []),'Private selection inserted a wrong ID/URL or duplicate token')
    require(value.get('tokens')==[dict(value=v,label=l) for v,l in zip(tokens,['@Alex Local','@Alex Local']+([label] if inserted else []))],
        'Mounted original/user-title labels changed or replaced wire values')
    tail=[v for v in values[3:] if v.get('type')=='text']
    require(all(isinstance(v.get('text'),str) for v in tail),'Invalid actual typed query text')
    text=''.join(v['text'] for v in tail)
    if inserted:require(not text.strip(),'Selected query text remained in saved rich body')
    elif stage=='opened':require(blocks==initial,'Original editor seeded with a new query/token')
    else:require(text==' '+term,'Original native query replaced or lost while waiting')
    return blocks


def catalog(value,labels,case,stage,requests,baseline):
    if value is None:
        require(not labels,'Native popup has no original private catalog')
        return
    require(isinstance(value,dict) and type(value.get('sequence')) is int and value['sequence']>0
        and type(value.get('revision')) is int and value['revision']>=0,'Original query sequence/revision missing')
    rows=value.get('values')
    require(isinstance(rows,list) and [r.get('label') for r in rows]==labels,'Native popup order/labels differ from original private choices')
    require(all(isinstance(r.get('value'),dict) for r in rows),'Private choice values missing')
    files=[r for r in rows if r['value'].get('type') in ('canvas','file')]
    if stage=='query-ready':
        wanted={(r['value'].get('type'),r['value'].get('file_id')) for r in files}
        require(wanted=={('canvas','FCOMPLETE1'),('file','FCOMPLETE2')} and len(files)==2,'Hosted/missing/duplicated file choice')
        people=[r['value'] for r in rows if r['value'].get('type')=='user']
        require({r.get('user_id') for r in people}=={'UMENTION01','UCOMPLETE1','UCOMPLETE2','UCOMPLETE3','UINIT0001'}
            and len(people)==5 and len(rows)==7,'Original mixed people/file category identities changed')
        for r in files:
            fid=r['value']['file_id'];href=f'https://fixture.invalid/files/{fid}'
            require(r['value']==dict(type='canvas' if fid=='FCOMPLETE1' else 'file',file_id=fid,text=href,url=href)
                and r['label']==(U.label(case) if fid=='FCOMPLETE1' else 'Alex List'),'Canonical file choice lost title/wire separation')
        title=value.get('titleUsers')
        require(isinstance(title,dict) and set(title)=={'FCOMPLETE1'},'Nested user subset borrowed/wrong file')
        S.title_proof(title['FCOMPLETE1'],case,value['sequence'],requests)
        baseline['readyChoices']=copy.deepcopy(value)
    else:
        require(not any(r['value'].get('file_id')=='FCOMPLETE1' for r in rows) and not value.get('titleUsers'),
            'Unresolved or retired nested member became a selectable Canvas')
    group='project' if stage in ('query-replaced','late-settled') else 'al' if stage.startswith(('retry-','replacement-')) or (case=='recovery' and stage=='query-ready') else 'alex'
    previous=baseline.get('queryGroup')
    if previous==group:require(value['sequence']==baseline['sequence'],'Asynchronous dependency completion replaced original query')
    elif previous is not None:require(value['sequence']>baseline['sequence'],'New input reused retired query sequence')
    baseline.update(queryGroup=group,sequence=value['sequence'])


def validate(fixture,receipt,captures,h,*,case,platform=sys.platform,png=True):
    base=h['B'];initial=h['initial']();token=h['token']('FCOMPLETE1')
    require(case in STAGES and fixture in base['FIXTURES'] and isinstance(receipt,dict) and receipt.get('ok') is True
        and receipt.get('fixture')==fixture and receipt.get('case')==case,'Wrong original completion case/terminal')
    require(receipt.get('initialBlocks')==initial,'Original rich initial member tokens changed')
    rows=receipt.get('stages');paths=receipt.get('captures');names=STAGES[case]
    require(isinstance(rows,list) and [r.get('stage') for r in rows]==list(names) and isinstance(paths,dict)
        and set(paths)==set(names),'Missing/reordered query-owned stages or captures')
    viewport=base['expected_viewport'](fixture,platform);baseline={};previous=None
    for row in rows:
        stage=row['stage'];model=row.get('model',{});proof=row.get('proof',{});geometry=row.get('geometry',{})
        exercise=proof.get('exercise',{});saved=stage=='saved'
        require(model.get('enabled') is True and model.get('account')==S.ACCOUNT and type(model.get('epoch')) is int
            and type(model.get('visit')) is int and type(model.get('serial')) is int,'Original Profile owner missing')
        require(model.get('busy') is False and model.get('uncertain') is False and model.get('dirty') is False
            and model.get('managedValue')=='Preserved organization value' and model.get('choiceValue')=='Morning'
            and model.get('countryValue')=='US','Unrelated original draft/state changed')
        if previous:
            require((model['epoch'],model['visit'])==(previous['epoch'],previous['visit'])
                and ((model['serial']!=previous['serial']) if saved else model['serial']==previous['serial']),
                'Only actual Save/readback may replace original section serial')
        require(model.get('blocks')==(receipt.get('savedBlocks') if saved else initial) and model.get('reads')==int(saved)
            and isinstance(model.get('attempts'),list) and len(model['attempts'])==int(saved),'Unexpected Save/readback/draft change')
        if saved:
            wire=model['attempts'][0];fields={v.get('element_id'):v for v in wire}
            require(len(fields)==len(wire) and json.loads(fields.get('long',{}).get('long_text',{}).get('blocks','null'))==receipt['savedBlocks']
                and fields.get('locked',{}).get('text',{}).get('text')=='Preserved organization value','Actual Save wire differs from original inserted rich blocks')
        title=proof.get('titleCompletion',{})
        S.nested(title,model,case,stage,exercise,baseline)
        requests=S.completion(row.get('source'),case,stage,title['fileResponseBytes'],baseline)
        base['validate_geometry'](geometry,viewport,stage)
        labels=h['popup_geometry'](geometry,viewport)
        query=S.queries(case,stage)
        term=('#project' if query and query[-1]=='project' else '@'+query[-1]) if query else ''
        blocks=native(geometry.get('native'),initial,token,U.label(case),stage,term)
        if stage!='cancelled':
            require([t.get('text') for t in geometry['tokens']]==['@Alex Local','@Alex Local']+([U.label(case)] if stage in ('inserted','saved') else []),
                'Actual mounted token text differs from original native capture')
        else:require(not geometry['tokens'],'Cancelled native tokens remain')
        choices=proof.get('choices');catalog(choices,labels,case,stage,requests,baseline)
        if stage=='opened':require(exercise==dict(initialBlocks=initial) and choices is None,'Unrequested initial completion')
        elif stage in SERIALS:require(exercise==dict(serial=SERIALS[stage]),'Wrong actual held callback serial')
        elif stage=='unavailable':
            require(exercise==dict(failedMember=2) and h['FAILURE'] in [n.get('text') for n in geometry['notices']],
                'Offline nested acquisition lost exact guidance/draft')
        elif stage=='query-ready':
            observed=exercise.get('native',{})
            require(exercise.get('choices')==choices and observed.get('ok') is True and observed.get('sequence')==choices['sequence']
                and observed.get('serial')==str(model['serial']) and observed.get('native')==geometry['native'],
                'Original query DOM/private catalog/native editor did not agree')
            require(observed.get('rows')==[dict(id=r['id'],label=r['text'],value=v['value'])
                for r,v in zip(geometry['resultRows'],choices['values'])],'Original option node/order/value proof changed')
            h['ordered_source'](dict(fileResponseBytes=title['fileResponseBytes'],sourceFiles=proof.get('sourceFiles')))
            file=proof['sourceFiles']['rows'][0]
            require(file['source']['title_blocks']==U.title_blocks() and file['effective'].get('title_blocks')==U.title_blocks()
                and file['effective'].get('title')=='Alex Canvas','Actual canonical file source/rank input altered before private title projection')
        elif stage=='inserted':
            choice=exercise.get('choice',{});old=baseline.get('readyChoices',{});values=old.get('values',[]);index=choice.get('index')
            require(choice.get('ok') is True and choice.get('id')=='FCOMPLETE1' and choice.get('type')=='canvas'
                and choice.get('keyboard') is True and choice.get('sequence')==old.get('sequence') and choice.get('values')==values
                and type(index) is int and 0<=index<len(values) and values[index]==dict(value=token,label=U.label(case))
                and choice.get('label')==U.label(case),'Original keyboard choice did not select exact private Canvas identity')
            require(choices is None and exercise.get('expectedBlocks')==blocks==receipt.get('savedBlocks'),'Insertion lost native draft or retained consumed query')
        elif stage=='saved':
            require(exercise==dict(expectedBlocks=blocks,exactReadback=True) and blocks==receipt.get('savedBlocks') and choices is None,
                'Exact Save/readback/reopen did not preserve inserted wire')
        elif stage in ('query-replaced','late-settled'):
            require(exercise.get('replacement')==choices and len(labels)==2
                and set(labels)=={'#project-completion','#project-cove'},'Replacement channel query lost its original catalog')
            if stage=='query-replaced':
                require(exercise.get('heldMember')==2,'Wrong held original query member')
                baseline['replacement']=copy.deepcopy(choices)
            else:require(choices==baseline['replacement'],'Late original member changed replacement choice authority')
        elif stage=='cancelled':
            require(exercise.get('expectedBlocks')==initial and choices is None and 'savedBlocks' not in receipt,
                'Cancel retained private query/changed original draft')
        path=Path(captures).resolve()/f'{fixture}-{stage}.png'
        require(isinstance(paths[stage],str) and Path(paths[stage])==path,'Foreign/missing original capture path')
        if png:
            require(path.is_file() and not path.is_symlink(),'Missing original captured PNG')
            base['validate_png'](path,geometry['pngDimensions'])
        previous=model
    require(receipt.get('model')==rows[-1]['model'] and receipt.get('source')==rows[-1]['source']
        and receipt.get('titleCompletion')==rows[-1]['proof']['titleCompletion'],'Terminal original model/source history changed')
    return receipt
