"""Portable adversarial receipts; these tests do not execute the native UI."""
import copy
import unittest
import canvas_canonical_file_roots as O
from test_canvas_special_roots import rect,read
def visible(text):return {'text':text,'rect':rect(),'inView':True,'hit':True}
def retained(rows,thread,serial,error=None):
 return {'ownerCurrent':True,'loading':False,'serial':serial,'error':error,
  'query':{'kind':'thread' if thread else 'stream','rootTs':O.ROOT if thread else None},
  'rows':[{'message':copy.deepcopy(raw),'matchesOriginal':True} for raw in rows],
  'root':{'message':copy.deepcopy(rows[0]),'matchesOriginal':True} if rows and thread else None,
  'pendingFile':False,'navigationError':None}
def sample(viewport=(668,380)):
 stages=[];requests=[read('replies',O.ROOT) if n>=2 else read() for n in range(9)]
 for i,(name,count) in enumerate(zip(O.STAGES,O.READS)):
  raw=O.originals(i);v=retained(raw,i>=5,count,O.ERROR if i==9 else None)
  source=O.roots(O.modes(i))[0]['files'][-1]
  effective=copy.deepcopy(source)
  v['catalog']={'serial':count,'rows':[{'id':O.IDS[0],'source':source,'effective':effective,'current':i!=11,
   'link':None if i in (7,8) else {'name':source['name'],'url':source['permalink']}}],
   'origins':[{'ts':r['ts'],'firstId':r['files'][0]['id'],'blockId':'original-'+r['ts']} for r in raw if r['ts'] not in (O.REPLY['ts'],O.ROOTS[3])]}
  p={'requests':copy.deepcopy(requests[:count]),'responses':O.responses()[:count],'retained':v,'originals':copy.deepcopy(raw),
   'resolver':{'requests':[],'responses':[],'settlements':[],'releases':[],'held':None},
   'media':[{'url':u,'registered':0} for u in O.URLS]}
  rows=[]
  for r in raw:
   text=O.expected_text(r,i);link=O.expected_link(r['ts'],i)
   marks=[{'className':'bold','text':text,'weight':'700'}] if not link and r['ts'] in (O.ROOT,O.ROOTS[4]) else []
   rows.append({'ts':r['ts'],'text':text,'body':visible(text),'threadButton':dict(visible('Open thread'),disabled=False) if i<5 else None,
    'links':[dict(visible(link['name']),**link,href=link['url'])] if link else [],'marks':marks,'images':[],'attachments':[]})
  target=O.ROOTS[i] if i<5 else None if i==9 else O.ROOT
  body={'ok':True,'scroll':rect(0,0,600,300),'targetTs':target,'targetKind':'alert' if i==9 else 'body',
   'target':visible(O.ERROR) if i==9 else next(r['body'] for r in rows if r['ts']==target),'rows':rows,'alerts':[O.ERROR] if i==9 else []}
  layout={'ok':True,'viewport':list(viewport),'focused':True,'noOverflow':True,'dialog':rect(0,0,650,350),'panel':rect(0,0,620,320),
   'controls':[{'label':label,'rect':rect(),'hit':True} for label in ('Back to canvas','All threads' if i>=5 else 'Most recentUnreadResolved','Refresh comments')],'bodyProof':body}
  stages.append({'ok':True,'noSession':True,'noTransport':True,'ordinal':i,'stage':name,'proof':p,'layout':layout})
 return copy.deepcopy({'stages':stages,'proof':stages[-1]['proof'],'layout':stages[-1]['layout'],
  'staleCallback':{'originalCurrent':False,'resolverUnchanged':True}})
