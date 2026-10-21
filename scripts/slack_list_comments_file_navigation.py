"""Original received-file click, canonical native target and durable draft custody."""
import base64
import copy
import json
import math
from pathlib import Path
import runpy

F=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_file_completion.py')))
S=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_storage_recovery.py')))
G,R,C,H,READ=(F[k] for k in ('G','R','C','H','READ'))
require,exact,rectangle=F['require'],F['exact'],F['rectangle']
STAGES={'file-navigation':('draft','canvas-open','canvas-return','list-open','list-return'),
        'file-navigation-custody':('composition-refused','composition-recovered','save-refused','save-recovered','retry-open','retry-return','source-refused')}
CANVAS,LIST=F['CANVAS'],F['LIST'];TS='1781524801.000001'
CANVAS_EDIT='\nCanvas click \u4e2d\U0001f642';LIST_EDIT='\nList click \u65e5\U0001f98a';RETIRE_EDIT='\nRetained after source retirement C'
CONTENT={CANVAS:'Native target Canvas content',LIST:'Native target List row'}
COMPOSING='Finish composing or close the link editor before opening this file. Your draft is retained.'
SAVE='Your draft is retained here but could not be saved locally. Retry local save before leaving.'

def received():
 return dict(ts=TS,thread_ts=READ['ROOT_TS'],user=G['FIRST'],text='Open the existing Canvas and List.',navigation_opaque={'keep':'original received comment'},blocks=[dict(type='rich_text',elements=[dict(type='rich_text_section',elements=[
  dict(type='canvas',file_id=CANVAS,text='Original Canvas fallback',url='https://fixture.invalid/files/'+CANVAS,style={'bold':True},navigation_opaque={'occurrence':1}),
  dict(type='text',text=' and '),dict(type='file',file_id=LIST,text='Original List fallback',url='https://fixture.invalid/files/'+LIST,style={'italic':True},navigation_opaque={'occurrence':2})])])])

def text_rect(value):
 require(isinstance(value,dict) and set(value)=={'left','top','right','bottom','width','height'} and all(type(v) in (int,float) and math.isfinite(v) for v in value.values()),'Invalid text rectangle')
 require(value['width']>0 and value['height']>0 and math.isclose(value['right']-value['left'],value['width'],rel_tol=0,abs_tol=1e-6) and math.isclose(value['bottom']-value['top'],value['height'],rel_tol=0,abs_tol=1e-6),'Inconsistent text rectangle')

def text_proof(value,viewport,all_lines=False):
 require(isinstance(value,dict) and value.get('ok') is True,'Missing actual visible text proof')
 rectangle(value.get('clip',{}),viewport);lines=value.get('lines');require(isinstance(lines,list) and lines,'Missing painted text lines')
 count=0
 for line in lines:
  text_rect(line.get('rect',{}));fragments=line.get('fragments');require(isinstance(fragments,list) and fragments,'Missing painted text fragments')
  for fragment in fragments:text_rect(fragment.get('rect',{}))
  clip=value['clip'];r=line['rect'];inside=r['left']>=clip['left']-1 and r['right']<=clip['right']+1 and r['top']>=clip['top']-1 and r['bottom']<=clip['bottom']+1
  if line.get('fullVisible') is True and line.get('hit') is True:
   rectangle(r,viewport)
   for fragment in fragments:rectangle(fragment['rect'],viewport)
   require(inside and all(f.get('hit') is True for f in fragments),'False painted text visibility');count+=1
 require(value.get('visibleLines')==count and count>0 and (not all_lines or count==len(lines)),'Painted title/body is clipped or occluded')

def measured(value,text,viewport):
 require(isinstance(value,dict) and value.get('text')==text and value.get('visible') is True and value.get('hit') is True,'Wrong/unpainted canonical text')
 rectangle(value.get('rect',{}),viewport);text_proof(value.get('textProof'),viewport,True)

