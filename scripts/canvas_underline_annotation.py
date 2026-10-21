"""Independent bounded annotation wire, style, journal and native receipt oracle."""
import json
from html.parser import HTMLParser
from urllib.parse import urlsplit
from canvas_heading import require,decoded,wire,field,text

STAGES=('initial','existing','selected','stale','selected-fresh','created','canonical','uncertain-selected','uncertain','reviewed','reviewed-again')
TEXT='A&\U0001f600B\nNew &\U0001f98a tail'
WRITES=(0,0,0,0,0,2,2,2,3,3,3)
ROOTS=(1,1,1,1,1,2,2,2,2,2,2)
UNKNOWN='This comment thread may already exist. Refresh to review the current canvas. It will not be created again automatically.'
STALE='The paragraph or access changed. Refresh before creating this comment.'

def styles(value):
    class Parse(HTMLParser):
        def __init__(self):super().__init__(convert_charrefs=True);self.u=0;self.rows=[];self.controls=[]
        def handle_starttag(self,tag,attrs):
            if tag=='u':self.u+=1
            elif tag=='br':self.rows.append(['\n',False])
            elif tag=='annotation':self.controls.append(dict(attrs))
        def handle_endtag(self,tag):
            if tag=='u':self.u-=1
        def handle_data(self,data):self.rows.extend([c,self.u>0] for c in data)
    p=Parse();p.feed(value);return p.rows,p.controls

PAINT=[[c,True] for c in 'A&\U0001f600B']+[['\n',False]]+[[c,True] for c in 'New &\U0001f98a']+[[c,False] for c in ' tail']
def paragraph(proof):
    rows=proof.get('sections');require(isinstance(rows,list),'Missing canonical sections')
    selected=[r for r in rows if r.get('id')=='paragraph'];require(len(selected)==1,'Ambiguous paragraph')
    fields=decoded(selected[0]['raw']);require(text(fields,1)=='paragraph','Canonical identity changed')
    value=text(wire(field(wire(field(fields,12,2)),1,2)),1)
    require(styles(value)[0]==PAINT,'Canonical text/underline changed')
    return value

def form(request):
    pairs=request.get('fields');require(isinstance(pairs,list) and all(isinstance(p,list) and len(p)==2 and all(isinstance(x,str) for x in p) for p in pairs),'Malformed request form')
    f=dict(pairs);require(len(f)==len(pairs) and 'token' not in f,'Duplicate/secret form fields')
    url=urlsplit(request.get('url',''));require(url.scheme=='https' and url.netloc=='fixture.slack.com' and request.get('method')=='POST','Wrong closed request origin')
    return url.path,f

def snippet(value):
    require(isinstance(value,dict),'Missing snippet')
    rich=json.loads(value['rich_text']);blocks=rich.get('blocks');require(isinstance(blocks,list) and blocks,'Missing rich snippet')
    leaves=[]
    def scan(v):
        if isinstance(v,dict):
            if v.get('type')=='text':leaves.append(v)
            for x in v.get('elements',[]):scan(x)
    for b in blocks:scan(b)
    under=''.join(v.get('text','') for v in leaves if v.get('style',{}).get('underline') is True)
    require('New &\U0001f98a' in under,'Inner underline absent from rich snippet')
    require('<u>' not in value.get('mrkdwn','') and '</u>' not in value.get('mrkdwn',''),'Underline leaked as HTML Markdown')
    require('New &\U0001f98a' in value.get('mrkdwn',''),'Markdown text lost')

