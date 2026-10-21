"""Independent original-wire and canonical display oracle; never navigation authority."""
from canvas_special_roots import require, same, retained_fields, request, bounded, shown, root

STAGES = ('hosted','canvas','list','hidden','blocked','thread','renamed','no-permalink',
          'deleted','error','recovered','event-invalidated','event-restored')
READS = (2,2,2,2,2,3,4,5,6,7,8,8,9)
ROOT='1719800000.000100'
ROOTS=(ROOT,'1719800020.000100','1719800030.000100','1719800040.000100','1719800050.000100')
IDS=('FCANON001','FCANON002','FCANON003','FCANON004','FCANON005')
NAMES=('Hosted canonical & \u4e2d\U0001f98a','Canvas canonical name','List canonical name','SECOND MUST NOT WIN')
URLS=tuple('https://example.test/canonical/'+n for n in ('discarded-file.png','discarded-thumb.png','discarded-attachment.png'))
ERROR='Comments could not be loaded. Your canvas is unchanged. Retry when ready.'
REPLY={'type':'message','ts':'1719800001.000100','thread_ts':ROOT,'user':'U2','text':'Ordinary reply remains unchanged.'}

def file(id):
 if id==IDS[3]:return {'id':id,'mode':'tombstone'}
 n=(IDS[0],IDS[1],IDS[2],IDS[4]).index(id)
 return {'id':id,'mode':('hosted','quip','list','hosted')[n],'filetype':('svg','quip','list','svg')[n],
         'name':NAMES[n],'title':'DIFFERENT title '+id,'created':101,'permalink':'https://example.test/canonical/'+id,
         'is_deleted':False,'url_private':URLS[0],'thumb_360':URLS[1],'fixture_opaque':{'retain':[False,0,None]}}
def roots(mode='original'):
 result=[]
 for ts,id,label in zip(ROOTS,(IDS[0],IDS[1],IDS[2],IDS[4],IDS[3]),('Hosted','Canvas','List','Hidden','First blocked')):
  r=root(ts,label+' fallback & \u4e2d\U0001f98a')
  r['document_comment']['fixture_opaque']={'keep':True,'order':[2,1]}
  r['blocks']=[{'type':'rich_text','block_id':'original-'+ts,'elements':[{'type':'rich_text_section','elements':[{'type':'text','text':r['text'],'style':{'bold':True}}]}]}]
  first=file(id);first['name']='EMBEDDED first '+id;last=file(id)
  if id==IDS[0]:
   last={'renamed':{'id':id,'name':'Renamed canonical & \u4e2d\U0001f98a','permalink':'https://example.test/canonical/renamed','updated':102},
         'no-permalink':{'id':id,'name':'Name without permalink','updated':103},
         'deleted':{'id':id,'is_deleted':True,'updated':104}}.get(mode,last)
  r['files']=[first,last]
  r['attachments']=[{'id':1,'text':'DISCARDED attachment text','image_url':URLS[2],'image_width':120,'image_height':80}]
  result.append(r)
 result[3]['document_comment']['is_visible']=False
 result[4]['files']=[file(IDS[3]),file(IDS[4])]
 return result
def envelope(mode='original',thread=False):
 rows=roots(mode)
 return {'ok':True,'has_more':False,'messages':[rows[0],REPLY] if thread else rows}
def responses():
 return [envelope(),envelope(),envelope(thread=True),envelope('renamed',True),
         envelope('no-permalink',True),envelope('deleted',True),None,envelope(thread=True),envelope(thread=True)]
def modes(index):
 return {6:'renamed',7:'no-permalink',8:'deleted'}.get(index,'original')
def originals(index):
 return [] if index==9 else roots() if index<5 else [roots(modes(index))[0],REPLY]
