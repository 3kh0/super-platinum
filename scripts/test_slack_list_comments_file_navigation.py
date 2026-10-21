"""Strict synthetic navigation receipts and pure native-probe regressions."""
import base64,copy,json,runpy,subprocess,unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
V=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_file_navigation.py')))
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_file_completion.py')))
F,G,R=V['F'],V['G'],V['R'];CANVAS,LIST=V['CANVAS'],V['LIST'];DIR=Path('/private/file-navigation')
def rect():return dict(left=10,top=10,right=210,bottom=30,width=200,height=20)
def textproof():return dict(ok=True,clip=rect(),visibleLines=1,lines=[dict(rect=rect(),fullVisible=True,hit=True,fragments=[dict(rect=rect(),hit=True)])])
def measured(text):return dict(text=text,rect=rect(),visible=True,hit=True,textProof=textproof())
def append(blocks,text):
 b=copy.deepcopy(blocks);b[0]['elements'].append(dict(type='rich_text_section',elements=[dict(type='text',text=text.lstrip('\n'))]));return b
def target_reads(id,start):
 raw=copy.deepcopy(next(r for r in F['RAW'] if r['id']==id))
 if id==CANVAS:
  url='https://files.slack.com/files-tmb/FLISTFILE1/navigation.html';raw.update(quip_thread_id='navigation-thread',editable=False,url_private=url)
  values=[('/api/files.info',dict(file=id),dict(ok=True,file=raw)),('/api/quip.getControllerInitData',{'_x_reason':'canvas-controller-init'},dict(ok=True,quip_user_id='navigation-quip-user',init_options=V['canvas_binaries']()[0])),('/canvas/-/load-data',{'request_binary':V['canvas_binaries']()[1]},{'base64':V['canvas_binaries']()[2]}),('/api/files.info',dict(file=id),dict(ok=True,file=raw)),(url,{},dict(html="<div class='quip-canvas-content'><h1>Alex Canvas</h1><p>Native target Canvas content</p><p>The original List draft remains saved.</p></div>"))]
 else:
  raw.update(access='read',is_locked=True,list_metadata=dict(schema=[dict(id='C_NAV_NAME',key='NAME',name='Item',type='text',is_primary_column=True)],views=[]))
  values=[('/api/files.info',dict(file=id),dict(ok=True,file=raw)),('/api/lists.records.list',dict(list_id=id,include_subtasks='true',archived='false',include_suggested='false',_x_reason='get-list-records'),dict(ok=True,list_id=id,records=[dict(id='R_NAV_TARGET',list_id=id,position=0,fields=[dict(key='NAME',text=V['CONTENT'][id])],navigation_opaque={'keep':'separate target row'})])),('/api/team.prefs.get',dict(target_team='T1',_x_reason='native-list-permission'),dict(ok=True,prefs={'allow_lists':'full'})),('/api/users.prefs.get',dict(prefs='task_lists,assigned_to_list',_x_reason='native-list-completion-permission'),dict(ok=True,prefs={'task_lists':'[]','assigned_to_list':''})),('/api/experiments.getByUser',F['observations'](False)['experiments.getByUser'][0],dict(ok=True,assignments={}))]
 return [dict(serial=start+i,target=id,method='GET' if path.startswith('https:') else 'POST',path=path,args=args,response=response)for i,(path,args,response)in enumerate(values)]
