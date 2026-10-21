"""Cold received-file admission, original singleton reads and durable draft custody."""
import copy
import json
from pathlib import Path
import runpy

N=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_file_navigation.py')))
F,G,R,C,H,READ,S=(N[k] for k in ('F','G','R','C','H','READ','S'))
require,exact=N['require'],N['exact']
CANVAS,LIST,TS=N['CANVAS'],N['LIST'],N['TS']
STAGES={'cold-files':('cold-held','cold-loaded','canvas-open','canvas-return','list-open','list-return'),
        'cold-files-recovery':('cold-failed','retry-held','source-retired','cold-recovered','visit-held','visit-retired')}
RENAMED='Renamed received Canvas';NEXT='Next visit Canvas'
LOADING='Loading file\u2026';OFFLINE='File unavailable while offline. Retry when connected.'
UNAVAILABLE='File unavailable. Retry to load it again.'
FALLBACK={CANVAS:'Original Canvas fallback',LIST:'Original List fallback'}
BASE={id:dict(ok=True,file=copy.deepcopy(next(r for r in F['RAW'] if r['id']==id)))for id in (CANVAS,LIST)}
TEXT_DIR=Path(__file__).resolve().parents[1]/'src/desktop/slack_lists/comments/fixture/cold_files'
ORIGINAL_TEXT={id:(TEXT_DIR/name).read_bytes().decode('utf-8')for id,name in ((CANVAS,'canvas-response.json'),(LIST,'list-response.json'))}

def renamed(title):
 value=copy.deepcopy(BASE[CANVAS]);value['file'].update(title=title,name=title,updated=102);return value

def envelope_text(value,text):
 require(isinstance(text,str) and json.loads(text)==value,'Original singleton bytes differ from parsed envelope')
 if isinstance(value,dict) and isinstance(value.get('file'),dict):
  id=value['file']['id'];original=json.loads(ORIGINAL_TEXT[id])
  require(original==BASE[id],'Pinned original singleton differs from received fixture source')
  require(list(json.loads(text)['file']['channel_map'])==list(original['file']['channel_map']),'Original singleton channel_map order changed')
  expected=ORIGINAL_TEXT[id]
  if value!=BASE[id]:
   for key in ('title','name','updated'):
    old=json.dumps(key)+':'+json.dumps(BASE[id]['file'][key],separators=(',',':'))
    replacement=json.dumps(key)+':'+json.dumps(value['file'][key],separators=(',',':'))
    require(expected.count(old)==1,'Pinned scalar replacement is ambiguous');expected=expected.replace(old,replacement,1)
  require(text==expected,'Original positive singleton bytes rewritten')

def expected_requests(success):
 ids=[CANVAS,LIST] if success else [CANVAS,LIST,CANVAS,LIST,CANVAS,CANVAS]
 modes=['held','held'] if success else ['offline','malformed','held','held','success','held']
 raws=[BASE[CANVAS],BASE[LIST]] if success else [None,{'ok':True,'file':False},BASE[CANVAS],BASE[LIST],renamed(RENAMED),renamed(NEXT)]
 return ids,modes,raws