def wire_proof(p,expected_responses,thread_reads,expected_rows,thread,visible=True,error=None):
 count=len(expected_responses)
 require(same(p.get('responses'),expected_responses),'Original response prefix substituted')
 req=p.get('requests');require(isinstance(req,list) and len(req)==count,'Comment request count differs')
 for n,r in enumerate(req):request(r,'replies' if n in thread_reads else 'history',ROOT if n in thread_reads else None,None)
 media=p.get('media');require(isinstance(media,list) and len(media)==3,'Discarded-media audit missing')
 for m,url in zip(media,URLS):require(m.get('url')==url and type(m.get('registered')) is int and m['registered']==0,'Discarded media registered')
 v=p.get('retained')
 require(isinstance(v,dict) and (not visible or v.get('ownerCurrent') is True) and v.get('loading') is False,'Original comments owner not current')
 require(type(v.get('serial')) is int and v['serial']>0,'Original serial missing')
 require(same(v.get('query'),{'kind':'thread' if thread else 'stream','rootTs':ROOT if thread else None}),'Original query changed')
 require(v.get('error')==error,'Original read error differs')
 rows=v.get('rows');require(isinstance(rows,list) and len(rows)==len(expected_rows),'Original rows missing')
 for row,raw in zip(rows,expected_rows):
  require(row.get('matchesOriginal') is True and retained_fields(row.get('message'),raw),'Retained original Message changed')
  for key in ('blocks','document_comment'):
   if key in raw:require(same(row['message'].get(key),raw[key]),'Raw body/control changed')
 if expected_rows and thread:
  r=v.get('root');require(isinstance(r,dict) and r.get('matchesOriginal') is True and retained_fields(r.get('message'),expected_rows[0]),'Retained thread root changed')
 elif not thread:require(v.get('root') is None,'Stream borrowed a thread root')
 # On failure the response factory did not run; originals remains the prior received page.
 if error is None:require(same(p.get('originals'),expected_rows),'Original raw sidecar changed')
 return v
def expected_link(ts,index):
 if ts==ROOTS[3] or ts==ROOTS[4] or ts==REPLY['ts'] or index in (7,8,11):return None
 id=IDS[ROOTS.index(ts)]
 return {'id':id,'name':'Renamed canonical & \u4e2d\U0001f98a' if index==6 else file(id)['name'],
         'url':'https://example.test/canonical/renamed' if index==6 else file(id)['permalink'],'root':ts}
def expected_text(raw,index):
 link=expected_link(raw['ts'],index)
 return link['name'] if link else 'This content was deleted.' if raw['ts']==ROOTS[3] else raw['text']
def display_layout(v,index,viewport):
 require(v.get('ok') is True and v.get('viewport')==viewport and v.get('focused') is True and v.get('noOverflow') is True,'Panel state/viewport changed')
 require(bounded(v.get('dialog'),viewport) and bounded(v.get('panel'),viewport),'Panel clipped')
 controls=v.get('controls');require(isinstance(controls,list) and len(controls) in ((3,) if index<5 else (3,5)) and all(c.get('hit') is True and bounded(c.get('rect'),viewport) for c in controls),'Read controls clipped')
 if 'controlScope' in v:require(v['controlScope']=='comment-navigation' and len(controls)==3,'Scoped read toolbar includes foreign controls')
 require(controls[0].get('label')=='Back to canvas' and controls[2].get('label')=='Refresh comments','Read controls changed')
 if index>=5:
  require(controls[1].get('label')=='All threads','Original thread navigation changed')
  if len(controls)==5:require([c.get('label') for c in controls[3:]]==['Send reply','Earlier local attempts'],'Unexpected nested reply controls')
 b=v.get('bodyProof');require(isinstance(b,dict) and b.get('ok') is True and bounded(b.get('scroll'),viewport),'Body proof missing')
 rows=b.get('rows');raws=originals(index)
 require(isinstance(rows,list) and [r.get('ts') for r in rows]==[r['ts'] for r in raws],'Rendered original identities changed')
 for row,raw in zip(rows,raws):
  text=expected_text(raw,index);require(row.get('text')==text and row.get('body',{}).get('text')==text,'Canonical name/fallback differs')
  require(row.get('images')==[] and row.get('attachments')==[],'Discarded generic file/media leaked')
  link=expected_link(raw['ts'],index)
  if link:
   require(len(row.get('links',[]))==1,'Canonical link missing')
   a=row['links'][0]
   require(all(a.get(k)==value for k,value in link.items()) and a.get('href')==link['url'] and a.get('text')==link['name'],'Canonical name/permalink/identity substituted')
   require(row.get('marks')==[],'Discarded rich fallback leaked')
  else:
   require(row.get('links')==[],'Ineligible first file borrowed later link')
   if raw['ts']==ROOT and index in (7,8,11) or raw['ts']==ROOTS[4]:
    require(any('bold' in m.get('className','').split() and m.get('text')==text and str(m.get('weight','')).isdigit() and int(m['weight'])>=600 for m in row.get('marks',[])),'Original rich fallback bold lost')
  button=row.get('threadButton')
  if index>=5:require(button is None,'Thread gained stream Open action')
  else:require(isinstance(button,dict) and button.get('text')=='Open thread' and button.get('disabled') is False,'Original Open thread changed')
 target=ROOTS[index] if index<5 else None if index==9 else ROOT
 require(b.get('targetTs')==target and b.get('targetKind')==('alert' if index==9 else 'body'),'Capture target differs')
 require(b.get('alerts')==([ERROR] if index==9 else []),'Failure guidance differs')
 shown(b.get('target'),viewport,b['scroll'])
 if target and index<5:shown(rows[index]['threadButton'],viewport,b['scroll'])