def click(value,id,viewport):
 require(value.get('ok') is True and value.get('id')==id and value.get('kind')==('canvas' if id==CANVAS else 'file') and value.get('ts')==TS,'Click lost original token identity')
 require(value.get('control')==dict(tag='BUTTON',type='button',href=None,disabled=False) and type(value.get('defaultPrevented')) is bool,'Click is not the native button')
 measured(value.get('before'),F['LABELS'][id],viewport)
 require(isinstance(value.get('draft'),dict) and isinstance(value['draft'].get('blocks'),list),'Missing immediate native click draft')
 if value.get('input') is not None:require(value['input'].get('ok') is True and value['input'].get('snapshot')==value['draft'],'Native edit was not captured in same click turn')

def canvas_binaries():
 # Exact closed original loader fixture protobuf fields, independently encoded.
 def varint(n):
  out=[]
  while n>127:out.append((n&127)|128);n>>=7
  return bytes(out+[n])
 def text(tag,value):
  b=value.encode() if isinstance(value,str) else value
  return varint((tag<<3)|2)+varint(len(b))+b
 def number(tag,n):return varint(tag<<3)+varint(n)
 thread='navigation-thread';document='navigation-document'
 init=text(1,'navigation-quip-user')+text(33,G['SELF'])+text(34,'T1')
 request=text(1,thread)+text(3,number(1,1)+text(2,thread))
 payload=text(3,text(1,thread)+number(2,101)+text(10,document))+text(6,text(1,document)+number(2,101)+text(6,thread))
 response=text(2,number(1,1)+text(2,payload)+text(3,text(1,'Section/'+document)))
 return tuple(base64.b64encode(x).decode() for x in (init,request,response))

def target_reads(reads,ids):
 require(isinstance(reads,list) and len(reads)==5*len(ids),'Unexpected target acquisition')
 for offset,id in enumerate(ids):
  rows=reads[offset*5:offset*5+5]
  for index,r in enumerate(rows,offset*5+1):require(set(r)=={'serial','target','method','path','args','response'} and r['serial']==index and r['target']==id,'Unowned target read')
  raw=next(r for r in F['RAW'] if r['id']==id)
  if id==CANVAS:
   paths=['/api/files.info','/api/quip.getControllerInitData','/canvas/-/load-data','/api/files.info','https://files.slack.com/files-tmb/FLISTFILE1/navigation.html']
   require([r['path'] for r in rows]==paths and [r['method'] for r in rows]==['POST']*4+['GET'],'Wrong Canvas read path')
   require(rows[0]['args']==rows[3]['args']=={'file':id} and rows[1]['args']=={'_x_reason':'canvas-controller-init'} and set(rows[2]['args'])=={'request_binary'} and rows[4]['args']=={},'Wrong Canvas request body')
   expected=dict(raw,quip_thread_id='navigation-thread',editable=False,url_private=paths[-1]);require(rows[0]['response']==rows[3]['response']==dict(ok=True,file=expected),'Canvas canonical file response changed')
   require(rows[1]['response'].get('ok') is True and rows[1]['response'].get('quip_user_id')=='navigation-quip-user','Canvas init identity changed')
   require((rows[1]['response'].get('init_options'),rows[2]['args']['request_binary'],rows[2]['response'].get('base64'))==canvas_binaries(),'Original Canvas binary request/response changed')
   require(rows[4]['response']=={'html':"<div class='quip-canvas-content'><h1>Alex Canvas</h1><p>Native target Canvas content</p><p>The original List draft remains saved.</p></div>"},'Wrong target Canvas document')
  else:
   require([r['method'] for r in rows]==['POST']*5 and [r['path'] for r in rows]==['/api/files.info','/api/lists.records.list','/api/team.prefs.get','/api/users.prefs.get','/api/experiments.getByUser'],'Wrong List target path')
   require(rows[0]['args']==dict(file=id) and rows[1]['args']==dict(list_id=id,include_subtasks='true',archived='false',include_suggested='false',_x_reason='get-list-records') and rows[2]['args']==dict(target_team='T1',_x_reason='native-list-permission') and rows[3]['args']==dict(prefs='task_lists,assigned_to_list',_x_reason='native-list-completion-permission'),'Wrong List target request')
   expected=dict(raw,access='read',is_locked=True,list_metadata=dict(schema=[dict(id='C_NAV_NAME',key='NAME',name='Item',type='text',is_primary_column=True)],views=[]))
   require(rows[0]['response']==dict(ok=True,file=expected),'List canonical response changed')
   require(rows[1]['response']==dict(ok=True,list_id=id,records=[dict(id='R_NAV_TARGET',list_id=id,position=0,fields=[dict(key='NAME',text=CONTENT[id])],navigation_opaque={'keep':'separate target row'})]),'Wrong target List records')
   require(rows[2]['response']==dict(ok=True,prefs={'allow_lists':'full'}) and rows[3]['response']==dict(ok=True,prefs={'task_lists':'[]','assigned_to_list':''}) and rows[4]['response']==dict(ok=True,assignments={}),'List target policy changed')
   require(rows[4]['args']==F['observations'](False)['experiments.getByUser'][0],'List target experiment request changed')

