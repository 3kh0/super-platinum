"""Independent offline heading annotation wire/DOM/journal oracle. No server echo claim."""
import json
from html.parser import HTMLParser
from canvas_heading import require,decoded,wire,field,text
from canvas_underline_annotation import form

STAGES=('initial','existing','h1-selected','h1-created','h1-canonical','h2-selected','h2-stale','h2-created','h2-canonical','h3-selected','h3-created','h3-canonical','twin')
WRITES=(0,0,0,2,2,2,2,4,4,4,6,6,6)
IDS=('heading-one','heading-two','heading-three','heading-twin')
LEVELS=(1,2,3,2)
TEXT='A&\U0001f600B New &\u4e2d\U0001f98a tail'
SELECTED='New &\u4e2d\U0001f98a'
RTML='<b>A&amp;</b><u>\U0001f600B</u> <i>New</i> <u>&amp;\u4e2d\U0001f98a</u> tail'
STALE='The paragraph or access changed. Refresh before creating this comment.'


def shape(value):
    class P(HTMLParser):
        def __init__(self):super().__init__(convert_charrefs=True);self.tags=[];self.paint=[];self.controls=[];self.active=[]
        def handle_starttag(self,tag,attrs):
            require(tag in ('b','i','u','annotation'),'Unexpected canonical tag')
            self.tags.append(tag)
            if tag=='annotation':
                a=dict(attrs);require(set(a)=={'id'} and a['id'],'Invalid control attributes')
                self.active.append((a['id'],len(self.paint)))
        def handle_endtag(self,tag):
            require(self.tags and self.tags.pop()==tag,'Malformed canonical nesting')
            if tag=='annotation':
                id,start=self.active.pop();self.controls.append({'id':id,'text':''.join(v[0] for v in self.paint[start:])})
        def handle_data(self,data):self.paint.extend([c,'b' in self.tags,'i' in self.tags,'u' in self.tags] for c in data)
    p=P();p.feed(value);require(not p.tags,'Unclosed canonical tags');return p.paint,p.controls

PAINT=shape(RTML)[0]
DOM_PAINT=[[c,True,i,u] for c,b,i,u in PAINT] # semantic headings have inherited bold weight

def rows(proof):
    values=proof.get('sections');require(isinstance(values,list),'Missing canonical section inventory')
    out={}
    for row in values:
        require(row.get('id') not in out,'Duplicate section identity');fields=decoded(row['raw'])
        require(text(fields,1)==row['id'],'Wire section identity mismatch');out[row['id']]=(row,fields)
    return out

def content(fields):
    container=wire(field(fields,12,2));inner=wire(field(container,1,2))
    require(text(inner,2)=='ja','Original heading language changed')
    return text(inner,1)

def optional_uint(fields,tag,default):
    found=[v for v in fields if v[0]==tag]
    require(len(found)<=1 and all(v[1]==0 for v in found),'Duplicate/wrong ordinary scalar')
    return found[0][2] if found else default

def stable(fields):
    # The CONTENT producer owns these exact generated fields. All remaining
    # field bytes, including unknown fields and explicit presence, stay exact.
    return sorted((v[0],v[3]) for v in fields if v[0] not in (2,9,12,13,19,20,33,38))

def parent_request(request,id):
    _,form_fields=form(request)
    updates=[wire(v[2]) for v in decoded(form_fields['data_binary']) if v[0]==1]
    parents=[v for v in updates if text(v,1)==id]
    require(len(parents)==1,'Missing/duplicate original heading update')
    parent=parents[0]
    require(text(parent,33)==form_fields['session']+'-0','Generated actor does not bind original request session')
    require(field(parent,9,0)==0 and field(parent,13,2)==b'' and field(parent,19,0)==field(parent,20,0)==1,'Unexpected generated CONTENT fields')
    return parent


def snippet(value):
    rich=json.loads(value['rich_text']);leaves=[]
    def scan(v):
        if isinstance(v,dict):
            if v.get('type')=='text':leaves.append(v)
            for c in v.get('elements',[]):scan(c)
    for block in rich.get('blocks',[]):scan(block)
    painted=[]
    for leaf in leaves:
        s=leaf.get('style',{});painted.extend([c,s.get('bold') is True,s.get('italic') is True,s.get('underline') is True] for c in leaf.get('text',''))
    require(painted==shape('<i>New</i> <u>&amp;\u4e2d\U0001f98a</u>')[0],'Selected rich snippet formatting differs')
    require('<u>' not in value['mrkdwn'] and SELECTED.replace('New','') in value['mrkdwn'],'Invalid markdown snippet')

def validate(receipt,content_id=False):
    _validate(receipt,len(STAGES),True,content_id)

