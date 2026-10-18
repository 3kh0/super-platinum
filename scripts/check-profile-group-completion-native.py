#!/usr/bin/env python3
"""Closed mixed Profile completion. Uses the accepted process/capture launcher."""
import copy
from pathlib import Path
import runpy
import sys

M=runpy.run_path(str(Path(__file__).with_name('check-profile-member-completion-native.py')))
B=M['B'];require=B['require'];FIXTURES=B['FIXTURES']
FLAG='SUPER_PLATINUM_PROFILE_GROUP_COMPLETION_CHECK'
RESTRICTED='SUPER_PLATINUM_PROFILE_GROUP_COMPLETION_RESTRICTED'
FIRST,SECOND,DELETED='SCOMPLETE1','SCOMPLETE2','SCOMPLETE3'
LOCAL,SAME,DOUBLE,REOPENED='@Alex Local','@Alex Harbor','@@Alex Café','@Alex Cafe\u0301'
STAGES=('opened','local','refreshed','refreshed-middle','refreshed-lower','refreshed-bottom','keyboard','mouse','saved','query-retired','malformed','failure','empty','cancelled','stale-refused')
SHORT=('opened','restricted','restricted-middle','restricted-bottom')
initial=M['initial'];elements=M['elements'];popup_geometry=M['popup_geometry']
FAILURE=M['FAILURE']

def native(value,count,reopened=False):
    require(isinstance(value,dict),'Missing native mixed capture')
    snap=value.get('snapshot',{})
    require(snap.get('composing') is False and type(snap.get('revision')) is int and snap['revision']>=0 and isinstance(snap.get('generation'),str) and snap['generation'].isdigit(),'Invalid native owner')
    values=elements(snap.get('blocks'));tokens=[v for v in values if v.get('type')!='text']
    wanted=copy.deepcopy(initial()[0]['elements'][0]['elements'][1:])
    if count>=3:wanted.append(dict(type='usergroup',usergroup_id=FIRST))
    if count>=4:wanted.append(dict(type='usergroup',usergroup_id=SECOND))
    require(tokens==wanted and len(tokens)==count,'Wrong group identity/order or original token data')
    labels=[LOCAL,LOCAL,SAME,REOPENED if reopened else DOUBLE]
    require(value.get('tokens')==[dict(value=v,label=l) for v,l in zip(tokens,labels)],'Wrong inserted/reopened prefix or Unicode label')
    return snap['blocks']

def source(value):
    require(isinstance(value,dict) and value.get('networkCalls')==0,'Missing closed acquisition')
    requests=value.get('requests');require(isinstance(requests,list),'Missing request ledger')
    for serial,row in enumerate(requests,1):
        method,args=row.get('method'),row.get('args')
        require(row.get('serial')==serial and method in M['METHODS']|{'usergroups/search'} and isinstance(args,dict),'Invalid request identity')
        require(not {'token','enterprise_token','cookie','cookies'}.intersection(args),'Secret in ledger')
        if method=='usergroups/search':
            require(set(args)=={'query','count','org_wide'} and isinstance(args['query'],str) and not args['query'].startswith('@') and args['count']==25 and args['org_wide'] is True,'Wrong actual group factory')
        if method=='users/search':
            require(set(args)=={'query','count','fuzz','enable_workspace_ranking','top_users','default_workspace'} and isinstance(args['query'],str) and args['count']==25 and args['fuzz']==1 and args['enable_workspace_ranking'] is True and args['top_users']==[] and args['default_workspace']=='TINIT0001','Wrong member factory')
    members=sum(r['method']=='users/search' for r in requests);groups=sum(r['method']=='usergroups/search' for r in requests)
    require(value.get('memberSearches')==members and value.get('groupSearches')==groups and value.get('channelSearches')==0 and value.get('searches')==members+groups,'Mixed search accounting drift')
    require(all(type(value.get(k)) is int and value[k]>=0 for k in ('active','completed','cancelled')) and value['active']==members+groups-value['completed'] and value['cancelled']<=value['completed']<=members+groups,'Invalid completion/retirement count')
    require(isinstance(value.get('heldQueries'),list) and value.get('held') is bool(value['heldQueries']),'Wrong held evidence')
    return requests

def choices(value,labels=None):
    require(isinstance(value,dict) and type(value.get('sequence')) is int and value['sequence']>0 and type(value.get('revision')) is int,'Missing exact query choice owner')
    rows=value.get('values');require(isinstance(rows,list),'Missing ordered mixed choices')
    if labels is not None:require([r.get('label') for r in rows]==labels,'Native result order differs from source catalog')
    return rows

