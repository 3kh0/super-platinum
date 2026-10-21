"""Portable positive and adversarial List file completion evidence; no native IO."""
import copy,json,runpy,unittest,subprocess,hashlib
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
V=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_file_completion.py')))
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_group_completion.py')))
DIR=Path('/private/file-completion');G=V['G'];CANVAS,LIST,HOSTED=V['CANVAS'],V['LIST'],V['HOSTED']

def convert(v):
 if isinstance(v,list):return [convert(x) for x in v]
 if isinstance(v,dict):
  if v.get('type')=='usergroup' and v.get('usergroup_id') in (G['G2'],G['G1']):
   return {**V['token'](CANVAS if v['usergroup_id']==G['G2'] else LIST),**{k:convert(x) for k,x in v.items() if k not in ('type','usergroup_id')}}
  return {k:convert(x) for k,x in v.items()}
 return v

def sourceproof(acquired,invalidated=False):
 if not acquired:return {'unavailable':True}
 rows=[]
 for raw in V['RAW'][:2]:
  if invalidated and raw['id']==CANVAS:continue
  e=copy.deepcopy(raw);e['channels']=['CZMAP001','CAMAP001','CTAIL001'] if raw['id']==CANVAS else ['CYMAP002','CBMAP002']
  rows.append(dict(id=raw['id'],source=copy.deepcopy(raw),effective=e))
 return dict(rows=rows,unavailableIds=[CANVAS] if invalidated else [],excludedIds=[HOSTED])

def popup(row,ids,sequence=10,revision=3,settled=True):
 m,g=row['model'],row['geometry'];values=[V['choice'](id) for id in ids]
 m['completion']=dict(sequence=sequence,revision=revision,current=True,batchCurrent=True,settledBatch=settled,started=True,consumed=False,friendly=False,values=values,labels=[v['label'] for v in values])
 rect=lambda top,height:dict(left=10,top=top,right=210,bottom=top+height,width=200,height=height)
 g['completion']=dict(sequence=sequence,revision=revision,open=True,captureMode='interactive',status=None,popup=dict(visible=True,hit=True,rect=rect(10,180)),scroll=dict(top=0,height=180,total=180),rows=[dict(id=f"slack-list-channel-choice-{g['editor']['generation']}-{sequence}-{i}",type='button',role='option',selected=str(i==0).lower(),label=v['label'],visible=True,hit=True,fullyVisible=True,rect=rect(10+20*i,20)) for i,v in enumerate(values)])

