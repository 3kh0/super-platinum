"""Independent synthetic wire/output oracle; canonical file override remains unimplemented."""
import math
from canvas_special_roots import require, same, retained_fields, request, bounded, shown
STAGES=('initial','thread','thread-refreshed','unsupported-blocks','formatted','markdown','literal','hidden','unavailable','error','recovered')
READS=(2,3,4,6,6,6,6,6,6,7,8)
ROOT='1719800000.000100'; REPLY='1719800001.000100'
CONTROLS=tuple(f'17198000{n}0.000100' for n in range(2,8))
URLS=tuple('https://example.test/file-fallback/'+n for n in ('file.png','download.png','thumb.png','attachment.png','attachment-thumb.png','author.png','second.png'))
ERROR='Comments could not be loaded. Your canvas is unchanged. Retry when ready.'
TEXT={ROOT:'Original block & \u4e2d\U0001f98a',REPLY:'Ordinary reply remains unchanged.',CONTROLS[0]:'',CONTROLS[1]:'Formatted & \u4e2d\U0001f98a ',CONTROLS[2]:'Markdown & \u4e2d\U0001f98a',CONTROLS[3]:'*Literal & \u4e2d\U0001f98a*',CONTROLS[4]:'This content was deleted.',CONTROLS[5]:''}
def root(ts,text):
 return {'type':'message','subtype':'document_comment_root','ts':ts,'user':'U1','text':text,'reply_count':2,'document_comment':{'thread_id':'quip-'+ts}}
def decoys(row):
 row['files']=[{'id':'FFALLBACK1','name':'DECOY file name','title':'DECOY file title','filetype':'png','mimetype':'image/png','size':42,'url_private':URLS[0],'url_private_download':URLS[1],'thumb_360':URLS[2],'original_w':120,'original_h':80}]
 row['attachments']=[{'id':1,'text':'DECOY attachment','title':'DECOY attachment title','image_url':URLS[3],'image_width':120,'image_height':80,'thumb_url':URLS[4],'author_name':'DECOY attachment author','author_icon':URLS[5]}]
 row['document_comment']['opaque']={'preserve':True}
 return row
def primary():
 r=decoys(root(ROOT,'DECOY primary fallback'));r['document_comment']['is_visible']=True
 r['blocks']=[{'type':'rich_text','block_id':'original-block','elements':[{'type':'rich_text_section','elements':[{'type':'text','text':TEXT[ROOT],'style':{'bold':True,'underline':True}}]}]}]
 r['files'].append({'id':'FFALLBACK2','name':'DECOY second file','title':'DECOY second title','filetype':'png','mimetype':'image/png','url_private':URLS[6]})
 return r
def controls():
 r=[decoys(root(ts,'')) for ts in CONTROLS]
 r[0].update(text='DECOY unsupported fallback',blocks=[{'type':'fixture_unsupported','text':'DECOY unsupported block'}])
 r[1].update(text='_Formatted &amp; \u4e2d\U0001f98a _',blocks=[])
 r[2]['markdown_text']='*Markdown &amp; \u4e2d\U0001f98a*';r[2]['document_comment']['is_visible']=None
 r[3].update(text='*Literal &amp; \u4e2d\U0001f98a*',mrkdwn=False)
 r[4].update(text='DECOY hidden text',blocks=[{'type':'section','text':{'type':'plain_text','text':'DECOY hidden blocks'}}]);r[4]['document_comment']['is_visible']=False
 r[5]['markdown_text']={'unsupported':'DECOY malformed fallback'}
 return r
def responses():
 stream={'ok':True,'has_more':True,'response_metadata':{'next_cursor':'fallback-controls'},'messages':[primary()]}
 thread={'ok':True,'has_more':False,'messages':[primary(),{'type':'message','ts':REPLY,'thread_ts':ROOT,'user':'U2','text':TEXT[REPLY]}]}
 return [stream,stream,thread,thread,stream,{'ok':True,'has_more':False,'messages':controls()},None,stream]
def originals(index):
 return [primary()] if index in (0,10) else responses()[2]['messages'] if index in (1,2) else [] if index==9 else controls()
def target(index):return (ROOT,ROOT,ROOT,*CONTROLS,None,ROOT)[index]
def proof(p,index):
 count=READS[index]
 require(isinstance(p,dict) and same(p.get('primary'),primary()) and same(p.get('controls'),controls()),'Original fixture source substituted')
 require(same(p.get('responses'),responses()[:count]),'Original immutable response prefix changed')
 req=p.get('requests');require(isinstance(req,list) and len(req)==count,'Read count differs')
 for n,r in enumerate(req):request(r,'replies' if n in (2,3) else 'history',ROOT if n in (2,3) else None,'fallback-controls' if n==5 else None)
 media=p.get('media');require(isinstance(media,list) and len(media)==7,'Missing exact media audit')
 for m,url in zip(media,URLS):require(m.get('url')==url and type(m.get('registered')) is int and m['registered']==0,'Top-level decoy registered or audit substituted')
 v=p.get('retained');require(isinstance(v,dict) and v.get('ownerCurrent') is True and v.get('loading') is False and type(v.get('serial')) is int and v['serial']>0,'Original owner/read unavailable')
 thread=index in (1,2)
 require(same(v.get('query'),{'kind':'thread' if thread else 'stream','rootTs':ROOT if thread else None}),'Original query/control changed')
 require(v.get('error')==(ERROR if index==9 else None),'Retained failure differs')
 rows=v.get('rows');expected=originals(index)
 require(isinstance(rows,list) and len(rows)==len(expected),'Original retained rows missing')
 for row,raw in zip(rows,expected):
  message=row.get('message');require(row.get('matchesOriginal') is True and retained_fields(message,raw),'Retained message mutated')
  for k in ('blocks','document_comment','markdown_text'):
   if k in raw:require(same(message.get(k),raw[k]),'Raw control/body changed')
 if thread:require(retained_fields(v.get('root'),primary()) and same(v['root'].get('blocks'),primary()['blocks']) and same(v['root'].get('document_comment'),primary()['document_comment']),'Retained root differs')
 else:require(v.get('root') is None,'Stream borrowed thread root')