def pages(stages,indices,total):
    frames=[stages[i] for i in indices];first=frames[0]
    inventory=[(r['id'],r['text']) for r in first['geometry']['resultRows']]
    require(len(inventory)==total,'Wrong result count')
    for frame in frames:
        require([(r['id'],r['text']) for r in frame['geometry']['resultRows']]==inventory,'Scrolling changed result order')
        require(frame['source']==first['source'] and frame['proof']==first['proof'] and frame['geometry']['native']==first['geometry']['native'],'Scrolling changed custody')
    require(len({(f['geometry']['popup']['id'],f['geometry']['popup']['sequence']) for f in frames})==1,'Scrolling replaced query')
    require([f['geometry']['popup']['targetIndex'] for f in frames]==list(range(0,total,2)),'Missing scroll targets')
    require({r['index'] for f in frames for r in f['geometry']['rows']}==set(range(total)),'Not all rows visibly reached')
    offsets=[f['geometry']['popup']['scrollTop'] for f in frames]
    require(offsets[0]==0 and offsets==sorted(offsets),'Wrong scroll traversal')
    last=frames[-1]['geometry']['popup'];require(abs(last['scrollTop']-(last['scrollHeight']-last['clientHeight']))<=1,'Bottom not reached')

def validate(fixture,receipt,captures,*,platform=sys.platform,png=True):
    require(fixture in FIXTURES and receipt.get('ok') is True and receipt.get('fixture')==fixture,f"Group fixture failed: {receipt.get('error')}")
    restricted=receipt.get('restricted');require(type(restricted) is bool,'Missing initial policy case')
    names=SHORT if restricted else STAGES;stages=receipt.get('stages');paths=receipt.get('captures')
    require(receipt.get('initialBlocks')==initial(),'Original token fixture changed')
    require(isinstance(stages,list) and [r.get('stage') for r in stages]==list(names) and isinstance(paths,dict) and set(paths)==set(names),'Missing/reordered stages')
    viewport=B['expected_viewport'](fixture,platform);previous=[];saved=receipt.get('savedBlocks')
    for i,row in enumerate(stages):
        stage=row['stage'];g=row.get('geometry',{});model=row.get('model',{});proof=row.get('proof',{})
        B['validate_geometry'](g,viewport,'member-empty' if stage=='empty' else stage)
        labels=popup_geometry(g,viewport)
        require(model.get('enabled') is True and model.get('busy') is False and model.get('uncertain') is False and model.get('dirty') is False,'Unexpected section state')
        require(model.get('managedValue')=='Preserved organization value' and model.get('choiceValue')=='Morning' and model.get('countryValue')=='US','Unrelated fields changed')
        saved_yet=not restricted and i>=8
        require(model.get('blocks')==(saved if saved_yet else initial()) and model.get('reads')==(1 if saved_yet else 0) and len(model.get('attempts',[]))==(1 if saved_yet else 0),'Unexpected section save/readback')
        if saved_yet:
            wire=model['attempts'][0];fields={v.get('element_id'):v for v in wire}
            require(len(fields)==len(wire) and B['json'].loads(fields.get('long',{}).get('long_text',{}).get('blocks','null'))==saved and fields.get('locked',{}).get('text',{}).get('text')=='Preserved organization value','Wrong exact section wire')
        requests=source(row.get('source'));require(requests[:len(previous)]==previous,'Request ledger changed');previous=requests
        if stage=='cancelled':require(g.get('native') is None and not g['tokens'],'Cancelled editor retained custody')
        else:
            count=2 if restricted or i<6 else 3 if stage=='keyboard' else 4
            blocks=native(g.get('native'),count,saved_yet)
            if stage in ('mouse','saved'):require(blocks==saved,'Exact blocks changed on save')
        require('@Alex Deleted Group' not in labels and '@Alex Deleted Bot' not in labels,'Deleted item admitted')
        if stage=='opened':
            initial_source=proof.get('initialSource',{});require(initial_source.get('rows')==[] and initial_source.get('selfRestricted') is restricted,'Not a genuine empty boot source/policy')
        if stage=='local':
            rows=choices(proof.get('choices'),labels)
            require(len(rows)==5 and all(v['value']['type']=='user' for v in rows) and labels.count(SAME)==2 and set(labels)=={LOCAL,SAME,'@Alex Retired','@Alex Self'} and proof.get('held') is True and row['source']['active']==1,'Missing current-member-first held group case')
        if stage.startswith('refreshed'):
            rows=choices(proof.get('choices'),labels)
            ids=[(v['value']['type'],v['value'].get('user_id',v['value'].get('usergroup_id'))) for v in rows]
            require(('usergroup',FIRST) in ids and ('usergroup',SECOND) in ids and ('user','UCOMPLETE1') in ids and ('user','UCOMPLETE2') in ids and labels.count(SAME)==3 and DOUBLE in labels,'Mixed equal-label identity missing')
            require(proof.get('sequence')==stages[1]['proof'].get('sequence') and proof.get('release',{}).get('delivered') is True,'Wrong group continuation')
            records=proof.get('sourceGroups',{}).get('rows');require(isinstance(records,list),'No accepted group source')
            records={r.get('source',{}).get('id'):r for r in records}
            for gid in (FIRST,SECOND,DELETED):
                require(records.get(gid,{}).get('source',{}).get('fixture_opaque')==dict(preserve=gid,ordered=['raw','effective']) and records[gid]['effective'].get('is_usergroup') is True,'Group source fields/projection lost')
            require(records[SECOND]['source'].get('handle')=='@Alex Cafe\u0301' and records[SECOND]['effective'].get('handle')=='@Alex Cafe\u0301','Source Unicode transformed')
            require(records[DELETED]['effective'].get('date_delete')==123,'Deleted source was silently dropped')
        if stage.startswith('restricted'):
            rows=choices(proof.get('choices'),labels);require(len(rows)==5 and all(v['value']['type']=='user' for v in rows) and proof.get('noGroupRequest') is True and row['source']['groupSearches']==0 and proof.get('sourceGroups',{}).get('selfRestricted') is True,'Restricted self dispatched/admitted groups')
        if stage in ('keyboard','mouse'):
            gid=FIRST if stage=='keyboard' else SECOND;choice=proof.get('choice',{});rows=choice.get('values',[]);index=choice.get('index')
            require(proof.get('group')==gid and proof.get('keyboard') is (stage=='keyboard') and choice.get('id')==gid and choice.get('type')=='usergroup' and type(index) is int and 0<=index<len(rows) and rows[index]['value']==dict(type='usergroup',usergroup_id=gid),'Wrong ordinal/category selection')
            require(choice.get('label')==(SAME if stage=='keyboard' else DOUBLE),'Wrong selected literal label')
        if stage=='saved':require(proof.get('exactReadback') is True and proof.get('reopenedLabel')==REOPENED,'Missing existing-token label distinction')
        if stage in ('query-retired','cancelled'):
            r=proof.get('release',{});require(r.get('delivered') is False and r.get('cancelledBeforeReply') is True and r.get('query')==('alex-held' if stage=='query-retired' else 'alex-cancel') and row['source']['cancelled']>=(1 if stage=='query-retired' else 2),'Late group callback retained authority')
        if stage in ('malformed','failure'):require(proof.get('noWrite') is True and proof.get('retainedBlocks')==saved and FAILURE in [v.get('text') for v in g['notices']],'Failure lost feedback/draft')
        if stage=='empty':require('No matching people' in [v.get('text') for v in g['notices']],'Missing empty feedback')
        if stage=='stale-refused':require(proof.get('refused') is True and type(proof.get('previousSequence')) is int and type(proof.get('nextSequence')) is int and proof['nextSequence']>proof['previousSequence'] and proof.get('exactNativeBlocks')==g['native']['snapshot']['blocks'],'Stale group accepted')
        path=Path(paths[stage]);require(path.is_absolute() and path==captures.resolve()/f'{fixture}-{stage}.png','Foreign capture path')
        if png:require(path.is_file() and not path.is_symlink(),'Missing original PNG');B['validate_png'](path,g['pngDimensions'])
    pages(stages,[1,2,3] if restricted else [2,3,4,5],5 if restricted else 7)
    require(receipt.get('model')==stages[-1]['model'],'Terminal model drift')
    final=source(receipt.get('source'));require(final[:len(previous)]==previous,'Terminal request drift')
    if restricted:require(not any(r['method']=='usergroups/search' for r in final),'Restricted remote group search')
    else:require({'alex','cafe','alex-held','no-malformed','no-offline','zz-no-matching-group','alex-cancel'}<={r['args']['query'] for r in final if r['method']=='usergroups/search'},'Missing actual group outcomes')

_NativeFixture=M['_NativeFixture']
class GroupFixture(_NativeFixture):
    def __init__(self,fixture,log,binary=None,checks=None,**kwargs):
        checks={**(checks or {}),FLAG:'1','SUPER_PLATINUM_PROFILE_USER_MENTIONS_CHECK':'1','SUPER_PLATINUM_PROFILE_MEMBER_COMPLETION_CHECK':'1'}
        if short_case:checks[RESTRICTED]='1'
        super().__init__(fixture,log,binary,checks,**kwargs)

short_case=False
B.update(PREFIX='profile-group-completion-check: ',FAILURE=FAILURE,SCOPE='mixed-group-completion',validate=validate,NativeFixture=GroupFixture,STAGES=STAGES)
if __name__=='__main__':
    if '--restricted' in sys.argv:
        sys.argv.remove('--restricted');short_case=True;B['STAGES']=SHORT
    B['main']()
