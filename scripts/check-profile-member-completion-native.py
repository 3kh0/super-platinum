#!/usr/bin/env python3
"""Closed native Profile @member completion. Reuses the accepted fixture launcher.

This script does not build. Original PNGs still require visual inspection.
"""
import copy
from pathlib import Path
import runpy
import sys

# Bind the actual function globals, not runpy's returned shallow dictionary.
_loaded = runpy.run_path(str(Path(__file__).with_name('check-profile-channel-completion-native.py')))
B = _loaded['main'].__globals__
require = B['require']
BASE_STAGES = B['STAGES']
STAGES = BASE_STAGES[:3]+('refreshed-middle','refreshed-bottom')+BASE_STAGES[3:]
FIXTURES = B['FIXTURES']
MEMBER_FLAG = 'SUPER_PLATINUM_PROFILE_MEMBER_COMPLETION_CHECK'
FAILURE = 'People suggestions could not load. Edit your query to try again.'
LOCAL = '@Alex Local'
SAME = '@Alex Harbor'
FIRST, SECOND = 'UCOMPLETE1', 'UCOMPLETE2'


def initial():
    return [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[
        dict(type='text', text='Working with ', style=dict(bold=True)),
        dict(type='user', user_id='UMENTION01', fixture_occurrence=1, style=dict(italic=True), opaque=dict(keep=[1,'original'])),
        dict(type='user', user_id='UMENTION01', fixture_occurrence=2, opaque=dict(keep=[2,'original']))])])]


def elements(blocks):
    require(isinstance(blocks,list) and len(blocks)==1 and blocks[0].get('type')=='rich_text', 'Invalid member rich block')
    sections=blocks[0].get('elements')
    require(isinstance(sections,list) and len(sections)==1 and sections[0].get('type')=='rich_text_section', 'Invalid member section')
    values=sections[0].get('elements')
    require(isinstance(values,list) and values[:3]==initial()[0]['elements'][0]['elements'], 'Original opaque token/style changed')
    return values


def validate_native(native,count):
    require(isinstance(native,dict), 'Missing native resolved member capture')
    snap=native.get('snapshot',{})
    require(snap.get('composing') is False and type(snap.get('revision')) is int and snap['revision']>=0
            and isinstance(snap.get('generation'),str) and snap['generation'].isdigit(), 'Invalid member native custody')
    values=elements(snap.get('blocks'))
    tokens=[v for v in values if v.get('type')!='text']
    expected=copy.deepcopy(initial()[0]['elements'][0]['elements'][1:])
    if count>=3:expected.append(dict(type='user',user_id=FIRST))
    if count>=4:expected.append(dict(type='user',user_id=SECOND))
    require(tokens==expected and len(tokens)==count, 'Member IDs, occurrence order or opaque style changed')
    require(native.get('tokens')==[dict(value=v,label=l) for v,l in zip(tokens,[LOCAL,LOCAL,SAME,SAME])], 'Frozen member labels changed')
    return snap['blocks']


METHODS={'users/search','usergroups/search','users.prefs.get','experiments.getByUser','client.gantryBoot','client.userBoot',
         'api.getFlannelHttpUrl','drafts.list','drafts.listActive','search.autocomplete.model','search.autocomplete.offlineFeatures'}
def validate_source(source):
    require(isinstance(source,dict) and source.get('networkCalls')==0,'Missing closed member acquisition')
    requests=source.get('requests')
    require(isinstance(requests,list),'Missing member request ledger')
    for index,row in enumerate(requests,1):
        args=row.get('args')
        require(row.get('serial')==index and row.get('method') in METHODS and isinstance(args,dict),'Invalid member request identity')
        require(not {'token','enterprise_token','cookie','cookies'}.intersection(args),'Secret in fixture receipt')
        if row['method']=='users/search':
            require(set(args)=={'query','count','fuzz','enable_workspace_ranking','top_users','default_workspace'},'Incorrect member request options')
            require(isinstance(args['query'],str) and not args['query'].startswith('@') and args['count']==25 and args['fuzz']==1
                    and args['enable_workspace_ranking'] is True and args['top_users']==[] and args['default_workspace']=='TINIT0001',
                    'Member request diverged from Profile factory')
        elif row['method']=='usergroups/search':
            require(set(args)=={'query','count','org_wide'},'Incorrect group request options')
            require(isinstance(args['query'],str) and not args['query'].startswith('@')
                    and type(args['count']) is int and args['count']==25 and args['org_wide'] is True,
                    'Group request diverged from Profile factory')
    members=sum(r['method']=='users/search' for r in requests)
    groups=sum(r['method']=='usergroups/search' for r in requests)
    count=members+groups
    require(all(type(source.get(k)) is int and source[k]>=0 for k in ('searches','memberSearches','groupSearches','channelSearches')),
            'Invalid member/group read accounting')
    require(source['searches']==count and source['memberSearches']==members and source['groupSearches']==groups
            and source['channelSearches']==0,'Wrong member/group/channel read accounting')
    require(all(type(source.get(k)) is int and source[k]>=0 for k in ('active','completed','cancelled')),'Invalid member lifecycle counts')
    require(source['active']==count-source['completed'] and source['cancelled']<=source['completed']<=count,'Member lifecycle mismatch')
    require(isinstance(source.get('heldQueries'),list) and source.get('held') is bool(source['heldQueries']),'Held member reply mismatch')
    return requests