def validate(receipt):
 stages=receipt.get('stages');require(isinstance(stages,list) and len(stages)==len(STAGES),'Missing display stages')
 viewport=receipt.get('layout',{}).get('viewport');first=None
 for i,(s,name,count) in enumerate(zip(stages,STAGES,READS)):
  require(s.get('stage')==name and type(s.get('ordinal')) is int and s['ordinal']==i and all(s.get(k) is True for k in ('ok','noSession','noTransport')),'Stage order/isolation changed')
  p=s['proof'];v=wire_proof(p,responses()[:count],set(range(2,9)),originals(i),i>=5,error=ERROR if i==9 else None)
  first=v['serial'] if first is None else first
  require(v['serial']==first+count-2,'Original read serial changed')
  require(p.get('resolver')=={'requests':[],'responses':[],'settlements':[],'releases':[],'held':None},'Display issued file navigation')
  if i!=9:
   catalog=v.get('catalog');require(isinstance(catalog,dict) and catalog.get('serial')==v['serial'],'Canonical source observation missing')
   for raw in originals(i):
    if raw['ts'] in (REPLY['ts'],ROOTS[3]):continue
    first_id=raw['files'][0]['id']
    origin=next((o for o in catalog.get('origins',[]) if o.get('ts')==raw['ts']),None)
    require(origin=={'ts':raw['ts'],'firstId':first_id,'blockId':'original-'+raw['ts']},'First-file origin/block identity changed')
   hosted=next((r for r in catalog.get('rows',[]) if r.get('id')==IDS[0]),None)
   require(hosted is not None and hosted.get('current') is (i!=11),'Original event fence differs')
   source=roots(modes(i))[0]['files'][-1]
   require(same(hosted.get('source'),source),'Canonical source was typed/defaulted or substituted')
   effective=hosted.get('effective');require(isinstance(effective,dict) and effective.get('id')==IDS[0],'Effective ID changed')
   if i==8:require(effective.get('is_deleted') is True and 'name' not in effective and 'permalink' not in effective,'Deleted replacement retained previous facts')
   else:
    require(effective.get('name')==source['name'],'Canonical source name replaced by title/embedded name')
    require(effective.get('permalink')==source.get('permalink'),'Omitted permalink inherited predecessor')
   link=None if i in (7,8) else {'name':source['name'],'url':source['permalink']}
   require(same(hosted.get('link'),link),'Canonical eligibility/link mismatch')
  display_layout(s['layout'],i,viewport)
 require(same(receipt.get('proof'),stages[-1]['proof']) and same(receipt.get('layout'),stages[-1]['layout']),'Terminal changed captured proof')
 require(receipt.get('staleCallback')=={'originalCurrent':False,'resolverUnchanged':True},'Original stale callback not refused')
