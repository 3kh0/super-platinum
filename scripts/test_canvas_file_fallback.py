"""Portable independent receipt adversaries; no native execution claim."""
import copy
import importlib.util
from pathlib import Path
from unittest.mock import patch
import unittest
import canvas_file_fallback as O
from test_canvas_special_roots import rect, read

def sample(viewport=(668,380)):
 def visible(text):return {'text':text,'rect':rect(),'inView':True,'hit':True}
 requests=[read('replies',O.ROOT) if n in (2,3) else read(cursor='fallback-controls' if n==5 else None) for n in range(8)]
 stages=[]
 for i,(name,count) in enumerate(zip(O.STAGES,O.READS)):
  rows=[]
  for raw in O.originals(i):
   ts=raw['ts'];text=O.TEXT[ts];marks=[]
   if ts==O.ROOT:marks=[{'text':text,'className':' bold','weight':'700','style':'normal','decoration':'none'},{'text':text,'className':'rich-underline','weight':'400','style':'normal','decoration':'underline'}]
   if ts==O.CONTROLS[1]:marks=[{'text':text,'className':' italic','weight':'400','style':'italic','decoration':'none'}]
   if ts==O.CONTROLS[2]:marks=[{'text':text,'className':' bold','weight':'700','style':'normal','decoration':'none'}]
   body=visible(text)
   if text=='':body.update(rect=rect(height=0),inView=False,hit=False)
   rows.append({'ts':ts,'text':text,'body':body,'marks':marks,'links':[],'images':[],'attachments':[], 'threadButton':None if i in (1,2) else dict(visible('Open thread'),disabled=False)})
  target=O.target(i);selected=next((r for r in rows if r['ts']==target),None);empty=selected and selected['text']==''
  b={'ok':True,'targetTs':target,'targetKind':'alert' if i==9 else 'thread-button' if empty else 'body','target':visible(O.ERROR) if i==9 else selected['threadButton'] if empty else selected['body'],'scroll':rect(0,0,600,300),'rows':rows,'alerts':[O.ERROR] if i==9 else []}
  layout={'ok':True,'viewport':list(viewport),'focused':True,'noOverflow':True,'dialog':rect(0,0,650,350),'panel':rect(0,0,620,320),'scrollHeight':300,'controls':[{'label':label,'rect':rect(),'hit':True} for label in ('Back to canvas','All comments','Refresh comments')],'bodyProof':b}
  retained={'query':{'kind':'thread' if i in (1,2) else 'stream','rootTs':O.ROOT if i in (1,2) else None},'serial':count,'ownerCurrent':True,'loading':False,'error':O.ERROR if i==9 else None,'root':O.primary() if i in (1,2) else None,'rows':[{'message':raw,'matchesOriginal':True} for raw in O.originals(i)]}
  proof={'primary':O.primary(),'controls':O.controls(),'requests':requests[:count],'responses':O.responses()[:count],'media':[{'url':u,'registered':0} for u in O.URLS],'retained':retained}
  stages.append({'ok':True,'ordinal':i,'stage':name,'noSession':True,'noTransport':True,'layout':layout,'proof':proof})
 return copy.deepcopy({'stages':stages,'proof':stages[-1]['proof'],'layout':stages[-1]['layout']})

