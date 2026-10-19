#!/usr/bin/env python3
"""Closed mixed Canvas/List picker acceptance; never builds or sends Slack IO."""
import copy
import json
from pathlib import Path
import runpy
import sys

G=runpy.run_path(str(Path(__file__).with_name('check-profile-group-completion-native.py')))
M=G['M'];B=G['B'];require=B['require'];FIXTURES=B['FIXTURES']
FLAG='SUPER_PLATINUM_PROFILE_FILE_COMPLETION_CHECK'
OFF='SUPER_PLATINUM_PROFILE_FILE_COMPLETION_OFF'
CANVAS,LIST,HOSTED='FCOMPLETE1','FCOMPLETE2','FCOMPLETE3'
CANVAS_LABEL,LIST_LABEL='Alex Canvas','Alex List'
STAGES=('opened','local-held','mixed','mixed-middle','mixed-lower','mixed-bottom','keyboard','mouse','saved','query-retired','malformed','failure','cancelled')
SHORT=('opened','disabled','disabled-middle','disabled-bottom')
FAILURE=M['FAILURE'];initial=M['initial'];elements=M['elements'];popup_geometry=M['popup_geometry']

def token(fid):
    href=f'https://fixture.invalid/files/{fid}'
    return dict(type='canvas' if fid==CANVAS else 'file',file_id=fid,url=href,text=href)

def native(value,count):
    require(isinstance(value,dict),'Missing native resolved capture')
    snap=value.get('snapshot',{})
    require(snap.get('composing') is False and type(snap.get('revision')) is int and snap['revision']>=0 and isinstance(snap.get('generation'),str) and snap['generation'].isdigit(),'Invalid native custody')
    values=elements(snap.get('blocks'));tokens=[v for v in values if v.get('type')!='text']
    wanted=copy.deepcopy(initial()[0]['elements'][0]['elements'][1:])
    if count>=3:wanted.append(token(CANVAS))
    if count>=4:wanted.append(token(LIST))
    require(tokens==wanted and len(tokens)==count,'Wrong file identity/URL/opaque token/order')
    require(value.get('tokens')==[dict(value=v,label=l) for v,l in zip(tokens,['@Alex Local','@Alex Local',CANVAS_LABEL,LIST_LABEL])],'Canonical mounted label changed')
    return snap['blocks']

def source(value):
    require(isinstance(value,dict),'Missing file acquisition ledger')
    requests=value.get('requests');require(isinstance(requests,list),'Missing requests')
    for serial,row in enumerate(requests,1):
        require(row.get('serial')==serial,'Request serial drift')
        method,args=row.get('method'),row.get('args')
        require(isinstance(args,dict) and not {'token','enterprise_token','cookie','cookies'}.intersection(args),'Missing/unsafe request options')
        if method=='search.autocomplete.files':
            require(set(args)=={'query','include_shares','_x_reason'} and isinstance(args['query'],str) and not args['query'].startswith('@') and args['include_shares']=='true' and args['_x_reason']=='texty-autocomplete','Wrong file request factory')
        elif method=='files.info':
            require(args==dict(file=args.get('file'),page='1',count='500',truncate='true') and args.get('file') in (CANVAS,LIST),'Wrong canonical title request')
        else:require(method in M['METHODS'],'Unexpected fixture endpoint')
    files=sum(r['method']=='search.autocomplete.files' for r in requests)
    members=sum(r['method']=='users/search' for r in requests);groups=sum(r['method']=='usergroups/search' for r in requests)
    count=files+members+groups
    require(type(value.get('fileSearches')) is int and value['fileSearches']==files and value.get('searches')==count,'Wrong mixed file count')
    require(value.get('memberSearches')==members and value.get('groupSearches')==groups and value.get('channelSearches')==0,'Wrong other category count')
    require(value.get('networkCalls')==0 and all(type(value.get(k)) is int and value[k]>=0 for k in ('active','completed','cancelled')) and value['active']==count-value['completed'] and value['cancelled']<=value['completed']<=count,'Invalid read lifecycle')
    require(isinstance(value.get('heldQueries'),list) and value.get('held') is bool(value['heldQueries']),'Wrong held evidence')
    # Reuse the exact member/group endpoint grammar without treating file reads
    # as their completion counters or weakening that existing validator.
    ordinary=copy.deepcopy(value)
    ordinary['requests']=[r for r in ordinary['requests'] if r['method'] not in ('search.autocomplete.files','files.info')]
    for i,row in enumerate(ordinary['requests'],1):row['serial']=i
    ordinary.update(searches=members+groups,active=0,completed=members+groups,cancelled=0,held=False,heldQueries=[])
    M['validate_source'](ordinary)
    return requests

