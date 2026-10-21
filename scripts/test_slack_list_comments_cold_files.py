"""Independent cold singleton lifecycle and actual checker-JS adversarial cases."""
import copy,json,runpy,subprocess,unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

V=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_cold_files.py')))
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_file_navigation.py')))
N,F,G,R,C=(V[k]for k in ('N','F','G','R','C'))
CANVAS,LIST=V['CANVAS'],V['LIST'];DIR=Path('/private/cold-files')

def projected(raws,unavailable=()):
 rows=[]
 for raw in raws:
  f=copy.deepcopy(raw['file']);f['channels']={CANVAS:['CZMAP001','CAMAP001','CTAIL001'],LIST:['CYMAP002','CBMAP002']}[f['id']]
  rows.append(dict(id=f['id'],source=copy.deepcopy(raw['file']),effective=f,receipt=copy.deepcopy(raw)))
 return dict(rows=rows,unavailableIds=list(unavailable),excludedIds=[])

def rawtext(raw):
 if raw is None:return 'null'
 if isinstance(raw.get('file'),dict):
  id=raw['file']['id'];text=V['ORIGINAL_TEXT'][id]
  if raw!=V['BASE'][id]:
   # Literal fixture replaces these scalar values, never reparses map order.
   for key in ('title','name'):
    text=text.replace(json.dumps(V['BASE'][id]['file'][key]),json.dumps(raw['file'][key]),1)
   text=text.replace('"updated":101','"updated":102')
  return text
 return json.dumps(raw,separators=(',',':'))

def coldledger(m,success,index):
 counts=[1,2,2,2,2,2]if success else[2,3,4,5,6,6]
 completed=[0,2,2,2,2,2]if success else[2,2,4,5,5,6]
 cancelled=[0]*6 if success else[0,0,1,1,1,2]
 ids,modes,raws=V['expected_requests'](success);requests=[];responses=[]
 for j in range(counts[index]):
  requests.append(dict(serial=j+1,id=ids[j],method='files.info',httpMethod='POST',path='/api/files.info',args=dict(file=ids[j],page='1',count='500',truncate='true'),mode=modes[j],plannedResponse=copy.deepcopy(raws[j]),responseText=rawtext(raws[j])))
  responses.append(dict(serial=j+1,id=ids[j],disposition=modes[j],raw=copy.deepcopy(raws[j]),responseText=rawtext(raws[j])))
 releases=([dict(serial=j+1,id=id,delivered=True,cancelledBeforeReply=False)for j,id in enumerate((CANVAS,LIST))]if index>=1 else[])if success else([dict(serial=3,id=CANVAS,delivered=False,cancelledBeforeReply=True),dict(serial=4,id=LIST,delivered=True,cancelledBeforeReply=False)]if index>=2 else[])
 if not success and index==5:releases.append(dict(serial=6,id=CANVAS,delivered=False,cancelledBeforeReply=True))
 heldserial=1 if success and index==0 else 3 if not success and index==1 else 6 if not success and index==4 else None
 wanted=([[],list(V['BASE'].values())][index>0]if success else[[],[],[V['BASE'][LIST]],[V['BASE'][LIST],V['renamed'](V['RENAMED'])],[V['BASE'][LIST]],[V['BASE'][LIST]]][index])
 future=copy.deepcopy(V['BASE'])
 if not success and index>=2:future[CANVAS]=V['renamed'](V['NEXT']if index>=4 else V['RENAMED'])
 invalidations=[]
 if not success and index>=2:invalidations.append(dict(kind='accepted-local-file-invalidation',id=CANVAS,futureResponse=V['renamed'](V['RENAMED']),before=projected([]),after=projected([])))
 if not success and index>=4:invalidations.append(dict(kind='accepted-local-file-invalidation',id=CANVAS,futureResponse=V['renamed'](V['NEXT']),before=projected([V['BASE'][LIST],V['renamed'](V['RENAMED'])]),after=projected([V['BASE'][LIST]],[CANVAS])))
 return dict(enabled=True,named=True,protectedFixture=True,identityCurrent=True,account=m['account'],epoch=m['epoch'],team='T1',user=G['SELF'],networkCalls=0,httpCapability=False,requests=requests,responses=responses,releases=releases,settlements=[dict(serial=j,cancelled=not success and j in (3,6))for j in range(1,completed[index]+1)],invalidations=invalidations,completed=completed[index],cancelled=cancelled[index],held=[]if heldserial is None else[dict(serial=heldserial,id=CANVAS,receiverClosed=False)],futureResponses=future,futureResponseTexts={id:rawtext(raw)for id,raw in future.items()},source=projected(wanted,[CANVAS]if not success and index>=4 else[]))