def ledger(value,model,success,index):
 require(value.get('enabled') is True and value.get('named') is True and value.get('protectedFixture') is True and value.get('identityCurrent') is True,'Cold reader lacks original closed identity')
 require(value.get('account')==model['account'] and value.get('epoch')==model['epoch'] and value.get('team')=='T1' and value.get('user')==G['SELF'] and value.get('networkCalls')==0 and value.get('httpCapability') is False,'Cold reader changed account/network custody')
 counts=[1,2,2,2,2,2] if success else [2,3,4,5,6,6]
 done=[0,2,2,2,2,2] if success else [2,2,4,5,5,6]
 cancelled=[0]*6 if success else [0,0,1,1,1,2]
 ids,modes,raws=expected_requests(success);count=counts[index]
 requests=value.get('requests');responses=value.get('responses');require(isinstance(requests,list) and isinstance(responses,list) and len(requests)==len(responses)==count,'Unexpected singleton request count')
 for j,(q,r) in enumerate(zip(requests,responses)):
  require(q.get('serial')==j+1 and q.get('id')==ids[j] and q.get('method')=='files.info' and q.get('httpMethod')=='POST' and q.get('path')=='/api/files.info' and q.get('args')==dict(file=ids[j],page='1',count='500',truncate='true') and q.get('mode')==modes[j],'Wrong original singleton factory/sequence')
  require(q.get('plannedResponse')==raws[j] and r.get('serial')==j+1 and r.get('id')==ids[j] and r.get('disposition')==modes[j] and r.get('raw')==raws[j],'Original response was rewritten')
  require(q.get('responseText')==r.get('responseText'),'Response byte custody changed')
  if modes[j]=='offline':require(q.get('responseText')=='null','Offline request fabricated response bytes')
  else:envelope_text(raws[j],q.get('responseText'))
 require(value.get('completed')==done[index] and value.get('cancelled')==cancelled[index],'Cold completion/cancellation counts changed')
 settled=[dict(serial=j,cancelled=(not success and j in (3,6))) for j in range(1,done[index]+1)]
 require(value.get('settlements')==settled,'Original request settlement changed')
 held_serial=1 if success and index==0 else 3 if not success and index==1 else 6 if not success and index==4 else None
 require(value.get('held')==([] if held_serial is None else [dict(serial=held_serial,id=CANVAS,receiverClosed=False)]),'Held original read changed')
 releases=([dict(serial=j,id=id,delivered=True,cancelledBeforeReply=False)for j,id in enumerate((CANVAS,LIST),1)] if index>=1 else []) if success else ([dict(serial=3,id=CANVAS,delivered=False,cancelledBeforeReply=True),dict(serial=4,id=LIST,delivered=True,cancelledBeforeReply=False)] if index>=2 else [])
 if not success and index==5:releases.append(dict(serial=6,id=CANVAS,delivered=False,cancelledBeforeReply=True))
 require(value.get('releases')==releases,'Late original read was delivered or release missing')
 future=copy.deepcopy(BASE)
 if not success and index>=2:future[CANVAS]=renamed(NEXT if index>=4 else RENAMED)
 require(value.get('futureResponses')==future,'Future rename changed another original field')
 texts=value.get('futureResponseTexts');require(isinstance(texts,dict) and set(texts)==set(future),'Missing future byte receipts')
 for id,raw in future.items():envelope_text(raw,texts[id])
 return releases

def projection(value,wanted,unavailable=()):
 require(isinstance(value,dict),'Missing canonical singleton collection')
 if not wanted and not unavailable and value=={'unavailable':True}:return
 require(value.get('unavailableIds')==list(unavailable) and value.get('excludedIds')==[],'Cold source invented excluded/missing inventory')
 rows=value.get('rows');require(isinstance(rows,list) and [r.get('id')for r in rows]==[r['file']['id'] for r in wanted],'Wrong singleton source identities/order')
 for row,raw in zip(rows,wanted):
  require(row.get('source')==raw['file'] and row.get('receipt')==raw,'Singleton source/envelope custody lost')
  effective=row.get('effective',{});id=row['id']
  require(effective.get('id')==id and effective.get('title')==raw['file']['title'] and effective.get('mode')==raw['file']['mode'] and effective.get('permalink')==raw['file']['permalink'],'Canonical file projection changed')
  require(effective.get('channels')==({CANVAS:['CZMAP001','CAMAP001','CTAIL001'],LIST:['CYMAP002','CBMAP002']}[id]),'Original-byte channel map union changed')