def validate(receipt):
    stages=receipt.get('stages');require(isinstance(stages,list) and len(stages)==len(STAGES),'Missing stages')
    initial=receipt['initial'];paragraph(initial)
    require(len(initial['seedRequests'])==2 and len(initial['roots'])==1 and initial['prepared']==[],'Seed/driven request distinction lost')
    original_root=initial['roots'][0];oldcontrol=original_root['document_comment']['thread_id']
    previous=[];pending=None;confirmed_history=None
    for n,(stage,name) in enumerate(zip(stages,STAGES)):
        require(stage.get('stage')==name and stage.get('ordinal')==n and all(stage.get(k) is True for k in ('ok','noSession','noTransport')),'Stage order/isolation changed')
        p=stage['fixture'];rtml=paragraph(p)
        for original in initial['sections']:
            if original['id']!='paragraph':
                require([r for r in p['sections'] if r['id']==original['id']]==[original],'Untouched original section/control changed')
        require(p['seedRequests']==initial['seedRequests'] and p['roots'][0]==original_root,'Original thread changed')
        require(oldcontrol in rtml,'Existing inline identity lost')
        require(len(p['prepared'])==WRITES[n] and len(p['roots'])==ROOTS[n] and len(p['acks'])==(0 if n<5 else 1 if n<8 else 2),'Write/root/ACK count changed')
        require(p['sourceChanges']==(0 if n<3 else 1),'Source retirement count changed')
        require(p['requests'][:len(previous)]==previous,'Original request ledger mutated');previous=p['requests']
        driven=[]
        for request in p['requests']:
            if 'observation' in request:
                require(request=={'observation':'canonical_fixture_snapshot','file':'Ffixture'},'Unexpected producer');continue
            path,f=form(request)
            if path=='/canvas/-/edit-document':
                require(set(f)=={'thread','document','retry_count','secret_path','session','sequence','nav_action_id','search_session_id','data_binary','title'},'Anchor keys differ')
                require(f['retry_count']=='0' and f['sequence']=='1' and f['nav_action_id']==f['search_session_id']=='null','Replay/foreign anchor form')
                require(f['thread']=='thread' and f['document']=='document','Anchor scope changed')
                rows=[wire(x[2]) for x in decoded(f['data_binary']) if x[0]==1];require(any(text(r,1)=='paragraph' for r in rows),'No original parent in write')
                driven.append(request)
            else:
                require(path in ('/api/conversations.history','/api/conversations.replies'),'Unexpected HTTP method')
                require(f.get('channel')=='Cfixture','Foreign comment channel')
                if f.get('ts','').startswith('Qpc:'):
                    snippet(json.loads(f['content_snippet']));driven.append(request)
                else:require('content_snippet' not in f,'Ordinary read gained write authority')
        require(driven==p['prepared'],'Prepared/issued order differs')
        g=stage['geometry'];require(g.get('ok') is True,'Invalid geometry')
        if name in ('initial','canonical'):
            require(g['source']['text']==TEXT and g['source']['painted']==PAINT,'Original reader underline lost')
        if name=='existing':require('Existing comment' in g['text'],'Existing thread not routed')
        if name=='stale':require(STALE in g['alerts'],'No stale refusal')
        j=stage['journal']
        if n<5:require(j=={'pending':None,'history':[]},'Unsent selection changed journal')
        elif n<8:require(j['pending'] is None,'Confirmed annotation still pending')
        else:
            require(j['pending'] is not None and j['pending']['stage']=='AnchorStarted','Uncertain lock lost')
            if pending is None:pending=j['pending']
            require(j['pending']==pending,'Review replaced original uncertain attempt')
            if n==8:require(UNKNOWN in g['alerts'],'Unknown warning lost')
            require(not any(c['id']=='canvas-comment-create' and not c['disabled'] for c in g['controls']),'Unknown permits replay')
        if n>=5:
            require(sorted(h.get('status','') for h in j['history'])==['anchor_acknowledged','root_accepted'],'Confirmed anchor/root history differs')
            if confirmed_history is None:confirmed_history=j['history']
            require(j['history']==confirmed_history,'Review changed confirmed history')
        if n>=9:
            require(g['alerts']==[],'Reviewed transient error did not clear')
            require('The original annotation and its delivery state are retained locally.' in g['text'],'Missing retained delivery-state guidance')
            for label,disabled in [('Close annotation review',True),('Keep outcome unknown and remove local lock',True),('Refresh current annotation state',False)]:
                controls=[c for c in g['controls'] if c.get('label')==label]
                require(len(controls)==1 and controls[0].get('disabled') is disabled,'Reviewed lock/recovery controls differ')
            preview=g['preview'];require(preview and preview['painted']==PAINT,'Actual rich review underline/text lost')
            require('The original annotation control is present.' in g['text'],'Missing canonical uncertain review')
    require(receipt['fixture']==stages[-1]['fixture'],'Terminal ledger drift')
    require(len(receipt['selections'])==4,'Missing native selections')
    for selection,(start,end,wanted) in zip(receipt['selections'],[(1,4,'&\U0001f600'),(5,13,'\nNew &\U0001f98a'),(5,13,'\nNew &\U0001f98a'),(14,18,'tail')]):
        require(selection.get('ok') is True and selection.get('start')==start and selection.get('end')==end and selection.get('text')==wanted and selection.get('source')==TEXT,'Native UTF16 selection differs')
