"""Closed List whole-file completion; original bytes, native caret and durable rich draft."""
import copy
import json
import math
import runpy
from pathlib import Path
G=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_group_completion.py')))
R,X,C,H,READ=(G[k] for k in ('R','X','C','H','READ'))
require,rectangle,exact=G['require'],G['rectangle'],G['exact']
STAGES={
 'file-completion':('draft','local-held','refreshed','keyboard-inserted','mouse-inserted','reopened','sending','sent','readback'),
 'file-completion-custody':('draft','held','query-retired','stale-refused','malformed','failure','empty','cancelled-reopened','source-ready','source-refused'),
 'file-completion-off':('draft','off-results','member-inserted')}
CANVAS,LIST,HOSTED='FLISTFILE1','FLISTFILE2','FLISTFILE3'
LABELS={**G['LABELS'],CANVAS:'Alex Canvas',LIST:'Alex List'}
BASE_IDS=[G['SECOND'],G['FIRST'],G['RETIRED'],G['SELF'],G['G1'],G['G2'],G['G3']]
FAILURE='Mention suggestions could not load. Edit your query to try again.'
EMPTY='No matching mentions'
RESPONSE=(Path(__file__).resolve().parents[1]/'src/desktop/slack_lists/comments/fixture/completion/files-response.json').read_bytes().decode('utf-8')
RAW=json.loads(RESPONSE)['files']

def token(id):
 href='https://fixture.invalid/files/'+id
 return dict(type='canvas' if id==CANVAS else 'file',file_id=id,text=href,url=href)
def identity(c):
 v=c['value'];return v.get('file_id') if v.get('type') in ('canvas','file') else G['identity'](c)
def choice(id):return dict(value=token(id),label=LABELS[id]) if id in (CANVAS,LIST) else G['choice'](id)
def rich(blocks):
 require(isinstance(blocks,list),'Missing rich blocks');result=[]
 for block in blocks:
  require(block.get('type')=='rich_text' and isinstance(block.get('elements'),list),'Wrong rich block');sections=[]
  for section in block['elements']:
   require(section.get('type')=='rich_text_section' and isinstance(section.get('elements'),list),'Wrong rich section');values=[]
   for v in section['elements']:
    if v.get('type')=='text':
     require(isinstance(v.get('text'),str),'Invalid text');values.extend(('text',c,{k:x for k,x in v.items() if k!='text'}) for c in v['text'])
    else:
     require(v.get('type') in ('channel','user','usergroup','link','canvas','file'),'Unexpected inline');values.append(('opaque',v))
   sections.append(({k:v for k,v in section.items() if k!='elements'},values))
  result.append(({k:v for k,v in block.items() if k!='elements'},sections))
 return result
def display(blocks):
 return '\n'.join(''.join(LABELS[v['file_id']] if v['type'] in ('canvas','file') else LABELS[v['channel_id']] if v['type']=='channel' else LABELS[v['usergroup_id']] if v['type']=='usergroup' else LABELS.get(v['user_id'],'@'+v['user_id']) if v['type']=='user' else v.get('text','') for v in s['elements']) for b in blocks for s in b['elements'])
def inserted_blocks(a,ids):
 values=[]
 for id in ids:values.extend([dict(type='text',text=' '),choice(id)['value'],dict(type='text',text=' ')])
 return G['with_tail'](a,values)
# Reuse the independently loaded group oracle's exact owner/journal/read/send
# checks with only its rich-value algebra generalized to the admitted file tokens.
# This does not mutate another validator module or rewrite incoming evidence.
G['rich'].__globals__.update(rich=rich,display=display,inserted_blocks=inserted_blocks,PRIVATE=CANVAS,REMOTE=LIST,LABELS=LABELS)

def validator(case):
 require(case in STAGES,'Unknown file case')
 return lambda fixture,receipt,directory,**kw:validate(fixture,receipt,directory,expected_case=case,**kw)