def token_plan(success,index):
 if success:return [('loading',FALLBACK[id],False,LOADING)if index==0 else ('ready',F['LABELS'][id],False,None)for id in (CANVAS,LIST)]
 return [
  [('unavailable',FALLBACK[CANVAS],True,OFFLINE),('unavailable',FALLBACK[LIST],True,UNAVAILABLE)],
  [('loading',FALLBACK[id],False,LOADING)for id in (CANVAS,LIST)],
  [('unavailable',FALLBACK[CANVAS],True,UNAVAILABLE),('ready',F['LABELS'][LIST],False,None)],
  [('ready',RENAMED,False,None),('ready',F['LABELS'][LIST],False,None)],
  [('loading',FALLBACK[CANVAS],False,LOADING),('ready',F['LABELS'][LIST],False,None)],
  None][index]

def cold_geometry(proof,plan,viewport):
 expected={'ts':TS,'tokens':[dict(id=id,state=p[0],label=p[1],retry=p[2])for id,p in zip((CANVAS,LIST),plan)]}
 require(proof.get('coldExpected')==expected,'Expected cold DOM identity/status changed')
 g=proof.get('coldGeometry',{});require(g.get('ok') is True and g.get('ts')==TS and g.get('viewport')==viewport and g.get('noOverflow') is True,'Cold token geometry missing')
 rows=g.get('tokens');require(isinstance(rows,list) and len(rows)==2,'Missing cold token occurrences')
 for row,id,(state,label,retry,notice) in zip(rows,(CANVAS,LIST),plan):
  actual=row.get('actual',{});require(row.get('ok') is True and actual.get('id')==id and actual.get('kind')==('canvas'if id==CANVAS else 'file') and actual.get('state')==state and actual.get('label')==label and actual.get('disabled') is (state!='ready') and actual.get('retry') is retry,'Wrong original cold token state')
  require(actual.get('busy')==('true' if state=='loading' else 'false'),'Loading accessibility state changed')
  N['measured'](row.get('title'),label,viewport)
  if notice:N['measured'](row.get('status'),notice,viewport)
  else:require(row.get('status') is None,'Ready title retained stale error')
  if retry:
   N['measured'](row.get('retry'),'Retry',viewport)
   require(row.get('retryControl')==dict(tag='BUTTON',type='button',href=None,disabled=False),'Retry not enabled original native control')
  else:require(row.get('retry') is None and row.get('retryControl') is None,'Unexpected retry control')

def acquisition(value,plan):
 require(isinstance(value,dict) and type(value.get('generation'))is int and type(value.get('running'))is bool and isinstance(value.get('rows'),list),'Missing original acquisition controller observation')
 rows=value['rows'];require(len({r.get('id')for r in rows})==len(rows) and all(r.get('id')in (CANVAS,LIST)for r in rows),'Duplicated/foreign cold acquisition')
 if plan is None:
  require(value['running']is False and all(r.get('current')is False and r.get('retryable')is False and r.get('state')=='unavailable'for r in rows),'Retired visit retained active acquisition');return
 expected={id:p for id,p in zip((CANVAS,LIST),plan)}
 for row in rows:
  state,label,retry,notice=expected[row['id']]
  require(row.get('current')is True and row.get('state')==state and row.get('retryable')is retry and row.get('notice')==notice and row.get('retryAt')==0 and type(row.get('serial'))is int and row['serial']>0 and row.get('ordinal')==(0 if row['id']==CANVAS else 1),'DOM status not owned by original observed controller')
 for id,p in expected.items():
  if p[0]!='ready':require(any(row['id']==id for row in rows),'Unavailable/loading status lacks original acquisition')
 require(value['running']is any(p[0]=='loading'for p in plan),'Cold worker lifecycle differs from observed pending reads')