def sample(case='file-completion',narrow=False):
 success=case=='file-completion';off=case=='file-completion-off'
 f,r=B['restricted_sample'](narrow) if off else B['sample']('group-completion' if success else 'group-completion-custody',narrow)
 if success:r=convert(r)
 if off:
  def unrestricted(x):
   if isinstance(x,dict):
    if x.get('id')==G['SELF'] and 'is_restricted' in x:x['is_restricted']=False
    for y in x.values():unrestricted(y)
   elif isinstance(x,list):
    for y in x:unrestricted(y)
  unrestricted(r)
 if not success and not off:
  old=r['stages'][8];p=old['proof'];change=p['sourceChanges'][0]
  old['model']=copy.deepcopy(change['before']);old['geometry']=copy.deepcopy(change['original']);p.pop('sourceChanges',None)
  r['stages'][9]=copy.deepcopy(old)
 rows=r['stages'];r['case']=case
 counts=[0,1,1,1,1,1,1,1,1] if success else [0,0,0] if off else [0,1,2,2,3,4,5,6,7,7]
 done=[0,0,1,1,1,1,1,1,1] if success else [0,0,0] if off else [0,0,2,2,3,4,5,6,7,7]
 cancelled=[0]*len(rows) if success or off else [0,0,1,1,1,1,1,2,2,2]
 held=[0,1,0,0,0,0,0,0,0] if success else [0,0,0] if off else [0,1,1,0,0,0,0,0,0,0]
 settled={};inputs=[]
 for i,row in enumerate(rows):
  stage=V['STAGES'][case][i];row['stage']=stage;row['capture']=str(DIR/f'{f}-{stage}.png')
  m,p,g=row['model'],row['proof'],row['geometry'];p.pop('sourceChanges',None);p.pop('bottomWindow',None);p.pop('settled',None)
  source=m['completionSource'];oldreq=source['requests'];oldresp=source['responses'];source['requests']=[];source['responses']=[]
  source['seed']=G['seed_source']();source['groups']=G['group_state'](G['group_rows']()[:1] if i==0 else G['group_rows']())
  # Restricted baseline omits group requests; the off file case still permits groups.
  if off and i:
   oldreq.append(dict(serial=0,method='usergroups/search',args=dict(query='alex',count=25,org_wide=True)));oldresp.append(dict(serial=0,disposition='response',raw=dict(ok=True,results=G['group_rows']())))
  file_queries=[]
  for req,reply in zip(oldreq,oldresp):
   req=copy.deepcopy(req);reply=copy.deepcopy(reply);method=req['method'];serial=len(source['requests'])+1;req['serial']=reply['serial']=serial
   if method=='experiments.getByUser':reply['raw']=copy.deepcopy(V['observations'](off)[method][1])
   if method=='usergroups/search':reply.update(disposition='response',raw=dict(ok=True,results=G['group_rows']()))
   source['requests'].append(req);source['responses'].append(reply)
   if method=='usergroups/search' and not off and len(file_queries)<counts[i]:
    q=req['args']['query'];j=len(file_queries);serial+=1
    disp='held' if j==0 or not success and j==5 else 'offline' if not success and j==3 else 'response'
    raw=None if disp=='offline' else '{"ok":true,"files":false}' if not success and j==2 else '{"ok":true,"files":[]}' if not success and j==4 else V['RESPONSE']
    source['requests'].append(dict(serial=serial,method='search.autocomplete.files',args=dict(query=q,include_shares='true',_x_reason='texty-autocomplete')))
    source['responses'].append(dict(serial=serial,disposition=disp,raw=raw));file_queries.append(serial)
  source.update(completed=done[i],cancelled=cancelled[i],held=held[i],releases=[])
  for j in ([0] if success and i>=2 else [0,5] if not success and not off and i>=7 else [0] if not success and not off and i>=3 else []):source['releases'].append(dict(serial=file_queries[j],delivered=success,cancelledBeforeReply=not success))
  source['files']=dict(enabled=True,gate='off' if off else 'on',originalResponse=V['RESPONSE'],source=sourceproof(not off and i>=2,not success and not off and i==9))
  p['releases']=[dict(stage='refreshed' if success else 'stale-refused' if j==0 else 'cancelled-reopened',value=x) for j,x in enumerate(source['releases'])]
  if success:
   for post in m['composeLedger']['posts']+m['composeLedger']['prepared']:post['args']['blocks']=json.dumps(convert(json.loads(post['args']['blocks'])))
  blocks=p['expectedBlocks'];g['editor']['text']=V['display'](blocks)
  for t in g.get('native',{}).get('tokens',[]):
   if t['value'].get('type') in ('canvas','file'):t['label']=V['LABELS'][t['value']['file_id']]
  if stage in ('local-held','held','off-results','refreshed','source-ready'):
   popup(row,V['BASE_IDS']+([CANVAS,LIST] if stage in ('refreshed','source-ready') else []),80 if stage=='source-ready' else 10,3,stage not in ('local-held','held'))
  if stage in ('query-retired','stale-refused','empty','malformed','failure'):
   status=V['FAILURE'] if stage in ('malformed','failure') else V['EMPTY'];g['completion']['status']['text']=status
  if stage=='source-refused':
   m['completion']=None;g['completion']=dict(sequence=0,revision=0,scroll=None,captureMode='interactive',open=False,rows=[],status=None,popup=None)
  # Rewrite only the new case's path names, exact selected identities and publications.
  items=copy.deepcopy(p['inputs'])
  for item in items:
   item['stage']={'restricted-results':'off-results','source-renamed':'source-ready'}.get(item['stage'],item['stage'])
   if item['kind']=='choose':
    id=G['SECOND'] if off else CANVAS if item['value']['keyboard'] else LIST
    item['selected']=id;choices=[V['choice'](x) for x in V['BASE_IDS']+([] if off else [CANVAS,LIST])]
    item['values']=choices;pub=copy.deepcopy(rows[1 if off else 2]['model']['completion']);pub.update(values=choices,labels=[x['label'] for x in choices],settledBatch=True)
    if not off and id==LIST:pub['sequence']=20
    item['publication']=pub;item['value'].update(label=V['LABELS'][id],index=next(j for j,x in enumerate(choices) if V['identity'](x)==id),sequence=pub['sequence'],revision=pub['revision'])
  p['inputs']=items
  if stage in ('query-retired','stale-refused','empty','malformed','failure','off-results','source-ready'):
   q=m['completion'];status=V['FAILURE'] if stage in ('malformed','failure') else V['EMPTY'] if stage in ('query-retired','stale-refused','empty') else ''
   settled[stage]=dict(completion=copy.deepcopy(q),native=dict(ok=True,ready=True,sequence=q['sequence'],revision=q['revision'],labels=q['labels'],status=status))
  if settled:p['settled']=copy.deepcopy(settled)
  if stage in ('local-held','held','off-results','refreshed','source-ready'):
   for pos in (['middle','bottom'] if stage in ('refreshed','source-ready') else ['bottom']):p[pos+'Window']=dict(model=copy.deepcopy(m),geometry=copy.deepcopy(g),capture=str(DIR/f'{f}-{stage}-{pos}.png'))
 if not success and not off:
  ready,row=rows[8:];before=copy.deepcopy(ready['model']);after=copy.deepcopy(row['model']);original=copy.deepcopy(ready['geometry']);snap=original['native']['snapshot']
  row['proof']['sourceChange']=dict(id=CANVAS,before=before,retired=after,after=copy.deepcopy(after),original=original,index=7,
   operation=dict(id=CANVAS,before=sourceproof(True),after=sourceproof(True,True),eventRevisionBefore=0,eventRevisionAfter=0),staleChoice=dict(ok=True,accepted=False,before=copy.deepcopy(snap),after=copy.deepcopy(snap)))
 if success:
  for row in rows[7:9]:
   message=row['model']['composeLedger']['messages'][0];text=V['display'](message['blocks'])
   reply=next(v for v in row['geometry']['rows'] if v['ts']==message['ts']);reply['text']=text
   rect=lambda left,top,width,height:dict(left=left,top=top,right=left+width,bottom=top+height,width=width,height=height)
   bounds=rect(10,10,200,180);tokens=[]
   for id,kind,left,width in ((CANVAS,'canvas',15,90),(LIST,'file',115,65)):
    box=rect(left,15,width,16);tokens.append(dict(id=id,kind=kind,text=V['LABELS'][id],renderedText=V['LABELS'][id],rect=box,textRects=[dict(rect=copy.deepcopy(box),hit=True)],visible=True,hit=True,visibleHeight=16))
   proof=dict(ok=True,ts=message['ts'],viewport=row['geometry']['viewport'],rowText=text,rowRect=reply['rect'],scrollTop=0,scrollRect=bounds,clip=copy.deepcopy(bounds),tokens=tokens)
   row['proof']['renderedFiles']=dict(message=copy.deepcopy(message),before=proof,after=copy.deepcopy(proof))
 r['final']=copy.deepcopy(rows[-1]['model']);return f,r