def marks(row):
 ts=row['ts'];items=row.get('marks');require(isinstance(items,list),'Missing actual mark proof')
 def marked(cls,style):return any(m.get('text')==TEXT[ts] and cls in m.get('className','').split() and style(m) for m in items)
 if ts==ROOT:
  require(marked('bold',lambda m:str(m.get('weight','')).isdigit() and int(m['weight'])>=600),'Rich bold absent')
  require(marked('rich-underline',lambda m:'underline' in m.get('decoration','').split()),'Rich underline absent')
 elif ts==CONTROLS[1]:require(marked('italic',lambda m:m.get('style')=='italic'),'Formatted fallback italic absent')
 elif ts==CONTROLS[2]:require(marked('bold',lambda m:str(m.get('weight','')).isdigit() and int(m['weight'])>=600),'Markdown fallback bold absent')
 else:require(items==[],'Literal/empty/hidden body unexpectedly formatted')
def layout(v,index,viewport):
 require(isinstance(v,dict) and v.get('ok') is True and v.get('focused') is True and v.get('noOverflow') is True and v.get('viewport')==viewport and bounded(v.get('dialog'),viewport) and bounded(v.get('panel'),viewport),'Panel geometry unavailable')
 c=v.get('controls');require(isinstance(c,list) and len(c)==3 and c[0].get('label')=='Back to canvas' and c[-1].get('label')=='Refresh comments' and all(x.get('hit') is True and bounded(x.get('rect'),viewport) for x in c),'Original controls clipped/changed')
 require(type(v.get('scrollHeight')) in (int,float) and v['scrollHeight']>=40,'Scroll viewport missing')
 b=v.get('bodyProof');require(isinstance(b,dict) and b.get('ok') is True and bounded(b.get('scroll'),viewport),'Body geometry absent')
 rows=b.get('rows');ids=[r['ts'] for r in originals(index)];ts=target(index)
 require(isinstance(rows,list) and [r.get('ts') for r in rows]==ids and b.get('targetTs')==ts,'Original rows/target changed')
 for row in rows:
  require(row.get('text')==TEXT[row['ts']] and row.get('body',{}).get('text')==TEXT[row['ts']],'Original body or empty refusal changed')
  require(all(row.get(k)==[] for k in ('links','images','attachments')),'Top-level decoy output leaked')
  marks(row)
  button=row.get('threadButton')
  if index in (1,2):require(button is None,'Thread gained stream-only action')
  else:require(isinstance(button,dict) and button.get('disabled') is False and button.get('text')=='Open thread','Original thread action changed')
  if row['ts']==ts:
   empty=TEXT[ts]=='';kind='thread-button' if empty else 'body'
   require(b.get('targetKind')==kind and same(b.get('target'),button if empty else row['body']),'Visible target not bound to original body/action')
   if button:shown(button,viewport,b['scroll'])
 require(b.get('alerts')==([ERROR] if index==9 else []),'Failure guidance changed')
 if index==9:require(b.get('targetKind')=='alert' and b.get('target',{}).get('text')==ERROR,'Failure target not real alert')
 shown(b.get('target'),viewport,b['scroll'])
def validate(r):
 stages=r.get('stages');viewport=r.get('layout',{}).get('viewport')
 require(isinstance(viewport,list) and len(viewport)==2 and all(type(n) in (int,float) and math.isfinite(n) and n>0 for n in viewport),'Missing viewport')
 require(isinstance(stages,list) and len(stages)==len(STAGES),'Missing fallback stages')
 previous=[];first=None
 for i,(s,name,count) in enumerate(zip(stages,STAGES,READS)):
  require(s.get('stage')==name and type(s.get('ordinal')) is int and s['ordinal']==i and all(s.get(k) is True for k in ('ok','noSession','noTransport')),'Stage order/isolation changed')
  p=s.get('proof');proof(p,i);serial=p['retained']['serial'];first=serial if first is None else first
  require(serial==first+count-2,'Original read serial changed');require(same(p['requests'][:len(previous)],previous),'Earlier captured request changed');previous=p['requests']
  layout(s.get('layout'),i,viewport)
 require(same(r.get('proof'),stages[-1]['proof']) and same(r.get('layout'),stages[-1]['layout']),'Terminal proof substituted')