def validate(fixture,receipt,directory,*,expected_case=None,dimensions=None,png=True):
 case=expected_case or receipt.get('case');success=case=='cold-files'
 require(case in STAGES and receipt.get('case')==case and receipt.get('ok') is True and receipt.get('fixture')==fixture and fixture in C['FIXTURES'] and receipt.get('noLiveNetwork') is True,'Wrong cold receipt')
 ready=receipt.get('readiness');H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
 rows=receipt.get('stages');require(isinstance(rows,list)and [r.get('stage')for r in rows]==list(STAGES[case]),'Missing cold stages')
 touched,unrelated=READ['record_proof'](receipt.get('initialRecords'));require(len(receipt['initialRecords'])==73 and unrelated['count']==71,'Wrong original inventory')
 first=rows[0]['model'];seed=rows[0]['proof']['seed'];scope=dict(workspace='T1',user=G['SELF'],list=READ['LIST'],record=READ['KNOWN'],root=READ['ROOT_TS'],created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))
 require(seed.get('scope')==scope and seed.get('blocks')==G['seed_blocks']() and seed.get('reopened') is True,'Wrong original draft seed')
 navseed=dict(received=N['received'](),originalDocument=first['initialDocument'],originalRecords=touched,originalUnrelated=unrelated,canonicalResponse=F['RESPONSE'])
 coldseed=dict(navigation=navseed,initialResponses=BASE,initialResponseTexts=ORIGINAL_TEXT,canonicalSeeded=False,searchRequests=0)
 previous=None;previous_blocks=None
 for i,row in enumerate(rows):
  m,p,g=row['model'],row['proof'],row['geometry'];stage=row['stage'];opened=stage in ('canvas-open','list-open');retired=stage=='visit-retired'
  require(g.get('ok') is True and g==row.get('geometryAfter') and g.get('noOverflow') is True,'Geometry changed during cold capture')
  viewport=g.get('viewport');require(isinstance(viewport,list)and len(viewport)==2 and all(type(v)is int and v>0 for v in viewport),'Invalid viewport')
  require(m.get('account')==first.get('account') and R['uuid'](m.get('account')) and m.get('epoch')==first.get('epoch') and m.get('noSession') is False and m.get('noTransport') is False and m.get('networkCalls')==m.get('writes')==0,'Original disconnected owner changed')
  require(m['initialDocument']==first['initialDocument'] and m['initialRecords']==touched and m['initialUnrelated']==unrelated and p.get('seed')==seed and p.get('received')==navseed and p.get('coldSeed')==coldseed,'Original source or no-canonical-seed proof changed')
  nav=m['navigation'];returns=([0,0,0,1,1,2]if success else [0]*6)[i];targets=([[],[],[CANVAS],[CANVAS],[CANVAS,LIST],[CANVAS,LIST]]if success else [[]]*6)[i]
  require(nav.get('enabled') is True and nav.get('received')==N['received']() and nav.get('originalDocument')==first['initialDocument'] and nav.get('originalRecords')==touched and nav.get('originalUnrelated')==unrelated and nav.get('returns')==returns and nav.get('networkCalls')==0 and nav.get('httpCapability') is False,'Original received row/navigation authority changed')
  N['target_reads'](nav.get('targetRequests'),targets)
  for k in ('prepared','posts','preflights','policies','messages'):require(m['composeLedger'].get(k)==[],'Cold hydration sent a message')
  require(all(m['composeLedger'].get(k)is False for k in ('published','held','heldPost')),'Unexpected send lifecycle')
  reads=m['ledger'];C['validate_reads'](reads['requests']);require(len(reads['requests'])==reads['completed']==4*(returns+1) and reads['cancelled']==0 and reads['held'] is False,'Unexpected comments refresh')
  source=m['completionSource'];require(source.get('enabled')is True and source.get('identityCurrent')is True and source.get('account')==m['account'] and source.get('epoch')==m['epoch'] and source.get('team')=='T1' and source.get('user')==G['SELF'],'Original completion credential shell changed')
  require(all(source.get(k)==[]for k in ('requests','responses','releases'))and source.get('completed')==source.get('cancelled')==source.get('held')==source.get('networkCalls')==0 and source.get('httpCapability')is False,'Cold title used autocomplete or extra HTTP')
  require(not source.get('files',{}).get('enabled',False),'Autocomplete file gate unexpectedly enabled')
  releases=ledger(m['coldFiles'],m,success,i);require(p.get('releases')==releases,'Original cold release proof changed')
  wanted=([[],[BASE[CANVAS],BASE[LIST]]][i>0] if success else [[],[],[BASE[LIST]],[BASE[LIST],renamed(RENAMED)],[BASE[LIST]],[BASE[LIST]]][i])
  projection(m['coldFiles']['source'],wanted,[CANVAS]if not success and i>=4 else [])
  if previous:
   for name in ('requests','responses','releases','settlements','invalidations'):require(m['coldFiles'][name][:len(previous['coldFiles'][name])]==previous['coldFiles'][name],'Cold history rewritten')
   require(nav['targetRequests'][:len(previous['navigation']['targetRequests'])]==previous['navigation']['targetRequests'] and reads['requests'][:len(previous['ledger']['requests'])]==previous['ledger']['requests'],'Read/target receipt history rewritten')
   require(source['seed']==previous['completionSource']['seed'],'Cold source reseeded')
  blocks=p['expectedBlocks'];require(F['rich'](blocks)and 'Final review \u4e2d\U0001f642'in F['display'](blocks)and '@alex'not in F['display'](blocks),'Cold rich draft/query changed')
  opaque=lambda b:[v for v in R['nodes'](b)if v.get('type')in ('user','channel','link')]
  require(opaque(blocks)==opaque(seed['blocks']),'Opaque/style seed lost')
  if previous_blocks:
   old=R['text_units'](previous_blocks);require(R['text_units'](blocks)[:len(old)]==old,'Original styled text prefix changed')
  clicks=p.get('clicks');count=([0,0,1,1,2,2]if success else [0]*6)[i];require(isinstance(clicks,list)and len(clicks)==count,'Wrong cold native click count')
  for j,c in enumerate(clicks):
   N['click'](c,[CANVAS,LIST][j],viewport);require(c.get('input')is not None and F['display'](c['draft']['blocks']).endswith([N['CANVAS_EDIT'],N['LIST_EDIT']][j].lstrip('\n')),'Missing same-turn edit before actual cold click')
  if clicks:require(blocks==clicks[-1]['draft']['blocks'],'Click/draft custody changed')
  if not success:require(blocks==rows[0]['proof']['expectedBlocks'],'Recovery changed original draft')
  drafts=m['draftStore']['drafts'];require(len(drafts)==1,'Original draft missing');draft=drafts[0]
  require(all(draft.get(k)is False for k in ('busy','saving','loading','terminalPending'))and draft.get('attempt')is None and draft.get('error')is None and draft.get('blocks')==blocks,'Cold draft not retained/settled')
  S['inventory'](p['journal'],scope);S['paired'](draft,p['journal']);require(p['journal']['blocks']==blocks and p['journal']['attempt']is None,'Independent disk draft differs')
  route=m['route']
  if opened:
   id=CANVAS if stage=='canvas-open'else LIST;require(g.get('editorAbsent')is True and route.get('canvasActive')is(id==CANVAS)and route.get('listActive')is(id==LIST)and route.get('canvasId'if id==CANVAS else'listId')==id,'Wrong native cold target')
   N['measured'](g.get('title'),F['LABELS'][id],viewport);N['measured'](g.get('content'),N['CONTENT'][id],viewport)
   require(p.get('coldExpected')is None,'Target inherited original token DOM proof')
   acquisition(m.get('coldAcquisition'),None)
  else:
   require(route.get('listActive')is True and route.get('listId')==READ['LIST']and route.get('canvasActive')is False,'Cold action replaced original List')
   require(m['document']==first['initialDocument']and m['records']==touched,'Cold load changed canonical List')
   if retired:
    require(g.get('editorAbsent')is True and g.get('record')==READ['KNOWN']and p.get('coldExpected')is None,'Actual Back did not retire original comments/editor')
    acquisition(m.get('coldAcquisition'),None)
    require(m['coldAcquisition']['rows']==[],'Explicit Back did not retire original acquisition queue')
   else:
    require(g.get('comments')is True and g.get('editor',{}).get('editable')is True and g['editor']['blocks']==blocks and g.get('native',{}).get('blocks')==blocks and g['native'].get('composing')is False,'Original editable draft missing')
    N['text_proof'](g.get('draftText'),viewport)
    require(m['comments']['record']==READ['KNOWN']and m['comments']['root']==READ['ROOT_TS']and [r for r in m['comments']['page']['messages']if r.get('ts')==TS]==[N['received']()],'Cold read rewrote original received message')
    cold_geometry(p,token_plan(success,i),viewport)
    acquisition(m.get('coldAcquisition'),token_plan(success,i))
  if 'return'in stage:
   require(p.get('runtimeForgotten')is True and g['native']['generation']!=clicks[-1]['draft']['generation'],'No real original journal reopen')
  elif not opened and not retired and i:require(g['native']['generation']==rows[0]['geometry']['native']['generation']if not success else True,'Recovery replaced original native editor')
  path=Path(directory)/f'{fixture}-{stage}.png';require(Path(row.get('capture',''))==path,'Foreign cold screenshot')
  if png:H['png_signature'](path,dimensions)
  previous=m;previous_blocks=blocks
 if not success:recovery(rows)
 require(receipt.get('final')==rows[-1]['model'],'Final cold model changed');return receipt