def coldgeometry(plan,viewport):
 rows=[]
 for id,(state,label,retry,notice)in zip((CANVAS,LIST),plan):
  rows.append(dict(ok=True,actual=dict(id=id,kind='canvas'if id==CANVAS else'file',state=state,label=label,disabled=state!='ready',busy='true'if state=='loading'else'false',retry=retry),title=B['measured'](label),status=B['measured'](notice)if notice else None,retry=B['measured']('Retry')if retry else None,retryControl=dict(tag='BUTTON',type='button',href=None,disabled=False)if retry else None))
 return dict(ok=True,ts=V['TS'],tokens=rows,viewport=viewport,noOverflow=True)

def sample(case='cold-files',narrow=False):
 fixture,base=B['sample']('file-navigation',narrow);success=case=='cold-files';rows=[];clicks=[]
 blocks=copy.deepcopy(G['seed_blocks']());blocks[0]['elements'].append(dict(type='rich_text_section',elements=[dict(type='text',text='Final review \u4e2d\U0001f642',style={'bold':True})]))
 initial=base['stages'][0]['model'];navseed=base['stages'][0]['proof']['received'];seed=base['stages'][0]['proof']['seed']
 readset=copy.deepcopy(initial['ledger']['requests'][:4]);retries=[];cancelled_absent=None;cancelled_visit=None
 for i,name in enumerate(V['STAGES'][case]):
  template=base['stages'][([0,0,1,2,3,4][i]if success else 0)];row=copy.deepcopy(template);m=row['model'];p=row['proof'];g=row['geometry']
  row.update(stage=name,capture=str(DIR/f'{fixture}-{name}.png'))
  for k in ('requests','responses','releases'):m['completionSource'][k]=[]
  m['completionSource'].update(completed=0,cancelled=0,held=0);m['completionSource']['files']={'enabled':False}
  returns=([0,0,0,1,1,2]if success else[0]*6)[i]
  m['navigation']['returns']=returns
  m['ledger'].update(requests=[dict(copy.deepcopy(r),serial=j+1)for j,r in enumerate(readset*(returns+1))],completed=4*(returns+1),cancelled=0,held=False)
  if not success:m['navigation']['targetRequests']=[]
  if success and name in ('canvas-open','list-open'):
   id=CANVAS if name=='canvas-open'else LIST;edit=N['CANVAS_EDIT']if id==CANVAS else N['LIST_EDIT']
   blocks=B['append'](blocks,edit);native=dict(blocks=copy.deepcopy(blocks),composing=False,revision=10+i,generation=1+returns)
   clicks.append(dict(ok=True,id=id,kind='canvas'if id==CANVAS else'file',ts=V['TS'],before=B['measured'](F['LABELS'][id]),input=dict(ok=True,snapshot=copy.deepcopy(native)),draft=native,defaultPrevented=False,control=dict(tag='BUTTON',type='button',href=None,disabled=False)))
  if not success:
   new=([CANVAS,LIST]if i==1 else[CANVAS]if i in(3,4)else[])
   retries.extend(dict(ok=True,id=id,before=B['measured']('Retry'),control=dict(tag='BUTTON',type='button',href=None,disabled=False))for id in new)
  cold=coldledger(m,success,i);m['coldFiles']=cold
  p=dict(seed=seed,coldSeed=dict(navigation=navseed,initialResponses=copy.deepcopy(V['BASE']),initialResponseTexts=V['ORIGINAL_TEXT'],canonicalSeeded=False,searchRequests=0),received=navseed,expectedBlocks=copy.deepcopy(blocks),clicks=copy.deepcopy(clicks),retries=copy.deepcopy(retries),releases=copy.deepcopy(cold['releases']),runtimeForgotten='return'in name,coldExpected=None)
  if not success and i>=2:
   p['invalidation']=copy.deepcopy(cold['invalidations'][0])
   if cancelled_absent is None:
    cancelled_absent=copy.deepcopy(cold);cancelled_absent['requests']=cold['requests'][:3];cancelled_absent['held']=[dict(serial=3,id=CANVAS,receiverClosed=True)];cancelled_absent['settlements']=cold['settlements'][:3]
   p['cancelledAbsentRead']=copy.deepcopy(cancelled_absent)
  if not success and i>=4:p['visitInvalidation']=copy.deepcopy(cold['invalidations'][1])
  if not success and i==5:
   cancelled_visit=copy.deepcopy(cold);cancelled_visit['held']=[dict(serial=6,id=CANVAS,receiverClosed=True)];p['cancelledVisitRead']=cancelled_visit
  d=m['draftStore']['drafts'][0];d.update(blocks=copy.deepcopy(blocks),capture='00000000-0000-4000-8000-000000000001',savedRevision=3+i,attempt=None,error=None,busy=False,saving=False,loading=False,terminalPending=False)
  p['journal']=dict(scope=seed['scope'],revision=d['savedRevision'],capture=d['capture'],blocks=copy.deepcopy(blocks),attempt=None)
  if name=='visit-retired':g=dict(ok=True,record=V['READ']['KNOWN'],editorAbsent=True,viewport=g['viewport'],noOverflow=True)
  elif name not in ('canvas-open','list-open'):
   g['native']=dict(blocks=copy.deepcopy(blocks),composing=False,generation=1+returns);g['editor'].update(blocks=copy.deepcopy(blocks),editable=True)
   plan=V['token_plan'](success,i);p['coldExpected']=dict(ts=V['TS'],tokens=[dict(id=id,state=v[0],label=v[1],retry=v[2])for id,v in zip((CANVAS,LIST),plan)])
   p['coldGeometry']=coldgeometry(plan,g['viewport'])
  plan=None if name in ('canvas-open','list-open','visit-retired')else V['token_plan'](success,i)
  m['coldAcquisition']=dict(generation=10+returns,running=bool(plan and any(v[0]=='loading'for v in plan)),rows=[]if plan is None else[dict(id=id,serial=j+1,phase=v[0],state=v[0],current=True,ordinal=j,retryAt=0,retryable=v[2],notice=v[3])for j,(id,v)in enumerate(zip((CANVAS,LIST),plan))])
  row.update(proof=p,geometry=g,geometryAfter=copy.deepcopy(g));rows.append(row)
 return fixture,dict(ok=True,case=case,fixture=fixture,readiness=base['readiness'],stages=rows,final=copy.deepcopy(rows[-1]['model']),initialRecords=base['initialRecords'],noLiveNetwork=True)