class Canonical(unittest.TestCase):
 def refuse(self,change):
  r=sample();change(r)
  with self.assertRaises((RuntimeError,KeyError,TypeError)):O.validate(r)
 def test_complete_both_viewports(self):
  O.validate(sample());O.validate(sample((1088,728)))
 def test_absent_reply_blocks_may_have_typed_empty_default(self):
  r=sample()
  for s in r['stages']:
   for row in s['proof']['retained']['rows']:
    if row['message']['ts']==O.REPLY['ts']:row['message']['blocks']=[]
  O.validate(r)
 def test_supplied_body_and_explicit_null_remain_exact(self):
  self.refuse(lambda r:r['stages'][0]['proof']['retained']['rows'][0]['message'].update(blocks=[]))
  raw={'ts':'original','blocks':None,'document_comment':{'is_visible':None}}
  self.assertFalse(O.retained_fields({'ts':'original','blocks':[],'document_comment':{'is_visible':None}},raw))
  self.assertFalse(O.retained_fields({'ts':'original','blocks':None,'document_comment':{'is_visible':False}},raw))
 def test_exact_nested_reply_controls_are_bounded(self):
  r=sample()
  for s in r['stages'][5:]:s['layout']['controls'].extend([{'label':label,'rect':rect(),'hit':True} for label in ('Send reply','Earlier local attempts')])
  O.validate(r)
  bad=copy.deepcopy(r);bad['stages'][5]['layout']['controls'][3]['label']='Other action'
  with self.assertRaises(RuntimeError):O.validate(bad)
  r['stages'][5]['layout']['controls'][4]['rect']=rect(y=900)
  with self.assertRaises(RuntimeError):O.validate(r)
 def test_scoped_read_toolbar_admits_exactly_three_controls(self):
  r=sample()
  for s in r['stages']:s['layout']['controlScope']='comment-navigation'
  O.validate(r)
  r['stages'][5]['layout']['controls'].extend([{'label':label,'rect':rect(),'hit':True} for label in ('Send reply','Earlier local attempts')])
  with self.assertRaises(RuntimeError):O.validate(r)
 def test_canonical_name_not_title_or_embedded(self):
  for text in ('DIFFERENT title FCANON001','EMBEDDED first FCANON001','Hosted fallback'):
   self.refuse(lambda r:r['stages'][0]['layout']['bodyProof']['rows'][0].update(text=text))
 def test_exact_url_and_identity(self):
  for key,value in [('href','https://example.test/evil'),('id',O.IDS[1]),('root',O.ROOTS[1]),('name','title')]:
   self.refuse(lambda r:r['stages'][0]['layout']['bodyProof']['rows'][0]['links'][0].update({key:value}))
 def test_no_second_file_fallback(self):
  self.refuse(lambda r:r['stages'][4]['layout']['bodyProof']['rows'][4].update(links=[{'name':'SECOND MUST NOT WIN'}]))
 def test_hidden_never_link(self):
  self.refuse(lambda r:r['stages'][3]['layout']['bodyProof']['rows'][3].update(text='SECOND MUST NOT WIN'))
 def test_omitted_permalink_must_not_inherit(self):
  self.refuse(lambda r:r['stages'][7]['proof']['retained']['catalog']['rows'][0]['effective'].update(permalink='https://example.test/old'))
 def test_deleted_replaces_not_merges(self):
  self.refuse(lambda r:r['stages'][8]['proof']['retained']['catalog']['rows'][0]['effective'].update(name='Old title'))
 def test_actual_event_retirement(self):
  self.refuse(lambda r:r['stages'][11]['proof']['retained']['catalog']['rows'][0].update(current=True))
  self.refuse(lambda r:r['staleCallback'].update(originalCurrent=True))
 def test_each_raw_media_registration(self):
  for i in range(3):self.refuse(lambda r:r['stages'][0]['proof']['media'][i].update(registered=1))
 def test_original_order_and_opaque_fields(self):
  self.refuse(lambda r:r['stages'][0]['proof']['retained']['rows'][0]['message']['files'].reverse())
  self.refuse(lambda r:r['stages'][0]['proof']['retained']['rows'][0]['message']['document_comment'].pop('fixture_opaque'))
 def test_source_not_typed_defaults(self):
  self.refuse(lambda r:r['stages'][6]['proof']['retained']['catalog']['rows'][0]['source'].update(mode='hosted'))
 def test_wire_no_extra_reads_or_mutation(self):
  self.refuse(lambda r:r['stages'][5]['proof']['requests'][2]['fields'].append(['token','forbidden']))
  self.refuse(lambda r:r['stages'][0]['proof']['resolver']['requests'].append({}))
 def test_geometry_and_error(self):
  self.refuse(lambda r:r['stages'][0]['layout']['bodyProof']['target'].update(hit=False))
  self.refuse(lambda r:r['stages'][4]['layout']['bodyProof']['rows'][4]['threadButton'].update(rect=rect(y=500)))
  self.refuse(lambda r:r['stages'][9]['layout']['bodyProof'].update(alerts=[]))
 def test_terminal_and_retained_source(self):
  self.refuse(lambda r:r.update(proof={}))
  self.refuse(lambda r:r['stages'][5]['proof']['retained'].update(ownerCurrent=False))
if __name__=='__main__':unittest.main()