def sample(case='file-navigation',narrow=False):
 fixture,base=B['sample']('file-completion',narrow);template=base['stages'][2];source=template['model']['completionSource']
 for r in source['responses']:
  if r['disposition']=='held':r['disposition']='response'
 source.update(releases=[],held=0)
 initial=template['model'];seed=template['proof']['seed'];a=copy.deepcopy(template['proof']['expectedBlocks']);current=a
 received=dict(received=V['received'](),originalDocument=initial['initialDocument'],originalRecords=initial['initialRecords'],originalUnrelated=initial['initialUnrelated'],canonicalResponse=F['RESPONSE'])
 rows=[];clicks=[];targets=[];returns=0;scope=seed['scope'];storage=None;retirement=None
 ready=base['readiness'];account=initial['account']
 def inventory(blocks,rev):return dict(scope=scope,revision=rev,capture='00000000-0000-4000-8000-000000000001',blocks=copy.deepcopy(blocks),attempt=None)
 def ack(seq,inv):
  value=dict(ready,sequence=seq,action='hold' if seq%2 else 'release',ok=True,account=account,inventories=[copy.deepcopy(inv)],contentSha256='ab'*32)
  if seq%2==0:value['unchanged']=True
  return value
 for i,name in enumerate(V['STAGES'][case]):
  row=copy.deepcopy(template);row['stage']=name;row['capture']=str(DIR/f'{fixture}-{name}.png');m=row['model'];g=row['geometry']
  clickid=None;newtext=None
  if case=='file-navigation':
   if name=='canvas-open':clickid=CANVAS;newtext=V['CANVAS_EDIT'];targets.append(CANVAS)
   if name=='list-open':clickid=LIST;newtext=V['LIST_EDIT'];targets.append(LIST)
  else:
   if name=='composition-refused':clickid=CANVAS
   if name=='save-refused':
    storage={'hold':ack(1,inventory(current,2)),'release':None};clickid=CANVAS;newtext=V['CANVAS_EDIT']
   if name=='save-recovered':storage['release']=ack(2,storage['hold']['inventories'][0])
   if name=='retry-open':clickid=CANVAS;targets.append(CANVAS)
   if name=='source-refused':
    inv=inventory(current,3);retirement=dict(hold=ack(3,inv),release=ack(4,inv));clickid=CANVAS;newtext=V['RETIRE_EDIT']
  if newtext:current=append(current,newtext)
  if clickid:
   native=dict(blocks=copy.deepcopy(current),revision=10+len(clicks),composing=name=='composition-refused',generation=1+returns)
   clicks.append(dict(ok=True,id=clickid,kind='canvas' if clickid==CANVAS else 'file',ts=V['TS'],before=measured(F['LABELS'][clickid]),input=dict(ok=True,snapshot=copy.deepcopy(native)) if newtext else None,draft=native,defaultPrevented=False,control=dict(tag='BUTTON',type='button',href=None,disabled=False)))
  if 'return' in name:returns+=1
  nav=dict(enabled=True,received=V['received'](),originalDocument=initial['initialDocument'],originalRecords=initial['initialRecords'],originalUnrelated=initial['initialUnrelated'],targetRequests=[],returns=returns,networkCalls=0,httpCapability=False,listContent=V['CONTENT'][LIST],canvasContent=V['CONTENT'][CANVAS])
  for id in targets:nav['targetRequests'].extend(target_reads(id,len(nav['targetRequests'])+1))
  m['navigation']=nav;m['composeLedger'].update(prepared=[],posts=[],preflights=[],policies=[],messages=[],published=False,held=False,heldPost=False)
  readset=copy.deepcopy(initial['ledger']['requests'][:4]);m['ledger'].update(requests=readset*(returns+1),completed=4*(returns+1),cancelled=0,held=False)
  m['ledger']['requests']=[dict(copy.deepcopy(r),serial=j+1) for j,r in enumerate(m['ledger']['requests'])]
  m['comments']['page']['messages']=[V['received']() if x['ts']==V['TS'] else x for x in m['comments']['page']['messages']]
  inv=inventory(current,3+i);d=m['draftStore']['drafts'][0];d.update(blocks=copy.deepcopy(current),capture=inv['capture'],savedRevision=inv['revision'],attempt=None,error=None,busy=False,saving=False,loading=False,terminalPending=False)
  if name=='composition-refused':d['error']=V['COMPOSING']
  if name=='save-refused':d['error']=V['SAVE'];d['savedRevision']=2
  p=dict(seed=seed,received=received,expectedBlocks=copy.deepcopy(current),clicks=copy.deepcopy(clicks),journal=None if name=='save-refused' else inv,storage=copy.deepcopy(storage),runtimeForgotten='return' in name)
  opened=name in ('canvas-open','list-open','retry-open');id=LIST if name=='list-open' else CANVAS
  m['route']=dict(canvasActive=opened and id==CANVAS,listActive=not opened or id==LIST,canvasId=CANVAS if opened and id==CANVAS else None,listId=id if opened and id==LIST else V['READ']['LIST'])
  if opened:g=dict(ok=True,editorAbsent=True,kind='canvas' if id==CANVAS else 'file',title=measured(F['LABELS'][id]),content=measured(V['CONTENT'][id]),viewport=g['viewport'],noOverflow=True)
  else:
   g.update(ok=True,comments=True,native=dict(blocks=copy.deepcopy(current),composing=name=='composition-refused',generation=1+returns),draftText=textproof());g['editor'].update(blocks=copy.deepcopy(current),editable=True)
  if name=='source-refused':
   m['completionSource']['files']['source']=B['sourceproof'](True,True)
   p.update(retirementStorage=retirement,retirementPending=copy.deepcopy(m),retirementNative=dict(ok=True,wrapperInert=True,connected=True,native=dict(blocks=copy.deepcopy(current),composing=False)),sourceInvalidation=dict(id=CANVAS,before=B['sourceproof'](True),after=B['sourceproof'](True,True),eventRevisionBefore=0,eventRevisionAfter=0))
   p['retirementPending']['draftStore']['drafts'][0]['saving']=True
  row.update(proof=p,geometry=g,geometryAfter=copy.deepcopy(g));rows.append(row)
 result=dict(ok=True,case=case,fixture=fixture,readiness=ready,stages=rows,final=copy.deepcopy(rows[-1]['model']),initialRecords=base['initialRecords'],noLiveNetwork=True)
 return fixture,result