class ColdFilesTests(unittest.TestCase):
 def check(self,f,r):return V['validate'](f,r,DIR,png=False)
 def reject(self,mutation,index=1,case='cold-files'):
  f,r=sample(case);mutation(r['stages'][index]);r['final']=copy.deepcopy(r['stages'][-1]['model'])
  with self.assertRaises((RuntimeError,KeyError,ValueError,TypeError,IndexError)):self.check(f,r)
 def test_success_and_recovery_both_widths(self):
  for case in V['STAGES']:
   for narrow in (False,True):
    with self.subTest(case=case,narrow=narrow):self.check(*sample(case,narrow))
 def test_missing_stage(self):
  f,r=sample();r['stages'].pop()
  with self.assertRaises(RuntimeError):self.check(f,r)
 def test_no_canonical_seed(self):self.reject(lambda r:r['proof']['coldSeed'].update(canonicalSeeded=True))
 def test_no_autocomplete_request(self):self.reject(lambda r:r['model']['completionSource']['requests'].append({'method':'search.autocomplete.files'}))
 def test_no_query_in_draft(self):self.reject(lambda r:r['proof']['expectedBlocks'][0]['elements'][0]['elements'].append(dict(type='text',text='@alex')))
 def test_wrong_singleton_request_id(self):self.reject(lambda r:r['model']['coldFiles']['requests'][0]['args'].update(file=LIST))
 def test_wrong_singleton_factory(self):self.reject(lambda r:r['model']['coldFiles']['requests'][0]['args'].pop('truncate'))
 def test_wrong_original_response(self):self.reject(lambda r:r['model']['coldFiles']['requests'][0]['plannedResponse']['file'].update(title='Forged'))
 def test_reordered_original_bytes(self):
  def corrupt(row):
   c=row['model']['coldFiles'];text=json.dumps(c['requests'][0]['plannedResponse'],sort_keys=True);c['requests'][0]['responseText']=c['responses'][0]['responseText']=text
  self.reject(corrupt)
 def test_response_bytes_not_same_receipt(self):self.reject(lambda r:r['model']['coldFiles']['responses'][0].update(responseText='{}'))
 def test_no_stale_delivery(self):self.reject(lambda r:r['model']['coldFiles']['releases'][0].update(delivered=True,cancelledBeforeReply=False),2,'cold-files-recovery')
 def test_missing_actual_cancellation(self):self.reject(lambda r:r['proof']['cancelledAbsentRead']['held'][0].update(receiverClosed=False),2,'cold-files-recovery')
 def test_visit_cancellation_missing_settlement(self):self.reject(lambda r:r['proof']['cancelledVisitRead'].update(settlements=[]),5,'cold-files-recovery')
 def test_absent_id_must_have_no_source_preimage(self):self.reject(lambda r:r['proof']['invalidation'].update(before=projected([V['BASE'][CANVAS]])),2,'cold-files-recovery')
 def test_stale_old_title_after_fresh_response(self):self.reject(lambda r:r['model']['coldFiles']['source']['rows'][1]['effective'].update(title='Alex Canvas'),3,'cold-files-recovery')
 def test_wrong_channel_map_order(self):self.reject(lambda r:r['model']['coldFiles']['source']['rows'][0]['effective'].update(channels=['CAMAP001','CZMAP001','CTAIL001']))
 def test_singleton_receipt_not_fabricated_batch(self):self.reject(lambda r:r['model']['coldFiles']['source']['rows'][0].update(receipt={'ok':True,'files':[V['BASE'][CANVAS]['file']]}))
 def test_unavailable_title_cannot_enable_navigation(self):self.reject(lambda r:r['proof']['coldGeometry']['tokens'][0]['actual'].update(disabled=False),0)
 def test_missing_loading_status(self):self.reject(lambda r:r['proof']['coldGeometry']['tokens'][0].update(status=None),0)
 def test_missing_original_loading_attempt(self):self.reject(lambda r:r['model']['coldAcquisition'].update(rows=[]),0)
 def test_foreign_acquisition_notice(self):self.reject(lambda r:r['model']['coldAcquisition']['rows'][0].update(notice='Ready'),0,'cold-files-recovery')
 def test_wrong_offline_status(self):self.reject(lambda r:r['proof']['coldGeometry']['tokens'][0]['status'].update(text=V['UNAVAILABLE']),0,'cold-files-recovery')
 def test_retry_disabled(self):self.reject(lambda r:r['proof']['coldGeometry']['tokens'][0]['retryControl'].update(disabled=True),0,'cold-files-recovery')
 def test_missing_explicit_retry(self):self.reject(lambda r:r['proof'].update(retries=[]),1,'cold-files-recovery')
 def test_opaque_received_raw_changed(self):self.reject(lambda r:r['model']['navigation']['received'].update(blocks=[]))
 def test_draft_journal_changed(self):self.reject(lambda r:r['proof']['journal'].update(blocks=[]))
 def test_no_post(self):self.reject(lambda r:r['model']['composeLedger']['posts'].append({}))
 def test_actual_target_body_required(self):self.reject(lambda r:r['geometry']['content'].update(text='Original page'),2)
 def test_target_route_identity(self):self.reject(lambda r:r['model']['route'].update(canvasId=LIST),2)
 def test_real_journal_reopen(self):self.reject(lambda r:r['proof'].update(runtimeForgotten=False),3)
 def test_visit_back_removes_editor(self):self.reject(lambda r:r['geometry'].update(editorAbsent=False),5,'cold-files-recovery')
 def test_root_case_registry_and_exact_capture_edges(self):
  for case in V['STAGES']:
   f,r=sample(case);stages,_=C['case_contract'](case);self.assertEqual(stages,V['STAGES'][case])
   edges=[dict(stage=s,edge=e)for s in stages for e in ('before','after')];packets={C['WAITING']:[r['readiness']],C['PREFIX']:[r],C['EDGE']:edges};native=SimpleNamespace(receipts=lambda k:packets[k])
   with patch.dict(C['validate_log'].__globals__,case_contract=lambda _:(stages,lambda *a,**k:r)):
    C['validate_log'](native,f,DIR,r['readiness'],None,case);packets[C['EDGE']]=edges[:-1]
    with self.assertRaisesRegex(RuntimeError,'Capture lifecycle'):C['validate_log'](native,f,DIR,r['readiness'],None,case)
 def test_actual_cold_probe_refuses_stale_unpainted_or_unavailable_controls(self):
  script=r'''
import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';
const code=fs.readFileSync('src/desktop/slack_lists/check/compose_comments/file_navigation/cold.js','utf8');
let hit=true,showRetry=true,showStatus=true,duplicate=false,clicks=0;
function node(text,attr,top){return {tagName:'BUTTON',disabled:false,textContent:text,dataset:{},attrs:{[attr]:'F1',type:'button'},getAttribute(k){return this.attrs[k]??null;},hasAttribute(k){return k in this.attrs;},contains(){return false;},scrollIntoView(){},dispatchEvent(){clicks++;},getBoundingClientRect(){return {left:10,top,right:110,bottom:top+20,width:100,height:20};}};}
const title=node('Original fallback','data-rich-file-id',10),retry=node('Retry','data-rich-file-retry',70),status=node('Unavailable','data-rich-file-status',40);
title.dataset={richFileId:'F1',richFileKind:'canvas',richFileState:'unavailable'};title.attrs['aria-busy']='false';title.disabled=true;
const row={dataset:{listCommentTs:'1.2'},querySelectorAll(selector){return selector==='[data-rich-file-id]'?(duplicate?[title,title]:[title]):selector==='[data-rich-file-retry]'?(showRetry?[retry]:[]):showStatus?[status]:[];}};
const document={documentElement:{scrollWidth:400},querySelectorAll(){return [row];},elementFromPoint(x,y){return hit?[title,status,retry].find(n=>n.getBoundingClientRect().top+10===y):null;}};
const probe=vm.runInNewContext(code,{document,innerWidth:400,innerHeight:300,getComputedStyle:()=>({display:'inline',visibility:'visible'}),MouseEvent:class{}});
const arg={ts:'1.2',tokens:[{id:'F1',state:'unavailable',label:'Original fallback',retry:true}]};
const ready=()=>probe('state',arg,()=>({ok:true})).ok;
assert.equal(ready(),true);
title.textContent='Forged title';assert.equal(ready(),false);title.textContent='Original fallback';
row.dataset.listCommentTs='stale';assert.equal(ready(),false);row.dataset.listCommentTs='1.2';
title.disabled=false;assert.equal(ready(),false);title.disabled=true;
showRetry=false;assert.equal(ready(),false);showRetry=true;
retry.disabled=true;assert.equal(ready(),false);retry.disabled=false;
retry.attrs.href='https://untrusted.invalid';assert.equal(ready(),false);delete retry.attrs.href;
showStatus=false;assert.equal(ready(),false);showStatus=true;
title.attrs['aria-busy']='true';assert.equal(ready(),false);title.attrs['aria-busy']='false';
duplicate=true;assert.equal(ready(),false);duplicate=false;
hit=false;assert.equal(ready(),false);hit=true;
assert.equal(probe('retry',{ts:'1.2',id:'F1'},()=>({ok:true})).ok,true);assert.equal(clicks,1);
retry.disabled=true;assert.equal(probe('retry',{ts:'1.2',id:'F1'},()=>({ok:true})).ok,false);retry.disabled=false;
showRetry=false;assert.equal(probe('retry',{ts:'1.2',id:'F1'},()=>({ok:true})).ok,false);showRetry=true;
assert.equal(probe('retry',{ts:'other',id:'F1'},()=>({ok:true})).ok,false);assert.equal(clicks,1);
console.log('15 actual cold probe cases passed');
'''
  result=subprocess.run(['node','--input-type=module','-e',script],cwd=Path(__file__).resolve().parents[1],capture_output=True,text=True)
  self.assertEqual(result.returncode,0,result.stderr);self.assertIn('15 actual cold probe cases passed',result.stdout)
 def test_actual_inline_text_probe_preserves_real_ancestor_clipping_and_full_text(self):
  script=r'''
import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';
const current=fs.readFileSync('src/desktop/slack_lists/check/compose_comments/file_navigation/cold_text.js','utf8');
const original=fs.readFileSync('src/desktop/slack_lists/check/compose_comments/reaction_actions/text.js','utf8');
const box=(left,top,width,height)=>({left,top,right:left+width,bottom:top+height,width,height});
function run(source,o={}){
 const parent={hidden:!!o.hidden,parentElement:null,clientLeft:0,clientTop:0,clientWidth:o.width??400,clientHeight:o.height??300,getBoundingClientRect:()=>box(0,0,400,300)};
 const root={isConnected:o.connected!==false,hidden:false,parentElement:parent,clientLeft:0,clientTop:0,clientWidth:o.rootWidth??0,clientHeight:o.rootHeight??0,getBoundingClientRect:()=>box(20,20,180,40),contains:n=>n===root};
 const text={textContent:'Loading file and remaining text',parentElement:root};
 const fragments=o.fragments??[box(20,20,140,20)];
 const document={createTreeWalker(){let done=false;return {nextNode(){if(done)return null;done=true;return text;}};},createRange(){return {selectNodeContents(){},getClientRects(){return fragments;}};},elementFromPoint(){return o.occluded?{}:root;}};
 const probe=vm.runInNewContext(source,{document,NodeFilter:{SHOW_TEXT:4},innerWidth:400,innerHeight:300,getComputedStyle:n=>({display:n===root?(o.display??'inline'):'block',visibility:n===parent&&o.invisible?'hidden':'visible',overflowX:n===parent?'hidden':'visible',overflowY:n===parent?'auto':'visible'})});
 return probe(root);
}
assert.equal(run(original).ok,false); // The exact editor-only probe collapses the inline root.
let result=run(current);assert.equal(result.ok,true);assert.equal(result.visibleLines,1);assert.equal(result.clip.width,400);assert.equal(result.clip.height,300);
const cases=[
 ['horizontal ancestor clip',{width:100},false],
 ['vertical ancestor clip',{height:30},false],
 ['hidden ancestor',{hidden:true},false],
 ['invisible ancestor',{invisible:true},false],
 ['disconnected original',{connected:false},false],
 ['occluded fragment',{occluded:true},false],
 ['real zero block root',{display:'block'},false],
 ['real small inline client box',{rootWidth:50,rootHeight:20},false],
 ['actual nonzero block box',{display:'block',rootWidth:180,rootHeight:40},true],
 ['fully painted two lines',{fragments:[box(20,20,140,10),box(20,40,100,10)]},true],
 ['one of two lines clipped',{height:35,fragments:[box(20,20,140,10),box(20,40,100,10)]},false],
 ['viewport clipping',{fragments:[box(20,290,140,20)]},false],
 ['no painted text',{fragments:[]},false],
 ['root box clips final glyph',{rootWidth:139,rootHeight:20,fragments:[box(20,20,141,20)]},false]
];
for(const [name,options,expected]of cases)assert.equal(run(current,options).ok,expected,name);
result=run(current,{height:35,fragments:[box(20,20,140,10),box(20,40,100,10)]});assert.equal(result.visibleLines,1);assert.equal(result.lines.length,2);assert.equal(result.ok,false);
console.log('16 actual inline/clipping probe scenarios passed');
'''
  result=subprocess.run(['node','--input-type=module','-e',script],cwd=Path(__file__).resolve().parents[1],capture_output=True,text=True)
  self.assertEqual(result.returncode,0,result.stderr);self.assertIn('16 actual inline/clipping probe scenarios passed',result.stdout)

if __name__=='__main__':unittest.main()