def popup_geometry(geometry,viewport):
    rows=geometry.get('resultRows')
    if rows is None:
        require(not geometry['rows'],'Popup inventory omitted')
        return []
    require(isinstance(rows,list) and rows,'Invalid full popup inventory')
    popup=geometry.get('popup',{})
    require(isinstance(popup.get('id'),str) and popup['id'] and type(popup.get('sequence')) is int and popup['sequence']>0,'Missing popup query identity')
    B['rectangle'](popup.get('rect'),viewport)
    height,total,top=(popup.get(k) for k in ('clientHeight','scrollHeight','scrollTop'))
    require(all(type(v) in (int,float) for v in (height,total,top)) and 0<height<=total and 0<=top<=total-height+1,'Invalid actual scroll metrics')
    require([r.get('index') for r in rows]==list(range(len(rows))) and len({r.get('id') for r in rows})==len(rows),'Duplicate or missing ordered result identity')
    for row in rows:
        require(isinstance(row.get('id'),str) and row['id'] and isinstance(row.get('text'),str) and type(row.get('visible')) is bool,'Missing full result label/visibility')
    require(geometry['rows']==[r for r in rows if r['visible']] and geometry['rows'],'Viewport does not equal measured visible subset')
    for row in geometry['rows']:
        rect=row['rect'];box=popup['rect']
        require(rect['top']>=box['top']-1 and rect['bottom']<=box['bottom']+1 and rect['left']>=box['left']-1 and rect['right']<=box['right']+1,'Viewport row clipped by popup')
    target=popup.get('targetIndex')
    require(target is None or (type(target) is int and any(r['index']==target for r in geometry['rows'])),'Scroll target not visibly reachable')
    return [r['text'] for r in rows]