def observations(off):
 obs=G['observations']();obs['experiments.getByUser'][1]['assignments']['texty_autocomplete_files']={'group':'off' if off else 'on'}
 return obs

def source(source,model,off):
 requests=source.get('requests');responses=source.get('responses')
 require(isinstance(requests,list) and isinstance(responses,list) and len(requests)==len(responses),'Missing source ledger')
 files=[];filtered=copy.deepcopy(source);filtered['requests']=[];filtered['responses']=[]
 obs=observations(off)
 for serial,(req,reply) in enumerate(zip(requests,responses),1):
  require(set(req)=={'serial','method','args'} and exact(req['serial'],serial) and set(reply)=={'serial','disposition','raw'} and exact(reply['serial'],serial),'Original source serial changed')
  if req['method']=='search.autocomplete.files':
   q=req['args'].get('query');require(not off and isinstance(q,str) and q and not q.startswith(('@','\uff20')) and exact(req['args'],dict(query=q,include_shares='true',_x_reason='texty-autocomplete')),'File request contract changed')
   require(reply['disposition'] in ('response','held','offline'),'File response disposition changed')
   require(reply['raw'] is None if reply['disposition']=='offline' else reply['raw'] in (RESPONSE,'{"ok":true,"files":false}','{"ok":true,"files":[]}'),'Original response bytes changed')
   files.append((req,reply))
  else:
   a,b=copy.deepcopy(req),copy.deepcopy(reply);a['serial']=b['serial']=len(filtered['requests'])+1
   filtered['requests'].append(a);filtered['responses'].append(b)
 filtered.update(completed=sum(r['method']=='usergroups/search' for r in filtered['requests']),cancelled=0,held=0,releases=[])
 env=G['closed_source'].__globals__;old=env['observations'];env['observations']=lambda:copy.deepcopy(obs)
 try:G['closed_source'](filtered,model);G['source_receipts'](filtered)
 finally:env['observations']=old
 require(source.get('files',{}).get('enabled') is True and source['files'].get('gate')==('off' if off else 'on') and source['files'].get('originalResponse')==RESPONSE,'Missing original gate/byte observation')
 for k in ('completed','cancelled','held'):require(R['integer'](source.get(k)),'Invalid file lifecycle counter')
 require(source['cancelled']<=source['completed']<=len(files),'Unowned file completion')
 held={r['serial'] for _,r in files if r['disposition']=='held'};releases=source.get('releases')
 require(isinstance(releases,list) and len({r.get('serial') for r in releases})==len(releases),'Duplicate release')
 for r in releases:require(set(r)=={'serial','delivered','cancelledBeforeReply'} and r['serial'] in held and type(r['delivered']) is bool and r['cancelledBeforeReply'] is (not r['delivered']),'Unowned held release')
 require(source['held']==len(held)-len(releases),'Held inventory changed')
 return files

def file_projection(proof,acquired,invalidated=False):
 if not acquired:
  require(proof=={'unavailable':True} or proof==dict(rows=[],unavailableIds=[],excludedIds=[]),'Cold source fabricated rows');return
 require(isinstance(proof,dict) and proof.get('excludedIds')==[HOSTED] and proof.get('unavailableIds')==([CANVAS] if invalidated else []),'Hosted exclusion or unknown source changed')
 rows=proof.get('rows',[]);wanted=[LIST] if invalidated else [CANVAS,LIST]
 require([r.get('id') for r in rows]==wanted,'File source row identity/order changed')
 for r in rows:
  raw=next(v for v in RAW if v['id']==r['id']);require(exact(r.get('source'),raw),'Original file source lost')
  effective=r.get('effective',{});channels={CANVAS:['CZMAP001','CAMAP001','CTAIL001'],LIST:['CYMAP002','CBMAP002']}[r['id']]
  require(effective.get('channels')==channels and effective.get('title')==LABELS[r['id']] and effective.get('permalink')==token(r['id'])['url'],'Effective title/permalink/original-byte channel union changed')

