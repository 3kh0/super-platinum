"""Independent native/disk/response receipt adversaries, no native execution."""
import copy
import unittest
import canvas_canonical_file_custody as O
from test_canvas_canonical_file_roots import rect,read,retained,visible

def sample(viewport=(668,380)):
 stages=[];requests=[read('replies',O.D.ROOT) if i in (1,2,4) else read() for i in range(5)]
 for i,(name,count,n) in enumerate(zip(O.STAGES,O.READS,O.RESOLVES)):
  rows=[O.D.roots('renamed' if i>=6 else 'original')[0],O.D.REPLY]
  v=retained(rows,True,count);v['pendingFile']=i==5
  target=O.D.file(O.D.IDS[0]);target.update(title='Fresh hosted preview',mimetype='image/svg+xml',url_private=O.PREVIEW);target.pop('thumb_360')
  settlements=[{'serial':j,'delivered':True} for j in range(1,n+1) if not(i==5 and j==2)]
  resolver={'requests':[{'method':'POST','url':'https://fixture.slack.com/api/files.info','fields':[['file',O.D.IDS[0]]]} for _ in range(n)],
   'responses':[{'ok':True,'file':target} for _ in range(n)],'settlements':settlements,'held':2 if i==5 else None,
   'releases':[{'serial':2,'delivered':True}] if i>=6 else []}
  proof={'requests':requests[:count],'responses':O.responses()[:count],'retained':v,'originals':rows,
   'resolver':resolver,'media':[{'url':url,'registered':0} for url in O.D.URLS]}
  b=O.blocks() if i==0 else O.edited()
  reply={'runtime':{'blocks':b,'loading':False,'saving':False,'sending':False},
   'journal':{'drafts':[{'blocks':b,'id':'kept'}],'attempt':None,'history':[]}}
  native={'blocks':b,'composing':i==2,'sameNode':i in range(1,6),'sameMount':i in range(1,6),'sameBlocks':i!=0}
  viewer={'title':'Fresh hosted preview','close':visible('Close'),'image':{'ready':True,'src':'http://super-platinum-media.localhost/opaque'}} if i==7 else None
  layout={'ok':True,'viewport':list(viewport),'inScroll':True,'scroll':rect(0,0,600,300),
   'target':visible('Fresh hosted preview' if i==7 else 'original target'),
   'targetKind':'viewer-title' if i==7 else 'retry' if i==3 else 'link' if i==5 else 'editor-line',
   'native':None if i==7 else native,'viewer':viewer,'fault':{'count':1,'restored':True} if i>=3 else None,
   'alerts':[O.NOTICE] if i==2 else [],'retry':dict(visible('Retry local save'),disabled=False) if i==3 else None}
  stages.append({'ok':True,'noSession':True,'noTransport':True,'stage':name,'ordinal':i,'proof':proof,'reply':reply,'layout':layout})
 return copy.deepcopy({'stages':stages,'proof':stages[-1]['proof'],'layout':stages[-1]['layout']})
class Custody(unittest.TestCase):
 def refuse(self,change):
  r=sample();change(r)
  with self.assertRaises((RuntimeError,KeyError,TypeError)):O.validate(r)
 def test_complete_both_widths(self):
  O.validate(sample());O.validate(sample((1088,728)))
 def test_seed_uses_reply_editor_supported_marks(self):
  b=O.blocks()
  self.assertEqual(b[0]['elements'][0]['elements'][0]['style'],{'bold':True})
  self.assertEqual(b[0]['elements'][0]['elements'][1]['style'],{'italic':True})
  self.assertEqual(b[1]['opaque']['keep'],[0,False,None,'\U0001f680'])
 def test_original_rich_styles_and_opaque_attributes_preserved(self):
  for fn in (lambda b:b[0]['elements'][0]['elements'][0].pop('style'),
             lambda b:b[1].pop('opaque'),
             lambda b:b[1]['elements'][0]['elements'][0].pop('fixture_opaque')):
   self.refuse(lambda r:fn(r['stages'][8]['reply']['journal']['drafts'][0]['blocks']))
 def test_native_and_sqlite_independent_equality(self):
  self.refuse(lambda r:r['stages'][4]['layout']['native']['blocks'][1]['elements'][0]['elements'][0].update(text='changed'))
  self.refuse(lambda r:r['stages'][8]['reply']['journal']['drafts'][0].update(blocks=O.blocks()))
 def test_original_node_retained_before_departure_new_node_after(self):
  self.refuse(lambda r:r['stages'][2]['layout']['native'].update(sameNode=False))
  self.refuse(lambda r:r['stages'][8]['layout']['native'].update(sameNode=True))
 def test_composition_refusal_visible_and_no_viewer(self):
  self.refuse(lambda r:r['stages'][2]['layout'].update(alerts=[]))
  self.refuse(lambda r:r['stages'][2]['layout'].update(viewer={'title':'opened'}))
 def test_one_shot_capture_fault_and_retry_visible(self):
  self.refuse(lambda r:r['stages'][3]['layout']['fault'].update(count=2))
  self.refuse(lambda r:r['stages'][3]['layout']['fault'].update(restored=False))
  self.refuse(lambda r:r['stages'][3]['layout']['retry'].update(disabled=True))
 def test_held_duplicate_no_second_dispatch(self):
  self.refuse(lambda r:r['stages'][5]['proof']['resolver']['requests'].append({}))
  self.refuse(lambda r:r['stages'][5]['proof']['retained'].update(pendingFile=False))
 def test_actual_delivered_stale_result_not_cancellation_claim(self):
  self.refuse(lambda r:r['stages'][6]['proof']['resolver']['releases'][0].update(delivered=False))
  self.refuse(lambda r:r['stages'][6]['proof']['resolver']['settlements'].pop())
 def test_fresh_target_id_and_source_envelope(self):
  self.refuse(lambda r:r['stages'][7]['proof']['resolver']['requests'][2].update(fields=[['file',O.D.IDS[1]]]))
  self.refuse(lambda r:r['stages'][7]['proof']['resolver']['responses'][2]['file'].update(url_private='https://example.test/foreign'))
 def test_local_native_viewer_not_raw_url(self):
  self.refuse(lambda r:r['stages'][7]['layout']['viewer']['image'].update(src=O.PREVIEW))
  self.refuse(lambda r:r['stages'][7]['layout']['viewer'].update(title='EMBEDDED first'))
 def test_no_comment_write_or_attempt(self):
  self.refuse(lambda r:r['stages'][8]['reply']['journal'].update(attempt={'pending':True}))
  self.refuse(lambda r:r['stages'][8]['reply']['journal']['history'].append({}))
 def test_original_raw_root_and_no_discarded_media(self):
  self.refuse(lambda r:r['stages'][6]['proof']['retained']['root']['message']['files'].reverse())
  self.refuse(lambda r:r['stages'][7]['proof']['media'][1].update(registered=1))
 def test_geometry_actual_bounds_and_clip(self):
  self.refuse(lambda r:r['stages'][7]['layout']['viewer']['close'].update(rect=rect(y=800)))
  self.refuse(lambda r:r['stages'][3]['layout']['target'].update(hit=False))
  self.refuse(lambda r:r['stages'][5]['layout'].update(scroll=rect(0,100,600,100)))
 def test_stage_and_terminal_immutability(self):
  self.refuse(lambda r:r['stages'].pop())
  self.refuse(lambda r:r.update(proof={}))
if __name__=='__main__':unittest.main()