class FileTests(unittest.TestCase):
 def check(self,f,r):return V['validate'](f,r,DIR,png=False)
 def reject(self,change,stage=2,case='file-completion'):
  f,r=sample(case);change(r['stages'][stage]);r['final']=copy.deepcopy(r['stages'][-1]['model'])
  with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):self.check(f,r)
 def test_all_cases_and_widths(self):
  for case in V['STAGES']:
   for narrow in (False,True):
    with self.subTest(case=case,narrow=narrow):self.check(*sample(case,narrow))
 def test_original_bytes(self):self.reject(lambda r:r['model']['completionSource']['files'].update(originalResponse=json.dumps(json.loads(V['RESPONSE']))))
 def test_file_request_not_group(self):self.reject(lambda r:r['model']['completionSource']['requests'][-1].update(method='usergroups/search'))
 def test_file_wire_fields(self):self.reject(lambda r:r['model']['completionSource']['requests'][-1]['args'].update(count=25))
 def test_canonical_source(self):self.reject(lambda r:r['model']['completionSource']['files']['source']['rows'][0]['source'].pop('fixture_opaque'))
 def test_channel_order(self):self.reject(lambda r:r['model']['completionSource']['files']['source']['rows'][0]['effective'].update(channels=['CAMAP001','CZMAP001','CTAIL001']))
 def test_hosted_exclusion(self):self.reject(lambda r:r['model']['completionSource']['files']['source'].update(excludedIds=[]))
 def test_final_settlement(self):self.reject(lambda r:r['model']['completion'].update(settledBatch=False))
 def test_wrong_file_id(self):self.reject(lambda r:r['model']['completion']['values'][-1]['value'].update(file_id=HOSTED))
 def test_title_not_url(self):self.reject(lambda r:r['geometry']['native']['tokens'][-1].update(label=V['token'](LIST)['url']),4)
 def test_reopen_title(self):self.reject(lambda r:r['geometry']['native']['tokens'][-1].update(label='FLISTFILE2'),5)
 def test_exact_wire_url(self):self.reject(lambda r:r['proof']['expectedBlocks'][0]['elements'][0]['elements'][-2].update(url='https://wrong.invalid'),4)
 def test_opaque_style(self):self.reject(lambda r:r['proof']['expectedBlocks'][0]['elements'][0]['elements'][4].update(style={}),4)
 def test_saved_journal(self):self.reject(lambda r:r['proof']['journal'].update(blocks=[]),5)
 def test_no_extra_send(self):self.reject(lambda r:r['model']['composeLedger']['posts'].append(copy.deepcopy(r['model']['composeLedger']['posts'][0])),6)
 def test_cancelled_response(self):self.reject(lambda r:r['model']['completionSource']['releases'][0].update(delivered=True,cancelledBeforeReply=False),3,'file-completion-custody')
 def test_stale_choice(self):self.reject(lambda r:r['proof']['staleChoice'].update(accepted=True),3,'file-completion-custody')
 def test_source_refusal(self):self.reject(lambda r:r['proof']['sourceChange']['staleChoice'].update(accepted=True),9,'file-completion-custody')
 def test_off_stays_off(self):self.reject(lambda r:r['model']['completionSource']['files'].update(gate='on'),1,'file-completion-off')
 def test_off_still_groups(self):self.reject(lambda r:r['model']['completion']['values'].pop(),1,'file-completion-off')
 def test_middle_capture_current(self):self.reject(lambda r:r['proof']['middleWindow']['model']['completion'].update(revision=999))
 def test_unreachable_row(self):
  def change(r):
   for g in [r['geometry'],r['proof']['middleWindow']['geometry'],r['proof']['bottomWindow']['geometry']]:g['completion']['rows'][-1].update(fullyVisible=False,visible=False,hit=False,rect=dict(left=10,top=300,right=210,bottom=320,width=200,height=20))
  self.reject(change)
 def test_live_network_refused(self):self.reject(lambda r:r['model']['completionSource'].update(httpCapability=True))

 def test_input_missing(self):self.reject(lambda r:r['proof']['inputs'].clear(),3)
 def test_original_input_buffer(self):self.reject(lambda r:r['proof']['inputs'][-1]['value']['before'].update(blocks=[]),3)
 def test_keyboard_publication(self):self.reject(lambda r:r['proof']['inputs'][-1]['publication'].update(sequence=999),3)
 def test_wrong_group_query(self):
  def change(r):next(v for v in r['model']['completionSource']['requests'] if v['method']=='usergroups/search')['args']['query']='wrong'
  self.reject(change)
 def test_invalidation_original_source(self):self.reject(lambda r:r['proof']['sourceChange']['operation'].update(before={}),9,'file-completion-custody')
 def test_invalidation_not_server_event(self):self.reject(lambda r:r['proof']['sourceChange']['operation'].update(eventRevisionAfter=1),9,'file-completion-custody')
 def test_same_query_refresh(self):self.reject(lambda r:r['model']['completion'].update(sequence=98))
 def test_failure_source_not_empty(self):self.reject(lambda r:r['model']['completionSource']['responses'][-1].update(raw='{"ok":true,"files":[]}'),5,'file-completion-custody')
 def test_capture_lifecycle(self):
  c=V['C']
  for case in V['STAGES']:
   f,r=sample(case);captures=[]
   for stage in V['STAGES'][case]:
    captures.append(stage)
    if stage in ('refreshed','source-ready'):captures.extend([stage+'-middle',stage+'-bottom'])
    elif stage in ('local-held','held','off-results'):captures.append(stage+'-bottom')
   self.assertEqual(len(captures),{'file-completion':12,'file-completion-custody':13,'file-completion-off':4}[case])
   edges=[dict(stage=s,edge=e) for s in captures for e in ('before','after')]
   packets={c['WAITING']:[r['readiness']],c['PREFIX']:[r],c['EDGE']:edges}
   native=SimpleNamespace(receipts=lambda prefix:packets[prefix])
   with patch.dict(c['validate_log'].__globals__,case_contract=lambda _: (V['STAGES'][case],lambda *args,**kw:r)):
    c['validate_log'](native,f,DIR,r['readiness'],None,case)
    for bad in (edges[:-1],edges+[dict(stage='extra',edge='before')],edges[:3]+list(reversed(edges[3:5]))+edges[5:]):
     packets[c['EDGE']]=bad
     with self.assertRaisesRegex(RuntimeError,'Capture lifecycle edges changed'):c['validate_log'](native,f,DIR,r['readiness'],None,case)
 def test_dispatch(self):
  for case in V['STAGES']:self.assertEqual(V['C']['case_contract'](case)[0],V['STAGES'][case])
 def painted_reject(self,change):
  def alter(row):
   proof=row['proof']['renderedFiles'];change(proof['before']);proof['after']=copy.deepcopy(proof['before'])
  self.reject(alter,7)
 def test_painted_titles_required(self):self.reject(lambda r:r['proof'].pop('renderedFiles'),7)
 def test_painted_both_tokens_required(self):self.painted_reject(lambda p:p['tokens'].pop())
 def test_painted_kind_identity(self):self.painted_reject(lambda p:p['tokens'][0].update(kind='file'))
 def test_painted_title_not_id_fallback(self):self.painted_reject(lambda p:p['tokens'][0].update(text=CANVAS,renderedText=CANVAS))
 def test_painted_url_not_title(self):self.painted_reject(lambda p:p['tokens'][0].update(text=V['token'](CANVAS)['url']))
 def test_painted_text_must_be_rendered(self):self.painted_reject(lambda p:p['tokens'][0].update(renderedText=''))
 def test_painted_text_hit(self):self.painted_reject(lambda p:p['tokens'][0]['textRects'][0].update(hit=False))
 def test_painted_title_clipped(self):
  def change(p):
   t=p['tokens'][0];t['rect'].update(top=185,bottom=201);t['visibleHeight']=5;t['textRects'][0]['rect']=copy.deepcopy(t['rect'])
  self.painted_reject(change)
 def test_painted_height_tolerance_is_absolute_and_tight(self):
  self.painted_reject(lambda p:p['tokens'][0].update(visibleHeight=16.000002))
 def test_painted_height_must_be_finite_numeric(self):
  for value in (True,'16',float('nan'),float('inf')):
   with self.subTest(value=value):self.painted_reject(lambda p:p['tokens'][0].update(visibleHeight=value))
 def test_painted_derived_clip_dimensions_allow_float_roundoff_only(self):
  _,r=sample('file-completion')
  for row in r['stages'][7:9]:
   p=row['proof']['renderedFiles'];p['before']['clip']['width']+=1e-9;p['before']['clip']['height']-=1e-9;p['after']=copy.deepcopy(p['before'])
  V['painted_files'](r['stages'],'file-completion')
  self.painted_reject(lambda p:p['clip'].update(width=p['clip']['width']+2e-6))
  self.painted_reject(lambda p:p['clip'].update(height=p['clip']['height']-2e-6))
  self.painted_reject(lambda p:p['clip'].update(left=p['clip']['left']+1e-9))
 def test_painted_before_after_exact(self):self.reject(lambda r:r['proof']['renderedFiles']['after']['tokens'][0].update(text='changed'),7)
 def test_painted_original_raw_message(self):self.reject(lambda r:r['proof']['renderedFiles']['message'].update(blocks=[]),7)
 def test_painted_wrong_ts(self):self.painted_reject(lambda p:p.update(ts='1781524801.000001'))
 def test_painted_transcript_text_missing(self):self.painted_reject(lambda p:p.update(rowText='Original body without files'))
 def test_retained_false_positive_a_receipts_rejected_unchanged(self):
  base=Path(__file__).resolve().parents[1]/'tmp/list-comments-file-completion-20261019'
  paths=[base/'success-normal-a/slack-list/slack-list.json',base/'success-narrow-a/slack-list-narrow/slack-list-narrow.json']
  if not all(p.is_file() for p in paths):self.skipTest('Both original A receipts are not installed')
  for path in paths:
   original=path.read_bytes();e=json.loads(original);self.assertTrue(e['ok'])
   receipt=e['native'];self.assertTrue(receipt['ok'])
   for row in receipt['stages'][7:9]:
    ts=row['model']['composeLedger']['messages'][0]['ts'];painted=next(v for v in row['geometry']['rows'] if v['ts']==ts)
    self.assertNotIn('Alex Canvas',painted['text']);self.assertNotIn('Alex List',painted['text'])
   with self.assertRaisesRegex(RuntimeError,'Missing painted file title proof'):
    V['validate'](e['fixture'],receipt,Path(e['readyDirectory']),dimensions=e['expectedPngDimensions'])
   self.assertEqual(hashlib.sha256(original).digest(),hashlib.sha256(path.read_bytes()).digest())
 def test_retained_normal_b_receipt_and_original_pngs_pass_unchanged(self):
  path=Path(__file__).resolve().parents[1]/'tmp/list-comments-file-completion-20261019/success-normal-b/slack-list/slack-list.json'
  if not path.is_file():self.skipTest('Original normal B receipt is not installed')
  original=path.read_bytes();e=json.loads(original)
  self.assertFalse(e['ok']);self.assertEqual(e['error'],'Painted title clipped or occluded')
  self.assertEqual(len(e['terminalReceipts']),1);receipt=e['terminalReceipts'][0];self.assertTrue(receipt['ok'])
  p=receipt['stages'][7]['proof']['renderedFiles']['before'];t=p['tokens'][0];r=t['rect'];c=p['clip']
  height=max(0,min(r['bottom'],c['bottom'])-max(r['top'],c['top']))
  self.assertNotEqual(t['visibleHeight'],height);self.assertLess(abs(t['visibleHeight']-height),1e-6)
  captures=sorted(Path(e['readyDirectory']).glob('*.png'));self.assertEqual(len(captures),12)
  hashes={p:hashlib.sha256(p.read_bytes()).digest() for p in captures}
  V['validate'](e['fixture'],receipt,Path(e['readyDirectory']),dimensions=e['expectedPngDimensions'])
  self.assertEqual(hashlib.sha256(original).digest(),hashlib.sha256(path.read_bytes()).digest())
  self.assertEqual(hashes,{p:hashlib.sha256(p.read_bytes()).digest() for p in captures})
 def test_actual_transcript_probe(self):
  script=r'''
import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';
const src=fs.readFileSync('src/desktop/slack_lists/check/compose_comments/file_completion/transcript.js','utf8');
function fixture(){
 const expected={ts:'1.2',files:[{id:'F1',kind:'canvas',label:'Canvas title'},{id:'F2',kind:'file',label:'List title'}]};
 const box=(left,top,width,height)=>({left,top,right:left+width,bottom:top+height,width,height});
 const scroll={scrollTop:0,getBoundingClientRect:()=>box(10,40,260,100)};
 const nodes=expected.files.map((v,i)=>({dataset:{richFileId:v.id,richFileKind:v.kind},textContent:v.label,innerText:v.label,isConnected:true,opacity:'1',contains:n=>false,getBoundingClientRect:()=>box(20+i*120,150-scroll.scrollTop,100,20)}));
 const row={dataset:{listCommentTs:'1.2'},innerText:'Canvas title List title',querySelectorAll:()=>nodes,getBoundingClientRect:()=>box(10,40-scroll.scrollTop,260,200)};
 const matches=[row],pane={querySelector:()=>scroll,querySelectorAll:()=>matches};let occluded=false;
 const document={getElementById:()=>pane,createRange(){let node;return {selectNodeContents(n){node=n;},getClientRects(){return [node.getBoundingClientRect()];}};},elementFromPoint(x,y){return occluded?null:nodes.find(n=>{const r=n.getBoundingClientRect();return x>=r.left&&x<=r.right&&y>=r.top&&y<=r.bottom;});}};
 const probe=vm.runInNewContext(src,{document,innerWidth:400,innerHeight:300,getComputedStyle:n=>({display:'inline',visibility:'visible',opacity:n.opacity})});
 return {expected,scroll,nodes,matches,probe,occlude(){occluded=true;}};
}
let f=fixture();assert.equal(f.probe(f.expected,false).ok,false);let p=f.probe(f.expected,true);assert(p.ok);assert(f.scroll.scrollTop>0);assert(p.tokens.every(t=>t.hit&&t.visible));assert.deepEqual(JSON.parse(JSON.stringify(p)),JSON.parse(JSON.stringify(f.probe(f.expected,false))));
for(const change of [f=>f.nodes.pop(),f=>f.nodes[0].dataset.richFileKind='file',f=>f.nodes[0].textContent='F1',f=>f.nodes[0].innerText='',f=>f.nodes[0].opacity='0',f=>f.occlude(),f=>f.matches.push(f.matches[0])]){f=fixture();change(f);assert.equal(f.probe(f.expected,true).ok,false);}
console.log('8 actual transcript probe cases passed');
'''
  result=subprocess.run(['node','--input-type=module','-e',script],cwd=Path(__file__).resolve().parents[1],capture_output=True,text=True)
  self.assertEqual(result.returncode,0,result.stderr);self.assertIn('8 actual transcript probe cases passed',result.stdout)

if __name__=='__main__':unittest.main()