def validate_retained_prefix(stages):
    """Offline diagnostics only; the executable runner always requires all stages."""
    require(isinstance(stages,list) and 0<len(stages)<len(STAGES),'Not a retained failed-run prefix')
    _validate({'initial':stages[0]['fixture'],'stages':stages},len(stages),False)

def _validate(receipt,count_expected,terminal,content_id=False):
    stages=receipt.get('stages');require(isinstance(stages,list) and len(stages)==count_expected,'Missing heading stages')
    initial=receipt['initial'];original=rows(initial)
    require(initial['prepared']==[] and len(initial['seedRequests'])==2 and len(initial['roots'])==1 and initial['acks']==[],'Seed/driven ledger mixed')
    require(initial['file']['access']=='comment' and field(decoded(initial['thread']),23,0)==5,'Missing positive comment grant')
    old=initial['roots'][0]['document_comment']['thread_id']
    for id,level in zip(IDS,LEVELS):
        f=original[id][1];require(optional_uint(f,9,0)==0 and field(f,10,0)==level and text(f,177)=='heading-comment-opaque','Wrong original heading shape')
        require(shape(content(f))[0]==PAINT,'Wrong original styled text')
        if content_id:
            attrs=wire(field(f,16,2))
            require([v[0] for v in attrs]==[4,13,29,177] and text(attrs,29)=='retained-'+id and field(attrs,4,0)==field(attrs,13,0)==0 and text(attrs,177)=='heading-attributes-opaque','Wrong original full content-id attributes')
    if content_id:
        seed_parent=parent_request(initial['seedRequests'][0],IDS[0])
        require([v[3] for v in seed_parent if v[0]==16]==[v[3] for v in original[IDS[0]][1] if v[0]==16],'Existing-control seed lost full content-id attributes')
    history=[];prior_requests=[];prior_prepared=[];prior_acks=[]
    for n,(stage,name) in enumerate(zip(stages,STAGES)):
        require(stage.get('stage')==name and stage.get('ordinal')==n and all(stage.get(k) is True for k in ('ok','noSession','noTransport')),'Stage identity/isolation mismatch')
        p=stage['fixture'];current=rows(p);count=WRITES[n]//2
        require(p['document']==initial['document'] and p['thread']==initial['thread'] and p['file']==initial['file'],'Document/access bytes changed')
        require(p['seedRequests']==initial['seedRequests'] and p['roots'][0]==initial['roots'][0] and p['replies']==initial['replies'],'Existing thread mutated')
        require(len(p['prepared'])==WRITES[n] and len(p['roots'])==1+count and len(p['acks'])==count,'Write/root/ACK count differs')
        require(p['sourceChanges']==int(n>=6),'Stale source advance missing/extra')
        require(p['requests'][:len(prior_requests)]==prior_requests and p['prepared'][:len(prior_prepared)]==prior_prepared and p['acks'][:len(prior_acks)]==prior_acks,'Original receipt mutated')
        prior_requests=p['requests'];prior_prepared=p['prepared'];prior_acks=p['acks']
        allcontrols=[]
        for id,level in zip(IDS,LEVELS):
            f=current[id][1];require(stable(f)==stable(original[id][1]),'Heading style/position/opaque field mutated')
            paint,controls=shape(content(f));require(paint==PAINT,'Heading visible text/style changed');allcontrols.extend(c['id'] for c in controls)
            expected=([{'id':old,'text':'&\U0001f600'}] if id==IDS[0] else [])
            index=IDS.index(id)
            if index<count:
                generated=parent_request(p['prepared'][index*2],id)
                attrs=[v[3] for v in generated if v[0]==16]
                require(attrs==([v[3] for v in original[id][1] if v[0]==16] if content_id else []),'CONTENT parent lost/changed/introduced full attributes')
                for tag in (9,13,19,20,33):
                    require([v[3] for v in f if v[0]==tag]==[v[3] for v in generated if v[0]==tag],'Canonical generated field differs from original issued parent')
            else:
                for tag in (9,13,19,20,33):
                    require([v[3] for v in f if v[0]==tag]==[v[3] for v in original[id][1] if v[0]==tag],'Untouched heading generated metadata changed')
            if index<count:expected.append({'id':p['roots'][index+1]['document_comment']['thread_id'],'text':SELECTED})
            require(controls==expected,'Original control identity/range changed or duplicate wrapper')
        for id,(row,f) in original.items():
            if id not in IDS[:3]:require(current[id][0]==row,'Untouched twin/sibling/control bytes changed')
        require(set(current)-set(original)==set(allcontrols)-{old},'Unexpected canonical sibling/control')
        for encoded in p['acks']:
            ack=decoded(encoded);require(field(ack,1,0)==1,'ACK not positive')
            for entry in wire(field(ack,2,2)):
                require(entry[0:2]==(1,2),'Unexpected ACK field')
                fs=wire(entry[2]);id=text(fs,1)
                require(id in current and entry[2]==__import__('base64').b64decode(current[id][0]['raw']),'ACK/canonical bytes differ')
        driven=[]
        for request in p['requests']:
            if 'observation' in request:
                require(request=={'observation':'canonical_fixture_snapshot','file':'Ffixture'},'Unexpected source producer');continue
            path,f=form(request)
            if path=='/canvas/-/edit-document':
                require(set(f)=={'thread','document','retry_count','secret_path','session','sequence','nav_action_id','search_session_id','data_binary','title'},'Unexpected anchor fields')
                require(f['thread']=='thread' and f['document']=='document' and f['retry_count']=='0' and f['sequence']=='1' and f['nav_action_id']==f['search_session_id']=='null','Foreign/retried anchor')
                updates=[wire(v[2]) for v in decoded(f['data_binary']) if v[0]==1]
                index=len(driven)//2;require(any(text(v,1)==IDS[index] for v in updates),'Anchor addressed wrong original heading')
                driven.append(request)
            else:
                require(path in ('/api/conversations.history','/api/conversations.replies') and f.get('channel')=='Cfixture','Unexpected read/foreign scope')
                if f.get('ts','').startswith('Qpc:'):snippet(json.loads(f['content_snippet']));driven.append(request)
                else:require('content_snippet' not in f,'Ordinary read gained bootstrap authority')
        require(driven==p['prepared'],'Prepared/issued order changed')
        j=stage['journal'];require(j['pending'] is None,'Confirmed operation remains pending')
        require(len(j['history'])==count*2 and sorted(v['status'] for v in j['history'])==sorted(['anchor_acknowledged','root_accepted']*count),'Missing original anchor/root durable history')
        require(all(v in j['history'] for v in history),'Prior durable receipt replaced');history=j['history']
        g=stage['geometry'];require(g.get('ok') is True and g['focusTarget']['inView'] is True and g.get('critical') and all(v.get('inView') is True for v in g['critical']),'Meaningful selected content/action not visible')
        if name in ('initial','h1-canonical','h2-canonical','h3-canonical','twin'):
            require([v['id'] for v in g['headings']]==list(IDS),'Missing original DOM headings')
            for h,level in zip(g['headings'],LEVELS):
                require(h['tag']=='H'+str(level) and h['text']==TEXT and h['painted']==DOM_PAINT and h['marks']==PAINT,'Heading semantics/paint lost')
                require(h['controls']==shape(content(current[h['id']][1]))[1],'DOM duplicated/dropped original control')
            visible_id={'initial':IDS[0],'h1-canonical':IDS[0],'h2-canonical':IDS[1],'h3-canonical':IDS[2],'twin':IDS[3]}[name]
            require(next(v for v in g['headings'] if v['id']==visible_id)['inView'] is True,'Selected heading clipped')
        if name=='existing':require('Existing comment' in g['text'],'Existing H1 control did not route')
        if name=='h2-stale':require(STALE in g['alerts'] and STALE in [v['text'] for v in g['critical']],'Missing visible stale refusal')
        if name in ('h1-selected','h2-selected','h3-selected','h2-stale'):require(SELECTED in [v['text'] for v in g['critical']] and 'Create comment thread' in [v['text'] for v in g['critical']],'Selected snippet and action not both visible')
        if name.endswith('-created'):
            result=[v for v in g.get('controls',[]) if v.get('label')=='Open created thread']
            require(len(result)==1 and result[0].get('inView') is True and result[0].get('disabled') is False,'Created result action not visible/enabled')
    if not terminal:return
    require(receipt['fixture']==stages[-1]['fixture'],'Terminal source drift')
    selections=receipt.get('selections');require(isinstance(selections,list) and len(selections)==5,'Missing DOM Range selections')
    for s,(id,level,start,end,value) in zip(selections,[(IDS[0],1,1,4,'&\U0001f600'),(IDS[0],1,6,14,SELECTED),(IDS[1],2,6,14,SELECTED),(IDS[1],2,6,14,SELECTED),(IDS[2],3,6,14,SELECTED)]):
        require(s.get('ok') is True and s.get('id')==id and s.get('tag')=='H'+str(level) and (s.get('start'),s.get('end'),s.get('text'),s.get('source'))==(start,end,value,TEXT),'Native original heading/UTF16 selection differs')
    stale=receipt['stale'];require(stale['id']==IDS[1] and stale['before']==stages[5]['fixture']['sections'][next(i for i,v in enumerate(stages[5]['fixture']['sections']) if v['id']==IDS[1])]['raw'],'Stale source witness differs')
    a,b=decoded(stale['before']),decoded(stale['after']);require([v[3] for v in a if v[0]!=2]==[v[3] for v in b if v[0]!=2] and field(b,2,0)==field(a,2,0)+1,'Stale event changed content/identity')