class Tests(unittest.TestCase):
 def refuse(self,change):
  r=sample();change(r)
  with self.assertRaises((RuntimeError,ValueError)):O.validate(r)
 def test_complete_normal_and_narrow(self):O.validate(sample());O.validate(sample((1280,800)))
 def test_unicode_and_literal_vectors_are_real(self):
  self.assertIn(chr(0x1f98a),O.TEXT[O.ROOT]);self.assertIn(chr(0x4e2d),O.TEXT[O.ROOT]);self.assertEqual(O.TEXT[O.CONTROLS[1]][-1],' ');self.assertTrue(O.TEXT[O.CONTROLS[3]].startswith('*'))
 def test_empty_bodies_use_real_visible_action(self):
  O.validate(sample())
  for i,row in [(3,0),(8,5)]:
   self.refuse(lambda r:r['stages'][i]['layout']['bodyProof'].update(targetKind='body'))
   self.refuse(lambda r:r['stages'][i]['layout']['bodyProof']['rows'][row]['threadButton'].update(hit=False))
 def test_nonempty_blocks_never_fallback(self):self.refuse(lambda r:r['stages'][3]['layout']['bodyProof']['rows'][0].update(text='DECOY unsupported fallback'))
 def test_malformed_refusal_never_stringifies(self):self.refuse(lambda r:r['stages'][8]['layout']['bodyProof']['rows'][5].update(text='[object Object]'))
 def test_no_generic_file_or_attachment_output(self):
  for key,value in [('text','Original block & '+chr(0x4e2d)+chr(0x1f98a)+'DECOY file title'),('links',[{'href':O.URLS[0]}]),('images',[{'src':O.URLS[0]}]),('attachments',['DECOY attachment'])]:self.refuse(lambda r:r['stages'][0]['layout']['bodyProof']['rows'][0].update({key:value}))
 def test_each_original_media_url_must_remain_unregistered(self):
  for n in range(7):self.refuse(lambda r:r['stages'][2]['proof']['media'][n].update(registered=1))
  for x in [False,None,'0']:self.refuse(lambda r:r['stages'][0]['proof']['media'][0].update(registered=x))
 def test_original_two_file_order_and_raw_body_retained(self):
  self.refuse(lambda r:r['stages'][1]['proof']['retained']['rows'][0]['message']['files'].reverse())
  for k,v in [('files',[]),('attachments',[]),('blocks',[]),('text','painted'),('ts',O.REPLY)]:self.refuse(lambda r:r['stages'][1]['proof']['retained']['rows'][0]['message'].update({k:v}))
 def test_raw_unavailable_and_visibility_not_normalized(self):
  self.refuse(lambda r:r['stages'][8]['proof']['retained']['rows'][5]['message'].update(markdown_text=''))
  self.refuse(lambda r:r['stages'][5]['proof']['controls'][2]['document_comment'].pop('is_visible'))
  self.refuse(lambda r:r['stages'][4]['proof']['controls'][1]['document_comment'].update(is_visible=False))
 def test_block_marks_are_painted(self):
  for n in [0,1]:self.refuse(lambda r:r['stages'][0]['layout']['bodyProof']['rows'][0]['marks'].pop(n))
  self.refuse(lambda r:r['stages'][0]['layout']['bodyProof']['rows'][0]['marks'][0].update(weight='400'))
  self.refuse(lambda r:r['stages'][0]['layout']['bodyProof']['rows'][0]['marks'][1].update(decoration='none'))
 def test_formatted_text_and_markdown_marks(self):
  self.refuse(lambda r:r['stages'][4]['layout']['bodyProof']['rows'][1]['marks'][0].update(style='normal'))
  self.refuse(lambda r:r['stages'][5]['layout']['bodyProof']['rows'][2]['marks'][0].update(weight='400'))
 def test_literal_formatting_markers_stay_literal(self):
  self.refuse(lambda r:r['stages'][6]['layout']['bodyProof']['rows'][3].update(text=O.TEXT[O.CONTROLS[3]].strip('*')))
  self.refuse(lambda r:r['stages'][6]['layout']['bodyProof']['rows'][3]['marks'].append({'text':'Literal','className':'bold'}))
 def test_hidden_placeholder_unchanged(self):self.refuse(lambda r:r['stages'][7]['layout']['bodyProof']['rows'][4].update(text='DECOY hidden text'))
 def test_real_failure_and_explicit_recovery(self):
  self.refuse(lambda r:r['stages'][9]['proof']['responses'].__setitem__(6,{'ok':True,'messages':[]}))
  self.refuse(lambda r:r['stages'][9]['layout']['bodyProof'].update(alerts=[]))
 def test_original_owner_query_root_and_serial(self):
  self.refuse(lambda r:r['stages'][1]['proof']['retained'].update(root=None))
  for k,v in [('serial',True),('serial',99),('ownerCurrent',False),('loading',True)]:self.refuse(lambda r:r['stages'][1]['proof']['retained'].update({k:v}))
  self.refuse(lambda r:r['stages'][1]['proof']['retained']['query'].update(rootTs=O.REPLY))
 def test_request_target_and_count(self):
  self.refuse(lambda r:r['stages'][2]['proof']['requests'][3]['fields'].append(['token','unexpected']))
  self.refuse(lambda r:r['stages'][0]['proof']['requests'].pop())
 def test_target_geometry_independently_bounded(self):
  self.refuse(lambda r:r['stages'][4]['layout']['bodyProof']['target'].update(hit=False))
  self.refuse(lambda r:r['stages'][4]['layout']['bodyProof']['target'].update(rect=rect(y=310)))
  self.refuse(lambda r:r['stages'][4]['layout']['controls'][0].update(hit=False))
 def test_no_stage_or_terminal_substitution(self):
  self.refuse(lambda r:r['stages'].pop());self.refuse(lambda r:r.update(proof={}))
 def test_captured_proof_immutable(self):
  path=Path(__file__).with_name('check-canvas-file-fallback-native.py');spec=importlib.util.spec_from_file_location('fallback_runner',path);runner=importlib.util.module_from_spec(spec);spec.loader.exec_module(runner)
  r=sample();stages=copy.deepcopy(r['stages'])
  for s in stages:s['screenshot']='original.png'
  with patch.object(runner,'_base_validate_closed'):
   runner.validate_closed(runner.SPEC,'canvas-file-fallback',None,stages,r)
   stages[0]['proof']['primary']['files']=[]
   with self.assertRaises(RuntimeError):runner.validate_closed(runner.SPEC,'canvas-file-fallback',None,stages,r)
if __name__=='__main__':unittest.main()