def sources(rows,case):
 success=case=='file-completion';off=case=='file-completion-off'
 counts=[0,1,1,1,1,1,1,1,1] if success else [0,1,2,2,3,4,5,6,7,7]
 done=[0,0,1,1,1,1,1,1,1] if success else [0,0,2,2,3,4,5,6,7,7]
 cancels=[0]*9 if success else [0,0,1,1,1,1,1,2,2,2]
 holds=[0,1,0,0,0,0,0,0,0] if success else [0,1,1,0,0,0,0,0,0,0]
 queries=['alex'] if success else ['alex','nomatch','malformed','offline','empty','cancelled','alex']
 dispositions=['held'] if success else ['held','response','response','offline','response','held','response']
 if off:counts=done=cancels=holds=[0]*3;queries=[];dispositions=[]
 previous=None
 for i,row in enumerate(rows):
  s=row['model']['completionSource'];files=source(s,row['model'],off)
  ordinary_count=([0,1,1,1,2,2,2,2,2] if success else [0,1,1] if off else [0,1,2,2,3,4,5,6,7,7])[i]
  ordinary_queries=(['alex','alex'] if success else ['alex'] if off else queries)[:ordinary_count]
  for method in ('users/search','usergroups/search'):require([r['args']['query'] for r in s['requests'] if r['method']==method]==ordinary_queries,'Mixed caller query sequence changed')
  require([r['args']['query'] for r,_ in files]==queries[:counts[i]] and exact(s['completed'],done[i]) and exact(s['cancelled'],cancels[i]) and exact(s['held'],holds[i]),'Original file lifecycle changed')
  for j,(_,reply) in enumerate(files):
   raw=None if dispositions[j]=='offline' else '{"ok":true,"files":false}' if not success and j==2 else '{"ok":true,"files":[]}' if not success and j==4 else RESPONSE
   require(reply['disposition']==dispositions[j] and reply['raw']==raw,'File outcome changed')
  expected=[] if off else [dict(serial=files[j][0]['serial'],delivered=success,cancelledBeforeReply=not success) for j in ([0] if success and i>=2 else [0,5] if not success and i>=7 else [0] if not success and i>=3 else [])]
  require(s['releases']==expected,'Original releases changed')
  require(row['proof']['releases']==[dict(stage='refreshed' if success else 'stale-refused' if j==0 else 'cancelled-reopened',value=r) for j,r in enumerate(expected)],'Release proof changed')
  acquired=not off and i>=2
  file_projection(s['files']['source'],acquired,case=='file-completion-custody' and i==9)
  require(exact(s.get('groups'),G['group_state'](G['group_rows']()[:1] if i==0 else G['group_rows']())),'Group source lost during files')
  if previous:
   for k in ('requests','responses','releases'):require(s[k][:len(previous[k])]==previous[k],'Source history rewritten')
   require(s['seed']==previous['seed'],'Source reseeded')
  previous=s

def windows(receipt,rows,directory,dimensions,png):
 for row in rows:
  c=row['geometry']['completion'];stage=row['stage']
  if not c['rows']:continue
  positions=['top']+(['middle','bottom'] if stage in ('refreshed','source-ready') else ['bottom'])
  samples=[row['geometry']]
  for pos in positions[1:]:
   w=row['proof'].get(pos+'Window',{});require(w.get('model')==row['model'],'Scroll crossed original publication')
   path=Path(directory)/f'{receipt["fixture"]}-{stage}-{pos}.png';require(Path(w.get('capture',''))==path,'Foreign scroll capture')
   if png:H['png_signature'](path,dimensions)
   samples.append(w.get('geometry',{}))
  seen=set()
  for pos,g in zip(positions,samples):
   require(g.get('ok') is True and g.get('editor')==row['geometry']['editor'] and g.get('native')==row['geometry']['native'],'Scroll changed editor')
   p=g['completion'];b=p['popup']['rect'];rectangle(b,g['viewport']);scroll=p['scroll']
   require(p['sequence']==c['sequence'] and p['revision']==c['revision'] and [r['label'] for r in p['rows']]==[r['label'] for r in c['rows']],'Scroll changed result identity')
   require(all(type(scroll.get(k)) in (int,float) and math.isfinite(scroll[k]) for k in ('top','height','total')) and scroll['height']>0,'Invalid scroll')
   distance=max(0,scroll['total']-scroll['height']);expected=0 if pos=='top' else distance/2 if pos=='middle' else distance
   require(abs(scroll['top']-expected)<=1,'Required scroll position missing')
   for j,o in enumerate(p['rows']):
    r=o['rect'];full=r['left']>=b['left']-1 and r['right']<=b['right']+1 and r['top']>=b['top']-1 and r['bottom']<=b['bottom']+1
    require(o.get('fullyVisible') is full,'Fabricated row visibility')
    if full:rectangle(r,g['viewport']);require(o.get('visible') is True and o.get('hit') is True,'Occluded suggestion');seen.add(j)
  require(seen==set(range(len(c['rows']))),'Suggestion never visibly reachable')