class NavigationTests(unittest.TestCase):
 def check(self,f,r):return V['validate'](f,r,DIR,png=False)
 def reject(self,fn,index=1,case='file-navigation'):
  f,r=sample(case);fn(r['stages'][index]);r['final']=copy.deepcopy(r['stages'][-1]['model'])
  with self.assertRaises((RuntimeError,KeyError,ValueError,TypeError,IndexError)):self.check(f,r)
 def test_all_cases_and_widths(self):
  for c in V['STAGES']:
   for narrow in (False,True):
    with self.subTest(case=c,narrow=narrow):self.check(*sample(c,narrow))
 def test_missing_stage(self):
  f,r=sample();r['stages'].pop()
  with self.assertRaises(RuntimeError):self.check(f,r)
 def test_no_href_navigation(self):self.reject(lambda r:r['proof']['clicks'][0]['control'].update(href='https://fixture.invalid'))
 def test_wrong_clicked_id(self):self.reject(lambda r:r['proof']['clicks'][0].update(id=LIST))
 def test_wrong_clicked_kind(self):self.reject(lambda r:r['proof']['clicks'][0].update(kind='file'))
 def test_missing_immediate_input(self):self.reject(lambda r:r['proof']['clicks'][0].update(input=None))
 def test_native_edit_not_same_click_snapshot(self):self.reject(lambda r:r['proof']['clicks'][0]['input']['snapshot'].update(blocks=[]))
 def test_wrong_canonical_title(self):self.reject(lambda r:r['geometry']['title'].update(text='Original Canvas fallback'))
 def test_wrong_body(self):self.reject(lambda r:r['geometry']['content'].update(text='Original List body'))
 def test_wrong_route(self):self.reject(lambda r:r['model']['route'].update(canvasId=LIST))
 def test_editor_still_mounted(self):self.reject(lambda r:r['geometry'].update(editorAbsent=False))
 def test_geometry_after_changed(self):self.reject(lambda r:r['geometryAfter']['title'].update(text='changed'))
 def test_hidden_text(self):
  def corrupt(r):r['geometry']['title']['textProof']['lines'][0]['fullVisible']=False;r['geometryAfter']=copy.deepcopy(r['geometry'])
  self.reject(corrupt)
 def test_occluded_text(self):
  def corrupt(r):r['geometry']['title']['textProof']['lines'][0]['fragments'][0]['hit']=False;r['geometryAfter']=copy.deepcopy(r['geometry'])
  self.reject(corrupt)
 def test_long_draft_clipped_lines_remain_explicit_but_canonical_title_must_fit(self):
  proof=textproof();off=dict(left=10,top=-30,right=210,bottom=-10,width=200,height=20)
  proof['lines'].append(dict(rect=off,fullVisible=False,hit=False,fragments=[dict(rect=copy.deepcopy(off),hit=False)]))
  V['text_proof'](proof,[400,300])
  with self.assertRaisesRegex(RuntimeError,'clipped or occluded'):V['text_proof'](proof,[400,300],True)
  proof['lines'][-1].update(fullVisible=True,hit=True);proof['visibleLines']=2
  with self.assertRaises(RuntimeError):V['text_proof'](proof,[400,300])
 def test_wrong_target_response(self):self.reject(lambda r:r['model']['navigation']['targetRequests'][0]['response']['file'].update(id=LIST))
 def test_canvas_binary_receipt_changed(self):self.reject(lambda r:r['model']['navigation']['targetRequests'][2]['response'].update(base64='AQ=='))
 def test_fresh_return_cannot_reuse_retired_editor(self):self.reject(lambda r:r['geometry']['native'].update(generation=r['proof']['clicks'][-1]['draft']['generation']),2)
 def test_extra_target_read(self):self.reject(lambda r:r['model']['navigation']['targetRequests'].append(copy.deepcopy(r['model']['navigation']['targetRequests'][0])))
 def test_received_raw_rewritten(self):self.reject(lambda r:r['model']['navigation']['received'].update(blocks=[]))
 def test_no_post(self):self.reject(lambda r:r['model']['composeLedger']['posts'].append({'method':'chat.postMessage'}))
 def test_disk_mismatch(self):self.reject(lambda r:r['proof']['journal'].update(blocks=[]))
 def test_original_opaque_style_lost(self):self.reject(lambda r:r['proof']['expectedBlocks'][0]['elements'][0]['elements'].pop(1))
 def test_reopen_must_forget_runtime(self):self.reject(lambda r:r['proof'].update(runtimeForgotten=False),2)
 def test_foreign_account(self):self.reject(lambda r:r['model'].update(account='00000000-0000-4000-8000-000000000099'))
 def test_lock_requires_real_unreleased_inventory(self):self.reject(lambda r:r['proof']['storage'].update(release={}),2,'file-navigation-custody')
 def test_mid_save_requires_pending(self):self.reject(lambda r:r['proof']['retirementPending']['draftStore']['drafts'][0].update(saving=False),6,'file-navigation-custody')
 def test_mid_save_requires_native_lease(self):self.reject(lambda r:r['proof']['retirementNative'].update(wrapperInert=False),6,'file-navigation-custody')
 def test_mid_save_retains_c(self):self.reject(lambda r:r['proof']['journal'].update(blocks=r['proof']['retirementStorage']['hold']['inventories'][0]['blocks']),6,'file-navigation-custody')
 def test_dispatch_and_exact_lifecycle(self):
  for c in V['STAGES']:
   f,r=sample(c);stages,validator=C['case_contract'](c);self.assertEqual(stages,V['STAGES'][c])
   edges=[dict(stage=s,edge=e)for s in stages for e in ('before','after')];packets={C['WAITING']:[r['readiness']],C['PREFIX']:[r],C['EDGE']:edges}
   native=SimpleNamespace(receipts=lambda key:packets[key])
   with patch.dict(C['validate_log'].__globals__,case_contract=lambda _: (stages,lambda *a,**k:r)):
    C['validate_log'](native,f,DIR,r['readiness'],None,c)
    packets[C['EDGE']]=edges[:-1]
    with self.assertRaisesRegex(RuntimeError,'Capture lifecycle'):C['validate_log'](native,f,DIR,r['readiness'],None,c)
 def test_actual_navigation_probe(self):
  script=r'''
import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';
const source=fs.readFileSync('src/desktop/slack_lists/check/compose_comments/file_navigation.js','utf8');
const rect=()=>({left:10,top:10,right:100,bottom:30,width:90,height:20});
let serial=1,composing=false,inert=false,clicked=null,hit=true,text=true;
const native=()=>({blocks:[{type:'text',text:'native '+serial}],revision:serial,composing});
const wrapper={get inert(){return inert;}};
const editor={isContentEditable:true,isConnected:true,scrollIntoView(){},focus(){},closest(){return wrapper;},dispatchEvent(e){composing=e.type==='compositionstart';}};
const token={dataset:{richFileId:'F1',richFileKind:'canvas'},tagName:'BUTTON',textContent:'Title',disabled:false,contains(){return false;},getAttribute(k){return k==='type'?'button':null;},getBoundingClientRect:rect,scrollIntoView(){},dispatchEvent(e){clicked=native();}};
const row={dataset:{listCommentTs:'1.2'},querySelector(){return token;}};
const document={getElementById(){return editor;},querySelectorAll(){return [row];},elementFromPoint(){return hit?token:null;}};
class Event {constructor(type,options){this.type=type;this.defaultPrevented=false;}}
const probe=vm.runInNewContext(source,{document,window:{superPlatinumRichEditor:{snapshot:native},__listCompose:{type(){serial++;return {ok:true,snapshot:native()};}}},CSS:{escape:s=>s},MouseEvent:Event,CompositionEvent:Event,innerWidth:400,innerHeight:300,getComputedStyle:()=>({display:'inline-flex',visibility:'visible'})});
let r=probe('click',{id:'F1',ts:'1.2',append:'C'},()=>({ok:text}));assert(r.ok);assert.equal(r.defaultPrevented,false);assert.equal(r.control.tag,'BUTTON');assert.equal(r.control.href,null);assert.equal(r.draft.revision,2);assert.equal(clicked.revision,2);assert.deepEqual(r.input.snapshot,r.draft);
assert.equal(probe('click',{id:'F1',ts:'other'},()=>({ok:true})).ok,false);
hit=false;assert.equal(probe('click',{id:'F1',ts:'1.2'},()=>({ok:true})).ok,false);hit=true;
text=false;assert.equal(probe('click',{id:'F1',ts:'1.2'},()=>({ok:text})).ok,false);text=true;
assert.equal(probe('pending',null,()=>({ok:true})).ok,false);inert=true;assert.equal(probe('pending',null,()=>({ok:true})).ok,true);composing=true;assert.equal(probe('pending',null,()=>({ok:true})).ok,false);
assert.equal(probe('unknown',null,()=>({ok:true})).ok,false);
console.log('8 actual navigation probe cases passed');
'''
  result=subprocess.run(['node','--input-type=module','-e',script],cwd=Path(__file__).resolve().parents[1],capture_output=True,text=True)
  self.assertEqual(result.returncode,0,result.stderr);self.assertIn('8 actual navigation probe cases passed',result.stdout)
 def test_return_readiness_requires_exact_new_visit_and_owned_mounted_row(self):
  script=r'''
import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';
const source=fs.readFileSync('src/desktop/slack_lists/check/compose_comments/file_navigation.js','utf8');
let hidden=false,owned=true,hit=true,cells=true,duplicate=false;
const cell={isConnected:true,closest(s){return s==='.slack-list-dialog'?(owned?dialog:{}):(hidden?{}:null);},scrollIntoView(){},getBoundingClientRect(){return {left:10,top:10,right:50,bottom:30,width:40,height:20};},contains(){return false;}};
const row={dataset:{rowId:'R_LIST_000'},querySelector(){return cells?cell:null;}};
const dialog={isConnected:true,dataset:{listId:'FLISTFILE2',dialogGeneration:'12',listKeyboardMounted:'true'},closest(){return null;},querySelectorAll(){return duplicate?[row,row]:[row];}};
let dialogs=[dialog];
const probe=vm.runInNewContext(source,{document:{querySelectorAll(){return dialogs;},elementFromPoint(){return hit?cell:null;}},getComputedStyle:()=>({display:'table-cell',visibility:'visible'})});
const ready=()=>probe('returned-ready',{id:'FCOMMENT01',generation:13,record:'R_LIST_000'},null).ok;
assert.equal(ready(),false); // Old target List is fully keyboard-mounted.
dialog.dataset.listId='FCOMMENT01';assert.equal(ready(),false); // Same ID, retired generation.
dialog.dataset.dialogGeneration='13';assert.equal(ready(),true);
dialog.dataset.listKeyboardMounted='false';assert.equal(ready(),false);dialog.dataset.listKeyboardMounted='true';
cells=false;assert.equal(ready(),false);cells=true;
duplicate=true;assert.equal(ready(),false);duplicate=false;
row.dataset.rowId='OTHER';assert.equal(ready(),false);row.dataset.rowId='R_LIST_000';
owned=false;assert.equal(ready(),false);owned=true;
hidden=true;assert.equal(ready(),false);hidden=false;
hit=false;assert.equal(ready(),false);hit=true;
dialogs=[dialog,dialog];assert.equal(ready(),false);dialogs=[dialog];
cell.isConnected=false;assert.equal(ready(),false);cell.isConnected=true;
assert.equal(ready(),true);
console.log('13 returned visit probe cases passed');
'''
  result=subprocess.run(['node','--input-type=module','-e',script],cwd=Path(__file__).resolve().parents[1],capture_output=True,text=True)
  self.assertEqual(result.returncode,0,result.stderr);self.assertIn('13 returned visit probe cases passed',result.stdout)
 def test_source_capture_waits_for_model_notice_native_blocks_and_generation(self):
  script=r'''
import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';
const source=fs.readFileSync('src/desktop/slack_lists/check/compose_comments/file_navigation.js','utf8');
let notices=[],visible=[],blocks=[{type:'text',text:'retained'}];
const editor={isContentEditable:true,scrollIntoView(){}};
const panel={dataset:{editorGeneration:'9'},querySelectorAll(){return notices.map(textContent=>({textContent}));}};
const probe=vm.runInNewContext(source,{document:{getElementById(id){return id==='slack-list-comment-draft'?editor:panel;}},window:{__listCompose:{snapshot(){return {ok:true,notices:visible};}},superPlatinumRichEditor:{snapshot(){return {blocks,composing:true};}}},innerWidth:400,innerHeight:300});
const expected={generation:9,blocks:[{type:'text',text:'retained'}],notices:['Finish composing. Your draft is retained.']};
const ready=()=>probe('source',expected,()=>({ok:true})).ok;
assert.equal(ready(),false); // Model error has not painted yet.
notices=[...expected.notices];assert.equal(ready(),false); // Notice exists but isn't visible.
visible=[...notices];assert.equal(ready(),true);
blocks=[{text:'retained',type:'text'}];assert.equal(ready(),true); // JSON key order isn't rich-buffer identity.
notices.push('stale status');assert.equal(ready(),false);notices.pop();
blocks=[];assert.equal(ready(),false);blocks=expected.blocks;
panel.dataset.editorGeneration='8';assert.equal(ready(),false);panel.dataset.editorGeneration='9';
expected.notices=[];assert.equal(ready(),false);notices=[];visible=[];assert.equal(ready(),true);
console.log('9 source paint probe cases passed');
'''
  result=subprocess.run(['node','--input-type=module','-e',script],cwd=Path(__file__).resolve().parents[1],capture_output=True,text=True)
  self.assertEqual(result.returncode,0,result.stderr);self.assertIn('9 source paint probe cases passed',result.stdout)

C=V['C']
if __name__=='__main__':unittest.main()