def validate(fixture,receipt,captures,*,platform=sys.platform,png=True):
    require(fixture in FIXTURES and receipt.get('fixture')==fixture and receipt.get('ok') is True,
            f"Member native failure: {receipt.get('error','invalid owner or terminal')}")
    require(receipt.get('initialBlocks')==initial(),'Wrong member initial contract')
    saved=receipt.get('savedBlocks');elements(saved)
    stages=receipt.get('stages');paths=receipt.get('captures')
    require(isinstance(stages,list) and [s.get('stage') for s in stages]==list(STAGES),'Missing/reordered member stage')
    require(isinstance(paths,dict) and set(paths)==set(STAGES),'Missing member capture')
    viewport=B['expected_viewport'](fixture,platform)
    previous=[];attempts_before=[]
    for index,row in enumerate(stages):
        stage=row['stage'];model=row.get('model',{});geometry=row.get('geometry',{});proof=row.get('proof',{})
        logical='refreshed' if stage.startswith('refreshed') else stage
        index=BASE_STAGES.index(logical)
        B['validate_geometry'](geometry,viewport,'member-empty' if stage=='empty' else stage)
        labels=popup_geometry(geometry,viewport)
        if stage=='empty':require('No matching people' in [n.get('text') for n in geometry['notices']],'Missing people empty state')
        require(model.get('enabled') is True and model.get('busy') is False and model.get('uncertain') is False,'Invalid section state')
        require(model.get('managedValue')=='Preserved organization value' and model.get('choiceValue')=='Morning' and model.get('countryValue')=='US','Unrelated section field changed')
        require(model.get('blocks')==(initial() if index<5 else saved) and model.get('dirty') is False,'Stored draft changed without Use/save')
        attempts=model.get('attempts')
        require(isinstance(attempts,list) and len(attempts)==(0 if index<5 else 1) and model.get('reads')==(0 if index<5 else 1),'Unexpected section write/readback')
        require(attempts[:len(attempts_before)]==attempts_before,'Write history changed');attempts_before=attempts
        if attempts:
            fields={v.get('element_id'):v for v in attempts[0]}
            require(len(fields)==len(attempts[0]) and B['json'].loads(fields.get('long',{}).get('long_text',{}).get('blocks','null'))==saved,'Member section wire changed')
            require(fields.get('locked',{}).get('text',{}).get('text')=='Preserved organization value','Managed field wire changed')
        requests=validate_source(row.get('source'))
        require(requests[:len(previous)]==previous,'Request evidence changed');previous=requests
        count=2 if index<3 else 3 if stage=='keyboard' else 4
        if stage=='cancelled':require(geometry.get('native') is None and not geometry['tokens'],'Closed editor retained custody')
        else:
            blocks=validate_native(geometry.get('native'),count)
            if stage in ('mouse','saved'):require(blocks==saved,'Member native/readback blocks differ')
        require('@Alex Deleted Bot' not in labels,'Deleted bot incorrectly eligible')
        if stage=='local':require(labels==[LOCAL] and proof.get('held') is True and row['source']['active']==1,'Missing true local-first member result')
        if logical=='refreshed':
            require(labels.count(SAME)==2 and set(labels)=={LOCAL,SAME,'@Alex Retired','@Alex Self'},'Full eligible member results missing')
            require(proof.get('sequence')==stages[1]['proof'].get('sequence') and proof.get('release',{}).get('delivered') is True
                    and proof['release'].get('cancelledBeforeReply') is False,'Remote callback changed native query')
            members=proof.get('acceptedMembers');require(isinstance(members,list),'Missing actual shared Directory result')
            members={m.get('id'):m for m in members}
            for uid in (FIRST,SECOND,'UCOMPLETE3','UCOMPLETE4'):
                require(members.get(uid,{}).get('fixture_opaque')==dict(preserve=uid,ordered=['first','second']),'Shared source lost opaque member fields')
            require(members['UCOMPLETE3'].get('deleted') is True and members['UCOMPLETE4'].get('is_bot') is True,'Fixture eligibility source facts missing')
        if stage in ('keyboard','mouse'):
            expected=FIRST if stage=='keyboard' else SECOND;choice=proof.get('choice',{})
            require(proof.get('user')==expected and proof.get('keyboard') is (stage=='keyboard'),'Wrong native member selection path')
            require(choice.get('label')==SAME and choice.get('matches')==2 and choice.get('occurrence')==(0 if stage=='keyboard' else 1),'Identical-label ordinal not exercised')
        if stage=='saved':require(proof.get('exactReadback') is True and proof.get('expectedBlocks')==saved,'Missing member readback proof')
        if stage in ('query-retired','cancelled'):
            release=proof.get('release',{})
            require(release.get('delivered') is False and release.get('cancelledBeforeReply') is True
                    and release.get('query')==('alex-held' if stage=='query-retired' else 'alex-cancel'),'Wrong retired member reply')
            require(row['source']['cancelled']>=(1 if stage=='query-retired' else 2),'Cancelled member task remained active')
        if stage in ('malformed','failure'):require(proof.get('noWrite') is True and proof.get('retainedBlocks')==saved,'Failure changed stored member draft')
        if stage=='stale-refused':require(proof.get('refused') is True and type(proof.get('previousSequence')) is int and type(proof.get('nextSequence')) is int
                and proof['nextSequence']>proof['previousSequence'] and proof.get('exactNativeBlocks')==geometry['native']['snapshot']['blocks'],'Stale member catalog inserted')
        path=Path(paths[stage]);require(path.is_absolute() and path==captures.resolve()/f'{fixture}-{stage}.png','Foreign member capture path')
        if png:
            require(path.is_file() and not path.is_symlink(),'Missing original member PNG');B['validate_png'](path,geometry['pngDimensions'])
    require(receipt.get('model')==stages[-1]['model'],'Terminal member model drift')
    pages=stages[2:5]
    identities=[[(r['id'],r['text']) for r in p['geometry']['resultRows']] for p in pages]
    require(all(rows==identities[0] for rows in identities),'Scrolled popup changed ordered results')
    require(len(identities[0])==5,'Expected five eligible results')
    require(len({(p['geometry']['popup']['id'],p['geometry']['popup']['sequence']) for p in pages})==1,'Scrolled popup changed query/list owner')
    require([p['geometry']['popup']['targetIndex'] for p in pages]==[0,2,4],'Missing top/middle/bottom scroll targets')
    require({r['index'] for p in pages for r in p['geometry']['rows']}==set(range(5)),'Not every result is visibly reachable in captured pages')
    require(pages[0]['geometry']['popup']['scrollTop']==0 and all(pages[i]['geometry']['popup']['scrollTop']<=pages[i+1]['geometry']['popup']['scrollTop'] for i in range(2)),'Popup scrolling did not traverse top to bottom')
    bottom=pages[-1]['geometry']['popup']
    require(abs(bottom['scrollTop']-(bottom['scrollHeight']-bottom['clientHeight']))<=1,'Last popup frame did not reach bottom')
    require(all(p['source']==pages[0]['source'] and p['proof']==pages[0]['proof'] and p['geometry']['native']==pages[0]['geometry']['native'] for p in pages),'Scroll changed query source or native token custody')
    final=validate_source(receipt.get('source'));require(final[:len(previous)]==previous,'Terminal request history drift')
    queries={r['args']['query'] for r in final if r['method']=='users/search'}
    require({'alex','harbor','alex-held','no-malformed','no-offline','zz-no-matching-person','alex-cancel'}<=queries,'Missing actual member transport outcomes')


_NativeFixture=B['NativeFixture']
class MemberFixture(_NativeFixture):
    def __init__(self,fixture,log,binary=None,checks=None,**kwargs):
        checks={**(checks or {}),MEMBER_FLAG:'1','SUPER_PLATINUM_PROFILE_USER_MENTIONS_CHECK':'1'}
        super().__init__(fixture,log,binary,checks,**kwargs)

B.update(PREFIX='profile-member-completion-check: ',FAILURE=FAILURE,SCOPE='ordinary-member-completion',
         validate=validate,NativeFixture=MemberFixture,STAGES=STAGES)
# EDGE deliberately remains the shared capture helper's accepted prefix.
if __name__=='__main__': B['main']()