def completion(rows,case,a):
 success=case=='file-completion';off=case=='file-completion-off';previous=[];settled={}
 plan=[('local-held','query','@alex'),('keyboard-inserted','choose',CANVAS),('mouse-inserted','query','@alex'),('mouse-inserted','choose',LIST)] if success else [('off-results','query','@alex'),('member-inserted','choose',G['SECOND'])] if off else [('held','query','@alex'),('query-retired','query','@nomatch'),('malformed','query','@malformed'),('failure','query','@offline'),('empty','query','@empty'),('cancelled-reopened','query','@cancelled'),('source-ready','query','@alex')]
 input_counts=[0,1,1,2,4,4,4,4,4] if success else [0,1,2] if off else [0,1,2,2,3,4,5,6,7,7]
 for i,row in enumerate(rows):
  m,p,g=row['model'],row['proof'],row['geometry'];q=m.get('completion');cg=g['completion'];stage=row['stage']
  open_=stage in ('local-held','refreshed','held','query-retired','stale-refused','malformed','failure','empty','source-ready','off-results')
  ids=BASE_IDS+([CANVAS,LIST] if stage in ('refreshed','source-ready') else []) if stage in ('local-held','refreshed','held','source-ready','off-results') else []
  require(cg.get('open') is open_ and [v['label'] for v in cg['rows']]==(q['labels'] if open_ else []),'Popup ownership changed')
  status=FAILURE if stage in ('malformed','failure') else EMPTY if open_ and not ids else None
  require((cg.get('status') or {}).get('text')==status,'Wrong feedback')
  if open_:
   require(q.get('current') is True and q.get('started') is True and q.get('consumed') is False and q.get('friendly') is False and q.get('batchCurrent') is (status!=FAILURE),'Lost original query/source')
   require(q['sequence']==cg['sequence'] and (status==FAILURE or q['revision']==cg['revision']),'Wrong publication')
   values=q.get('values',[]);require(sorted(identity(v) for v in values)==sorted(ids) and all(v==choice(identity(v)) for v in values),'Wrong mixed original IDs/labels')
   if stage in ('refreshed','source-ready','off-results'):require(q.get('settledBatch') is True,'Interim batch mistaken for final')
   if stage in ('local-held','held'):require(q.get('settledBatch') is False,'Held file read was already settled')
   require([v['label'] for v in values]==[v['label'] for v in cg['rows']],'DOM differs from original order')
   if ids:require([identity(v) for v in values].index(G['SECOND'])<[identity(v) for v in values].index(G['FIRST']),'List context order lost')
   rectangle(cg['popup']['rect'],g['viewport']);require(cg['popup']['visible'] is True,'Popup hidden')
   for j,v in enumerate(cg['rows']):require(v['id']==f"slack-list-channel-choice-{g['editor']['generation']}-{cg['sequence']}-{j}" and v['role']=='option' and v['type']=='button' and v['selected']==str(j==0).lower(),'Row semantics changed')
   if status:rectangle(cg['status']['rect'],g['viewport']);require(cg['status']['visible'] is True and cg['status']['hit'] is True,'Feedback obscured')
  else:require(cg.get('popup') is None and (q is None or q.get('current') is False),'Retired popup selectable')
  inputs=p['inputs'];require(len(inputs)==input_counts[i] and inputs[:len(previous)]==previous,'Native input history rewritten')
  for j,item in enumerate(inputs[len(previous):],len(previous)):
   expected_stage,kind,value=plan[j];v=item['value']
   require(item.get('stage')==expected_stage and item.get('kind')==kind and v.get('ok') is True,'Native input path changed')
   original=inserted_blocks(a,[CANVAS]) if success and j>=2 else rows[7]['proof']['expectedBlocks'] if not success and not off and j==6 else a
   expected=G['query_blocks'](original,value if kind=='query' else '@alex')
   snap=v.get('native' if kind=='query' else 'before',{})
   require(rich(snap.get('blocks'))==rich(expected) and snap.get('composing') is False and R['integer'](snap.get('revision')) and isinstance(snap.get('generation'),str) and snap['generation'].isdigit(),'Input did not retain original native rich buffer')
   if item['kind']=='query':require(v.get('query')==value,'Wrong actual query')
   else:
    selected=item.get('selected');pub=item.get('publication',{});choices=item.get('values',[]);index=v.get('index')
    require(selected==value and sorted(identity(c) for c in choices)==sorted(BASE_IDS+([] if off else [CANVAS,LIST])) and all(c==choice(identity(c)) for c in choices),'Selection batch changed identity or category')
    require(item['kind']=='choose' and selected in (CANVAS,LIST,G['SECOND']) and R['integer'](index) and index<len(choices) and choices[index]==choice(selected),'Choice index lost original value')
    require(pub.get('current') is True and pub.get('batchCurrent') is True and pub.get('settledBatch') is True and pub['values']==choices and pub['labels']==[c['label'] for c in choices] and pub['sequence']==v['sequence'] and pub['revision']==v['revision'],'Choice borrowed interim/foreign batch')
    require(v['label']==LABELS[selected] and v['keyboard'] is (selected!=LIST),'Wrong native selection path')
    if selected!=LIST:
     original_row=rows[1 if off else 2];require(pub==original_row['model']['completion'] and choices==original_row['model']['completion']['values'],'Keyboard choice changed original publication')
  previous=inputs
  needed=stage in ('query-retired','stale-refused','malformed','failure','empty','source-ready','off-results')
  new=p.get('settled',{});require(set(new)==set(settled)|({stage} if needed else set()) and all(new[k]==v for k,v in settled.items()),'Settlement proof changed')
  if needed:
   v=new[stage];n=v.get('native',{});require(v.get('completion')==q and n==dict(ok=True,ready=True,sequence=q['sequence'],revision=q['revision'],labels=q['labels'],status=status or '') and q.get('settledBatch') is (status!=FAILURE),'Final DOM/source settlement missing')
  settled=new
  files=[v for v in R['nodes'](p['expectedBlocks']) if v.get('type') in ('canvas','file')]
  require([t for t in g.get('native',{}).get('tokens',[]) if t['value'].get('type') in ('canvas','file')]==[dict(value=v,label=LABELS[v['file_id']]) for v in files],'Canonical title lost on native insertion/reopen')
 if success:require(rows[1]['model']['completion']['sequence']==rows[2]['model']['completion']['sequence'],'Refreshed batch replaced query')
 if case=='file-completion-custody':
  require(rows[2]['geometry']['completion']==rows[3]['geometry']['completion'],'Late file response changed popup')
  for row in rows[3:]:
   stale=row['proof'].get('staleChoice',{});require(stale.get('accepted') is False and stale.get('before')==stale.get('after') and rich(stale['before']['blocks'])==rich(G['query_blocks'](a,'@nomatch')),'Old query changed native draft')

