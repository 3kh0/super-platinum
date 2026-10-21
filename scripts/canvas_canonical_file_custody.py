"""Fresh complete target, original callback guards and exact durable reply oracle."""
import canvas_canonical_file_roots as D
from urllib.parse import urlsplit
from canvas_special_roots import require,same,bounded,shown
STAGES=('thread','draft','composition-refused','capture-failed','capture-recovered','held','source-retired','viewer','reopened')
READS=(2,2,2,2,2,2,3,3,5)
RESOLVES=(0,0,1,1,1,2,2,3,3)
PREVIEW='https://example.test/canonical/preview.svg'
NOTICE='This file could not be opened. Your reply draft is retained here.'
def blocks():
 return [
  {'type':'rich_text','block_id':'canonical-draft','elements':[{'type':'rich_text_section','elements':[
   {'type':'text','text':'Keep exact \u4e2d\U0001f98a & ','style':{'bold':True}},
   {'type':'link','url':'https://example.test/draft','text':'draft link','style':{'italic':True}},
   {'type':'text','text':' e\u0301.'}]}]},
  {'type':'rich_text','block_id':'opaque-draft','opaque':{'keep':[0,False,None,'\U0001f680']},'elements':[
   {'type':'rich_text_section','elements':[{'type':'text','text':'Opaque attributes retained.','fixture_opaque':{'zero':0,'enabled':False}}]}]}]
def edited():
 b=blocks()
 # Browser inserts at the final text leaf inside the second supported rich block.
 b[1]['elements'][0]['elements'][0]['text']+=' Canonical tail \u4e2d\U0001f98a'
 return b
def responses():
 return [D.envelope(),D.envelope(thread=True),D.envelope('renamed',True),D.envelope('renamed'),D.envelope('renamed',True)]
def resolver(r,index):
 count=RESOLVES[index]
 require(isinstance(r,dict) and len(r.get('requests',[]))==count and len(r.get('responses',[]))==count,'Fresh target dispatch count differs')
 for req in r['requests']:
  url=urlsplit(req.get('url',''))
  require(req.get('method')=='POST' and url.scheme=='https' and url.netloc=='fixture.slack.com' and url.path=='/api/files.info' and not url.fragment,'Resolver factory changed')
  require(req.get('fields')==[['file',D.IDS[0]]],'Resolver original ID/shape differs')
 for raw in r['responses']:
  f=D.file(D.IDS[0]);f.update(title='Fresh hosted preview',mimetype='image/svg+xml',url_private=PREVIEW);f.pop('thumb_360')
  require(same(raw,{'ok':True,'file':f}),'Fresh complete target envelope changed')
 expected=[{'serial':1,'delivered':True}] if count else []
 if index>=6:expected.append({'serial':2,'delivered':True})
 if index>=7:expected.append({'serial':3,'delivered':True})
 require(same(r.get('settlements'),expected),'Held result delivery/settlement changed')
 require(r.get('held')==(2 if index==5 else None),'Held original resolver phase changed')
 require(same(r.get('releases'),[{'serial':2,'delivered':True}] if index>=6 else []),'Retired result was not actually delivered')
def draft(reply,index):
 require(isinstance(reply,dict),'Independent reply inventory missing')
 runtime=reply.get('runtime');journal=reply.get('journal')
 require(isinstance(runtime,dict) and isinstance(journal,dict) and runtime.get('loading') is False and runtime.get('saving') is False and runtime.get('sending') is False,'Reply remained busy')
 require(journal.get('attempt') is None and journal.get('history')==[],'Navigation wrote a comment attempt')
 drafts=journal.get('drafts');require(isinstance(drafts,list) and len(drafts)==1,'Original rich draft missing/duplicated')
 expected=blocks() if index==0 else edited()
 require(same(runtime.get('blocks'),expected) and same(drafts[0].get('blocks'),expected),'Durable exact rich draft changed')
 require(drafts[0].get('root_ts',D.ROOT)==D.ROOT,'Reply journal root changed')
 return expected
def geometry(v,index,viewport,expected):
 def rect(r):
  require(isinstance(r,dict) and all(type(r.get(k)) in (int,float) for k in ('x','y','width','height')),'Native rectangle absent')
  return {**r,'right':r['x']+r['width'],'bottom':r['y']+r['height']}
 def visible(p,clip=None):
  require(isinstance(p,dict) and p.get('inView') is True and p.get('hit') is True and bounded(rect(p.get('rect')),viewport),'Native actual target clipped')
  if clip is not None:shown({**p,'rect':rect(p['rect'])},viewport,rect(clip))
 require(v.get('ok') is True and v.get('viewport')==viewport and v.get('inScroll') is True,'Native target unavailable')
 target=v.get('target');visible(target,None if index==7 else v.get('scroll'))
 require(v.get('targetKind')==('viewer-title' if index==7 else 'retry' if index==3 else 'link' if index==5 else 'editor-line'),'Capture target changed')
 if index==7:
  viewer=v.get('viewer');require(isinstance(viewer,dict) and viewer.get('title')=='Fresh hosted preview','Fresh native viewer target wrong')
  visible(viewer.get('close'))
  image=viewer.get('image');require(image.get('ready') is True and isinstance(image.get('src'),str) and ('super-platinum-media' in image['src']),'Native local preview missing')
  require(v.get('native') is None,'Committed old editor still mounted')
 else:
  require(v.get('viewer') is None,'Refused navigation opened a viewer')
  n=v.get('native');require(isinstance(n,dict) and same(n.get('blocks'),expected),'Native blocks differ from independent expected')
  require(n.get('composing') is (index==2),'Composition state differs')
  if index in range(1,6):require(n.get('sameNode') is True and n.get('sameMount') is True and n.get('sameBlocks') is True,'Original same-mounted reply lost')
  if index==8:require(n.get('sameNode') is False and n.get('sameBlocks') is True,'Reopened fresh editor did not restore durable reply')
 if index>=3:require(v.get('fault')=={'count':1,'restored':True},'Snapshot fault not exact one-shot restored API')
 if index==3:
  retry=v.get('retry');require(isinstance(retry,dict) and retry.get('disabled') is False,'Native capture recovery action absent')
  visible(retry,v['scroll'])
 if index==2:require(NOTICE in v.get('alerts',[]),'Composition refusal lacks scoped guidance')
def validate(r):
 stages=r.get('stages');require(isinstance(stages,list) and len(stages)==len(STAGES),'Custody stage inventory changed')
 viewport=r.get('layout',{}).get('viewport')
 for i,(s,name,count) in enumerate(zip(stages,STAGES,READS)):
  require(s.get('stage')==name and type(s.get('ordinal')) is int and s['ordinal']==i and all(s.get(k) is True for k in ('ok','noSession','noTransport')),'Custody order/isolation changed')
  p=s['proof'];mode='renamed' if i>=6 else 'original';rows=[D.roots(mode)[0],D.REPLY]
  D.wire_proof(p,responses()[:count],{1,2,4},rows,True,visible=i!=7)
  resolver(p.get('resolver'),i)
  require(p['retained'].get('pendingFile') is True if i==5 else True,'Held duplicate click did not remain pending')
  expected=draft(s.get('reply'),i);geometry(s.get('layout'),i,viewport,expected)
 require(same(r.get('proof'),stages[-1]['proof']) and same(r.get('layout'),stages[-1]['layout']),'Terminal replaced captured original proof')