def choices(value,labels):
    rows=G['choices'](value,labels)
    require(all(r.get('value',{}).get('file_id')!=HOSTED for r in rows),'Hosted file became selectable')
    for row in rows:
        if row['value'].get('type') in ('canvas','file'):
            fid=row['value'].get('file_id');require(fid in (CANVAS,LIST) and row['value']==token(fid),'Noncanonical file wire in private catalog')
    return rows

def ordered_source(proof):
    raw_bytes=proof.get('fileResponseBytes')
    require(isinstance(raw_bytes,str),'Missing original response byte string')
    def object_pairs(pairs):
        require(len({key for key,_ in pairs})==len(pairs),'Unexpected duplicate fixture key')
        return dict(pairs)
    response=json.loads(raw_bytes,object_pairs_hook=object_pairs)
    require(response.get('ok') is True and isinstance(response.get('files'),list),'Invalid original file response')
    raw_rows=response['files'];require([r.get('id') for r in raw_rows]==[CANVAS,LIST,HOSTED],'Wrong original file occurrence order')
    expected_keys={CANVAS:['z-last','a-first','m-foreign'],LIST:['y-last','b-first','x-foreign']}
    expected_channels={CANVAS:['CZMAP001','CAMAP001','CTAIL001'],LIST:['CYMAP002','CBMAP002']}
    source_files=proof.get('sourceFiles',{})
    require(source_files.get('unavailableIds')==[] and source_files.get('excludedIds')==[HOSTED],'Shared files unavailable or hosted exclusion lost')
    rows=source_files.get('rows');require(isinstance(rows,list) and [r.get('id') for r in rows]==[CANVAS,LIST],'Missing actual accepted candidate rows')
    for raw,row in zip(raw_rows,rows):
        fid=raw['id'];mapping=raw.get('channel_map')
        require(isinstance(mapping,dict) and list(mapping)==expected_keys[fid],'Original string-key encounter order lost')
        require(list(mapping)!=sorted(mapping),'Fixture no longer distinguishes map order')
        require(row.get('source')==raw,'Accepted source differs from original receipt')
        selected=[]
        for entry in mapping.values():
            if entry.get('is_org_shared') or entry.get('team')=='TINIT0001':
                if entry['id'] not in selected:selected.append(entry['id'])
        for cid in raw.get('channels',[]):
            if cid not in selected:selected.append(cid)
        require(selected==expected_channels[fid] and row.get('effective',{}).get('channels')==selected,'Effective shared channels lost original ordered union')