def drift(rows,scope):
 prior,ready,row=rows[7:10];proof=row['proof'].get('sourceChange',{});before=proof.get('before');retired=proof.get('retired')
 require(before==ready['model'] and retired==proof.get('after')==row['model'] and proof.get('id')==CANVAS,'Source retirement evidence changed')
 require(row['model']['ledger']==prior['model']['ledger'] and row['model']['composeLedger']==prior['model']['composeLedger'],'Source invalidation sent/read comment')
 old=proof['original'];stale=proof['staleChoice'];index=proof['index'];q=before['completion']
 require(q['values'][index]==choice(CANVAS) and old['completion']['sequence']==q['sequence'] and old['completion']['revision']==q['revision'],'Old choice identity lost')
 require(stale.get('ok') is True and stale.get('accepted') is False and stale.get('before')==stale.get('after')==old['native']['snapshot'],'Invalidated file insertion mutated draft')
 expected=G['query_blocks'](prior['proof']['expectedBlocks'],'@alex')
 for v in (ready,row):
  m,p,g=v['model'],v['proof'],v['geometry'];d=m['draftStore']['drafts'][0];j=p['journal']
  require(rich(p['expectedBlocks'])==rich(expected) and d['blocks']==j['blocks']==g['editor']['blocks']==g['native']['snapshot']['blocks']==p['expectedBlocks'],'Source change lost rich draft')
  require(j['scope']==scope and j['capture']==d['capture'] and j['revision']==d['savedRevision'] and j['attempt'] is None and d['id']==prior['model']['draftStore']['drafts'][0]['id'] and d['generation']==prior['model']['draftStore']['drafts'][0]['generation'],'Source change lost retained journal')
  require(g['ok'] is True and g['noOverflow'] is True and g['editor']['editable'] is True and g['editor']['text']==display(expected),'Source change damaged editor')
  if v is row:X['editor_text'](g)
 operation=proof.get('operation',{})
 require(set(operation)=={'id','before','after','eventRevisionBefore','eventRevisionAfter'} and operation['id']==CANVAS and operation['before']==before['completionSource']['files']['source'] and operation['after']==retired['completionSource']['files']['source'] and R['integer'](operation['eventRevisionBefore']) and operation['eventRevisionAfter']==operation['eventRevisionBefore'],'Actual local source invalidation proof changed')
 require(ready['proof']['journal']==row['proof']['journal'],'Invalidation changed durable draft')
 require({k:v for k,v in before.items() if k not in ('completion','completionSource')}=={k:v for k,v in retired.items() if k not in ('completion','completionSource')},'Source invalidation changed unrelated serial state')
 for k in ('requests','responses','releases','seed'):require(before['completionSource'][k]==retired['completionSource'][k],'Source invalidation fabricated acquisition')