def recovery(rows):
 for i,row in enumerate(rows):
  p=row['proof'];expected=([CANVAS,LIST]if i>=1 else [])+([CANVAS]if i>=3 else [])+([CANVAS]if i>=4 else [])
  retries=p.get('retries');require(isinstance(retries,list)and [r.get('id')for r in retries]==expected,'Missing explicit retry path')
  for r in retries:
   require(r.get('ok')is True and r.get('control')==dict(tag='BUTTON',type='button',href=None,disabled=False),'Retry lost original native control')
   N['measured'](r.get('before'),'Retry',row['geometry']['viewport'])
  invalidations=row['model']['coldFiles']['invalidations'];require(len(invalidations)==(2 if i>=4 else 1 if i>=2 else 0),'Wrong local invalidation history')
  for key,index,old_id,new_title in [('invalidation',0,3,RENAMED),('visitInvalidation',1,6,NEXT)]:
   if len(invalidations)<=index:continue
   drift=p.get(key);require(drift==invalidations[index]and drift.get('kind')=='accepted-local-file-invalidation'and drift.get('id')==CANVAS and drift.get('futureResponse')==renamed(new_title),'Wrong source retirement identity')
   if index==0:projection(drift['before'],[]);projection(drift['after'],[])
   else:projection(drift['before'],[BASE[LIST],renamed(RENAMED)]);projection(drift['after'],[BASE[LIST]],[CANVAS])
  for key,serial,admitted in [('cancelledAbsentRead',3,i>=2),('cancelledVisitRead',6,i==5)]:
   if not admitted:continue
   witness=p.get(key,{});require(any(h==dict(serial=serial,id=CANVAS,receiverClosed=True)for h in witness.get('held',[])),'Original receiver was not actually cancelled')
   require(any(s==dict(serial=serial,cancelled=True)for s in witness.get('settlements',[])),'Cancelled read task never settled')
   require(witness.get('requests')==row['model']['coldFiles']['requests'][:len(witness.get('requests',[]))]and len(witness.get('requests',[]))>=serial,'Cancellation witness is foreign')

def validator(case):
 require(case in STAGES,'Unknown cold case')
 return lambda fixture,receipt,directory,**kwargs:validate(fixture,receipt,directory,expected_case=case,**kwargs)