def validate(fixture,receipt,directory,*,expected_case=None,dimensions=None,png=True):
 case=expected_case or receipt.get('case');require(case in STAGES and receipt.get('case')==case and receipt.get('ok') is True and receipt.get('fixture')==fixture and fixture in C['FIXTURES'] and receipt.get('noLiveNetwork') is True,'Wrong navigation receipt')
 ready=receipt.get('readiness');H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
 rows=receipt.get('stages');require(isinstance(rows,list) and [r.get('stage') for r in rows]==list(STAGES[case]),'Missing navigation stages')
 touched,unrelated=READ['record_proof'](receipt.get('initialRecords'));require(len(receipt['initialRecords'])==73 and unrelated['count']==71,'Wrong original inventory')
 seed=rows[0]['proof']['seed'];scope=dict(workspace='T1',user=G['SELF'],list=READ['LIST'],record=READ['KNOWN'],root=READ['ROOT_TS'],created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))
 require(seed.get('scope')==scope and seed.get('blocks')==G['seed_blocks']() and seed.get('reopened') is True,'Wrong original journal seed')
 initial=rows[0]['model'];previous=None;success=case=='file-navigation';prior_clicks=[]
 target_counts=[0,1,1,2,2] if success else [0,0,0,0,1,1,1]
 returns=[0,0,1,1,2] if success else [0,0,0,0,0,1,1]
 clicks_count=[0,1,1,2,2] if success else [1,1,2,2,3,3,4]
 for i,row in enumerate(rows):
  m,p,g=row['model'],row['proof'],row['geometry'];name=row['stage'];nav=m.get('navigation',{})
  require(g.get('ok') is True and g==row.get('geometryAfter') and g.get('noOverflow',True) is True,'Geometry changed during capture')
  viewport=g.get('viewport');require(isinstance(viewport,list) and len(viewport)==2 and all(type(v) is int and v>0 for v in viewport),'Invalid viewport')
  require(m.get('account')==initial.get('account') and R['uuid'](m.get('account')) and m.get('epoch')==initial.get('epoch') and m.get('noSession') is False and m.get('noTransport') is False and m.get('writes')==m.get('networkCalls')==0,'Original disconnected owner changed')
  require(m['initialDocument']==initial['initialDocument'] and m['initialRecords']==touched and m['initialUnrelated']==unrelated and p.get('seed')==seed,'Original source or seed changed')
  require(nav.get('enabled') is True and nav.get('received')==received() and nav.get('originalDocument')==initial['initialDocument'] and nav.get('originalRecords')==touched and nav.get('originalUnrelated')==unrelated and nav.get('networkCalls')==0 and nav.get('httpCapability') is False and nav.get('returns')==returns[i],'Navigation original source/custody changed')
  require(p.get('received')==rows[0]['proof']['received'] and p['received']==dict(received=received(),originalDocument=initial['initialDocument'],originalRecords=touched,originalUnrelated=unrelated,canonicalResponse=F['RESPONSE']),'Original received raw fixture changed')
  targets=[CANVAS,LIST][:target_counts[i]];target_reads(nav.get('targetRequests'),targets)
  if previous:require(nav['targetRequests'][:len(previous['navigation']['targetRequests'])]==previous['navigation']['targetRequests'],'Target receipt rewritten')
  cl=m['composeLedger'];require(all(cl.get(k)==[] for k in ('prepared','posts','preflights','policies','messages')) and cl.get('published') is False and cl.get('held') is False and cl.get('heldPost') is False,'Navigation posted a message')
  ledger=m['ledger'];C['validate_reads'](ledger['requests']);count=4*(returns[i]+1)
  require(len(ledger['requests'])==ledger['completed']==count and ledger['cancelled']==0 and ledger['held'] is False,'Extra original comment reads')
  if previous:require(ledger['requests'][:len(previous['ledger']['requests'])]==previous['ledger']['requests'],'Original read history changed')
  files=F['source'](m['completionSource'],m,False);require(len(files)==1 and files[0][0]['args']['query']=='alex' and files[0][1]['disposition']=='response' and files[0][1]['raw']==F['RESPONSE'],'Initial source was not actually acquired')
  require(m['completionSource']['completed']==1 and m['completionSource']['cancelled']==m['completionSource']['held']==0,'Unexpected file request lifecycle')
  F['file_projection'](m['completionSource']['files']['source'],True,not success and i==6)
  if previous:require(m['completionSource']['requests']==previous['completionSource']['requests'] and m['completionSource']['responses']==previous['completionSource']['responses'],'Navigation fetched replacement completion authority')
  clicks=p.get('clicks');require(isinstance(clicks,list) and len(clicks)==clicks_count[i] and clicks[:len(prior_clicks)]==prior_clicks,'Click history changed')
  for j,c in enumerate(clicks):click(c,LIST if success and j==1 else CANVAS,viewport)
  prior_clicks=copy.deepcopy(clicks)
  blocks=p['expectedBlocks'];require(isinstance(blocks,list) and F['rich'](blocks),'Missing rich draft')
  opaque=lambda b:[v for v in R['nodes'](b) if v.get('type') in ('user','link','channel')]
  require(opaque(blocks)==opaque(seed['blocks']),'Opaque/style draft custody lost')
  require('Final review \u4e2d\U0001f642' in F['display'](blocks) and '@alex' in F['display'](blocks),'Typed/query draft changed')
  if clicks:require(blocks==clicks[-1]['draft']['blocks'],'Draft differs from original click buffer')
  for j,c in enumerate(clicks):
   appended=(CANVAS_EDIT if j==0 else LIST_EDIT) if success else CANVAS_EDIT if j==1 else RETIRE_EDIT if j==3 else None
   require((c.get('input') is not None)==(appended is not None),'Immediate edit/click ordering changed')
   if appended:require(F['display'](c['draft']['blocks']).endswith(appended.lstrip('\n')),'Same-turn native edit lost')
  if i and p['expectedBlocks']!=rows[i-1]['proof']['expectedBlocks']:
   prior=R['text_units'](rows[i-1]['proof']['expectedBlocks']);current=R['text_units'](blocks);require(current[:len(prior)]==prior,'Original native text/style prefix changed')
  d=m['draftStore']['drafts'];require(len(d)==1,'Original draft entry missing');d=d[0]
  require(d.get('busy') is False and d.get('saving') is False and d.get('loading') is False and d.get('terminalPending') is False and d.get('attempt') is None and d.get('blocks')==blocks,'Native draft not settled/retained')
  if name=='save-refused':require(p.get('journal') is None and d.get('error')==SAVE,'SQLite failure was not retained')
  else:
   S['inventory'](p['journal'],scope);S['paired'](d,p['journal']);require(p['journal']['blocks']==blocks and p['journal']['attempt'] is None,'Original disk draft mismatch')
  target_id=CANVAS if name in ('canvas-open','retry-open') else LIST if name=='list-open' else None
  if target_id:
   route=m.get('route',{});require(g.get('editorAbsent') is True and route.get('canvasActive') is (target_id==CANVAS) and route.get('listActive') is (target_id==LIST) and route.get('canvasId' if target_id==CANVAS else 'listId')==target_id,'Wrong native route or editor still mounted')
   require(g.get('kind')==('canvas' if target_id==CANVAS else 'file'),'Wrong native target kind');measured(g.get('title'),F['LABELS'][target_id],viewport);measured(g.get('content'),CONTENT[target_id],viewport)
  else:
   require(m.get('route',{}).get('listActive') is True and m['route'].get('listId')==READ['LIST'] and m['route'].get('canvasActive') is False,'Original route was not retained')
   require(g.get('comments') is True and g.get('editor',{}).get('editable') is True and g['editor']['blocks']==blocks and g.get('native',{}).get('blocks')==blocks,'Original editable source missing')
   text_proof(g.get('draftText'),viewport)
   if clicks:
    native_generation=g['native'].get('generation');clicked_generation=clicks[-1]['draft'].get('generation')
    require(native_generation is not None and clicked_generation is not None and (native_generation!=clicked_generation if 'return' in name else native_generation==clicked_generation),'Original/fresh native editor generation changed')
   require(m['document']==initial['initialDocument'] and m['records']==touched and m['comments']['record']==READ['KNOWN'] and m['comments']['root']==READ['ROOT_TS'],'Returned to another item')
   actual=[v for v in m['comments']['page']['messages'] if v.get('ts')==TS];require(actual==[received()],'Original received wire changed')
  if name=='composition-refused':require(d.get('error')==COMPOSING and g['native'].get('composing') is True,'Composition guard was not exercised')
  if name in ('composition-recovered','save-recovered'):require(d.get('error') is None,'Local retry did not recover')
  if 'return' in name:require(p.get('runtimeForgotten') is True,'No real journal reopen')
  if name in ('save-refused','save-recovered','retry-open','retry-return','source-refused'):
   pair=p['storage'];a=S['ack'](pair['hold'],ready,1,m['account'],scope)
   if name!='save-refused':require(S['ack'](pair['release'],ready,2,m['account'],scope)==a and pair['hold']['contentSha256']==pair['release']['contentSha256'],'Lock modified original journal')
   else:require(pair.get('release') is None and a['blocks']==rows[1]['proof']['expectedBlocks'],'Save fault not held over original disk state')
  if name=='source-refused':
   pair=p['retirementStorage'];a=S['ack'](pair['hold'],ready,3,m['account'],scope);b=S['ack'](pair['release'],ready,4,m['account'],scope)
   require(a==b and a['blocks']==rows[5]['proof']['expectedBlocks'],'Mid-save lock did not preserve predecessor')
   pending=p['retirementPending'];require(pending['draftStore']['drafts'][0]['saving'] is True and pending['draftStore']['drafts'][0]['blocks']==blocks,'Source was not retired during actual save')
   held=p['retirementNative'];require(held.get('ok') is True and held.get('wrapperInert') is True and held.get('connected') is True and held.get('native',{}).get('blocks')==blocks and held['native'].get('composing') is False,'Source retirement was not under original native lease')
   drift=p['sourceInvalidation'];require(drift['id']==CANVAS and drift['before']==rows[5]['model']['completionSource']['files']['source'] and drift['after']==m['completionSource']['files']['source'],'Wrong original source invalidation')
  path=Path(directory)/f'{fixture}-{name}.png';require(Path(row.get('capture',''))==path,'Foreign capture')
  if png:H['png_signature'](path,dimensions)
  previous=m
 require(receipt.get('final')==rows[-1]['model'],'Final navigation snapshot changed');return receipt

def validator(case):
 require(case in STAGES,'Unknown navigation case')
 return lambda fixture,receipt,directory,**kwargs:validate(fixture,receipt,directory,expected_case=case,**kwargs)