def derived_geometry_equal(actual,expected):
 return type(actual) in (int,float) and math.isfinite(actual) and math.isclose(actual,expected,rel_tol=0,abs_tol=1e-6)

def painted_files(rows,case):
 if case!='file-completion':return
 for row in rows[7:9]:
  proof=row['proof'].get('renderedFiles')
  require(isinstance(proof,dict),'Missing painted file title proof')
  message=row['model']['composeLedger']['messages'][0]
  require(proof.get('message')==message and message==rows[6]['model']['composeLedger']['messages'][0], 'Rendered reply lost original raw message')
  before=proof.get('before');require(isinstance(before,dict) and before==proof.get('after'),'Painted titles changed during original screenshot')
  g=row['geometry'];require(before.get('ok') is True and before.get('ts')==message['ts'] and before.get('viewport')==g['viewport'],'Painted titles belong to another reply/viewport')
  reply=[r for r in g['rows'] if r.get('ts')==message['ts']]
  require(len(reply)==1 and before.get('rowRect')==reply[0].get('rect'),'Painted title proof detached from captured message row')
  bounds=before.get('scrollRect',{});clip=before.get('clip',{});rectangle(bounds,g['viewport']);rectangle(clip,g['viewport'])
  expected_clip=dict(left=max(0,bounds['left']),top=max(0,bounds['top']),right=min(g['viewport'][0],bounds['right']),bottom=min(g['viewport'][1],bounds['bottom']))
  expected_clip.update(width=expected_clip['right']-expected_clip['left'],height=expected_clip['bottom']-expected_clip['top'])
  same_clip=set(clip)==set(expected_clip) and all(clip[k]==expected_clip[k] for k in ('left','top','right','bottom')) and all(derived_geometry_equal(clip[k],expected_clip[k]) for k in ('width','height'))
  require(same_clip and bounds['height']>=80 and type(before.get('scrollTop')) in (int,float) and math.isfinite(before['scrollTop']) and before['scrollTop']>=0,'Invalid painted transcript clip/scroll')
  tokens=before.get('tokens');require(isinstance(tokens,list) and len(tokens)==2,'Missing/duplicate painted file title')
  for value,id,kind in zip(tokens,(CANVAS,LIST),('canvas','file')):
   label=LABELS[id];require(value.get('id')==id and value.get('kind')==kind and value.get('text')==value.get('renderedText')==label,'Painted file identity/kind/canonical title changed')
   require(isinstance(before.get('rowText'),str) and before['rowText'].count(label)==1 and reply[0].get('text','').count(label)==1,'File title absent/duplicated in actual accepted row text')
   r=value.get('rect',{});rectangle(r,g['viewport'])
   full=lambda r:r['left']>=clip['left']-1 and r['right']<=clip['right']+1 and r['top']>=clip['top']-1 and r['bottom']<=clip['bottom']+1
   height=max(0,min(r['bottom'],clip['bottom'])-max(r['top'],clip['top']))
   measured=value.get('visibleHeight')
   # JSON/DOM coordinate arithmetic can differ by a few binary float ulps.
   same_height=derived_geometry_equal(measured,height)
   require(full(r) and value.get('visible') is True and value.get('hit') is True and same_height and height>=r['height']-1,'Painted title clipped or occluded')
   fragments=value.get('textRects');require(isinstance(fragments,list) and 0<len(fragments)<=128,'Missing rendered title text ranges')
   for fragment in fragments:
    f=fragment.get('rect',{});rectangle(f,g['viewport'])
    require(full(f) and fragment.get('hit') is True and f['left']>=r['left']-1 and f['right']<=r['right']+1 and f['top']>=r['top']-1 and f['bottom']<=r['bottom']+1,'Rendered title text not visibly owned')