def validate(fixture,receipt,captures,*,platform=sys.platform,png=True):
    require(fixture in FIXTURES and receipt.get('ok') is True and receipt.get('fixture')==fixture,f"File fixture failed: {receipt.get('error')}")
    disabled=receipt.get('disabled');require(type(disabled) is bool,'Missing observed gate case')
    stages=receipt.get('stages');paths=receipt.get('captures');names=SHORT if disabled else STAGES
    require(isinstance(stages,list) and [r.get('stage') for r in stages]==list(names) and isinstance(paths,dict) and set(paths)==set(names),'Missing/reordered file stages')
    require(receipt.get('initialBlocks')==initial(),'Original token fixture changed')
    viewport=B['expected_viewport'](fixture,platform);saved=receipt.get('savedBlocks');previous=[]
    for i,row in enumerate(stages):
        stage=row['stage'];g=row.get('geometry',{});model=row.get('model',{});proof=row.get('proof',{})
        B['validate_geometry'](g,viewport,stage);labels=popup_geometry(g,viewport)
        require(model.get('enabled') is True and model.get('busy') is False and model.get('uncertain') is False and model.get('dirty') is False,'Unexpected section state')
        require(model.get('managedValue')=='Preserved organization value' and model.get('choiceValue')=='Morning' and model.get('countryValue')=='US','Unrelated fields changed')
        saved_yet=not disabled and i>=8
        require(model.get('blocks')==(saved if saved_yet else initial()) and model.get('reads')==(1 if saved_yet else 0) and len(model.get('attempts',[]))==(1 if saved_yet else 0),'Unexpected save/readback')
        if saved_yet:
            wire=model['attempts'][0];fields={v.get('element_id'):v for v in wire}
            require(len(fields)==len(wire) and B['json'].loads(fields.get('long',{}).get('long_text',{}).get('blocks','null'))==saved and fields.get('locked',{}).get('text',{}).get('text')=='Preserved organization value','Wrong exact section wire')
        requests=source(row.get('source'));require(requests[:len(previous)]==previous,'Request ledger changed');previous=requests
        if disabled:require(row['source']['fileSearches']==0,'Disabled gate dispatched files')
        if stage=='cancelled':require(g.get('native') is None and not g['tokens'],'Cancelled editor retained custody')
        else:
            count=2 if disabled or i<6 else 3 if stage=='keyboard' else 4
            blocks=native(g.get('native'),count)
            if stage in ('mouse','saved'):require(blocks==saved,'Save/readback wire changed')
        require('Alex Hosted' not in labels and '@Alex Deleted Bot' not in labels,'Excluded category admitted')
        if stage=='local-held':
            rows=choices(proof.get('choices'),labels)
            require(len(rows)==5 and all(v['value']['type']=='user' for v in rows) and labels.count('@Alex Harbor')==2 and row['source']['active']==1 and row['source']['held'] is True,'Held file blocked people')
            require(proof.get('beforeFiles')==row['source'],'Wrong pre-file read witness')
        if stage.startswith('mixed'):
            ordered_source(proof)
            rows=choices(proof.get('choices'),labels)
            require(len(rows)==7 and sum(v['value']['type']=='user' for v in rows)==5 and {v['value'].get('file_id') for v in rows if v['value']['type'] in ('canvas','file')}=={CANVAS,LIST},'Missing mixed category identities')
            before=proof.get('beforeFiles',{});require(before==stages[1]['source'] and source(before),'Wrong late publication predecessor')
            require(proof.get('sameQuery') is True and proof['choices']['sequence']==stages[1]['proof']['choices']['sequence'] and proof.get('release',{}).get('delivered') is True,'Late files replaced query')
            require([row['source'][k] for k in ('memberSearches','groupSearches')]==[before[k] for k in ('memberSearches','groupSearches')],'Late files refetched people/groups')
        if stage.startswith('disabled'):
            rows=choices(proof.get('choices'),labels);require(len(rows)==5 and all(v['value']['type']=='user' for v in rows) and proof.get('noFileRequest') is True,'Off gate admitted file rows')
        if stage in ('keyboard','mouse'):
            fid=CANVAS if stage=='keyboard' else LIST;choice=proof.get('choice',{});values=choice.get('values',[]);index=choice.get('index')
            require(proof.get('file')==fid and proof.get('keyboard') is (stage=='keyboard') and choice.get('id')==fid and choice.get('type')==token(fid)['type'] and type(index) is int and 0<=index<len(values) and values[index]['value']==token(fid),'Wrong private file selection')
            require(choice.get('label')==(CANVAS_LABEL if fid==CANVAS else LIST_LABEL),'Wrong canonical selected label')
        if stage=='saved':require(proof.get('exactReadback') is True and {r['args']['file'] for r in requests if r['method']=='files.info'}=={CANVAS,LIST},'Missing actual canonical reopen reads')
        if stage in ('query-retired','cancelled'):
            release=proof.get('release',{});require(release.get('delivered') is False and release.get('cancelledBeforeReply') is True and release.get('query')==('pending-file' if stage=='query-retired' else 'cancel-file') and row['source']['cancelled']>=(1 if stage=='query-retired' else 2),'Retired file read retained authority')
        if stage in ('malformed','failure'):require(proof.get('noWrite') is True and proof.get('retainedBlocks')==saved and FAILURE in [n.get('text') for n in g['notices']],'Failure lost feedback/draft')
        path=Path(paths[stage]);require(path.is_absolute() and path==captures.resolve()/f'{fixture}-{stage}.png','Foreign capture path')
        if png:require(path.is_file() and not path.is_symlink(),'Missing original PNG');B['validate_png'](path,g['pngDimensions'])
    G['pages'](stages,[1,2,3] if disabled else [2,3,4,5],5 if disabled else 7)
    require(receipt.get('model')==stages[-1]['model'],'Terminal model drift')
    final=source(receipt.get('source'));require(final[:len(previous)]==previous,'Terminal ledger drift')
    if not disabled:require({'alex','list','pending-file','broken-file','offline-file','cancel-file'}<={r['args']['query'] for r in final if r['method']=='search.autocomplete.files'},'Missing actual file outcomes')

_NativeFixture=M['_NativeFixture'];short_case=False
class FileFixture(_NativeFixture):
    def __init__(self,fixture,log,binary=None,checks=None,**kwargs):
        checks={**(checks or {}),FLAG:'1','SUPER_PLATINUM_PROFILE_USER_MENTIONS_CHECK':'1','SUPER_PLATINUM_PROFILE_MEMBER_COMPLETION_CHECK':'1'}
        if short_case:checks[OFF]='1'
        super().__init__(fixture,log,binary,checks,**kwargs)

B.update(PREFIX='profile-file-completion-check: ',FAILURE=FAILURE,SCOPE='mixed-file-completion',validate=validate,NativeFixture=FileFixture,STAGES=STAGES)
if __name__=='__main__':
    if '--disabled' in sys.argv:sys.argv.remove('--disabled');short_case=True;B['STAGES']=SHORT
    B['main']()