def validate(fixture,receipt,directory,*,expected_case=None,dimensions=None,png=True):
 case=expected_case or receipt.get('case');require(case in STAGES and receipt.get('case')==case and fixture in C['FIXTURES'] and receipt.get('fixture')==fixture and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True,'Wrong file receipt')
 ready=receipt.get('readiness',{});H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
 rows=receipt.get('stages',[]);require([r.get('stage') for r in rows]==list(STAGES[case]),'Missing file stages')
 painted_files(rows,case)
 _,scope,a=G['owner_and_records'](receipt,rows);sources(rows,case);windows(receipt,rows,directory,dimensions,png)
 mapped={'file-completion':'group-completion','file-completion-custody':'group-completion-custody','file-completion-off':'group-completion-restricted'}[case]
 base=rows[:8] if case=='file-completion-custody' else rows
 G['drafts_and_geometry'](base,mapped,scope,a);G['writes_and_reads'](base,mapped,scope,a)
 completion(rows,case,a)
 if case=='file-completion-custody':drift(rows,scope)
 for row in rows:
  path=Path(directory)/f'{fixture}-{row["stage"]}.png';require(Path(row.get('capture',''))==path,'Foreign capture')
  if png:H['png_signature'](path,dimensions)
 require(receipt.get('final')==rows[-1]['model'],'Final model changed');return receipt
