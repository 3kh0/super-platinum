"""Empty Canvas completion plus independently owned durable reply title evidence."""
import copy
import json
import runpy
from pathlib import Path
from types import SimpleNamespace
import slack_list_empty_canvas_reply_source as S

require=S.require
LIFECYCLE='empty-canvas-lifecycle'
RECOVERY='empty-canvas-recovery'
STAGES={LIFECYCLE:('draft','query-ready','canvas-inserted','list-inserted','reopened','sending','sent','readback'),
 RECOVERY:('draft','query-ready','canvas-inserted','draft-failed','retry-held','draft-replaced','draft-closed','draft-late','return-held','reopened')}
ABSENT={'draft-failed','retry-held','draft-replaced','draft-closed','draft-late','return-held'}
DEPARTED={'draft-replaced','draft-closed','draft-late'}
CLOSED={'draft-closed','draft-late'}
# Each load is isolated. Only display labels and the exact new stage layout
# change; original owner/disk/native geometry/read/send guards remain intact.
U=runpy.run_path(str(Path(__file__).with_name('slack_list_canvas_user_titles.py')))
U=U['validate'].__globals__
F,G,R,X,C,H,READ=(U[k] for k in ('F','G','R','X','C','H','READ'))
F['source'].__globals__['RESPONSE']=S.search_text()
F['source'].__globals__['RAW']=json.loads(S.search_text())['files']
U.update(S=SimpleNamespace(label=lambda case:'Untitled',CANVAS=S.CANVAS,LIST=S.LIST,TEAM='T1',SELF='ULIST0001'),
         ABSENT=ABSENT,DEPARTED=DEPARTED,CLOSED=CLOSED,STAGES=STAGES)
owner,custody,popup,painted=(U[k] for k in ('owner','custody','popup','painted'))
leaf_label,display,token_blocks=(U[k] for k in ('leaf_label','display','token_blocks'))

def action_plan(case):
    q=lambda stage,value:(stage,'query',value)
    choose=lambda stage,id:(stage,'choose',id)
    click=lambda id:(None,'click',id)
    initial=[q('query-ready','@alex'),choose('canvas-inserted',S.CANVAS)]
    if case==LIFECYCLE:
        return initial+[q('list-inserted','@alex'),choose('list-inserted',S.LIST),click('slack-list-comment-send'),click('slack-list-comments-refresh')],[0,1,2,4,4,5,5,6]
    return initial+[click(x) for x in ('slack-list-close','slack-list-comment-write','slack-list-comment-title-retry',
        'slack-list-comments-back','slack-list-record-back','slack-list-close','slack-list-comment-write')],[0,1,2,4,5,7,8,8,9,9]

def inputs(row,case,a,prior):
    p=row['proof'];items=p['inputs']
    plan,counts=action_plan(case)
    require(items[:len(prior)]==prior and len(items)==counts[STAGES[case].index(row['stage'])],'Original input event history rewritten/omitted')
    current=copy.deepcopy(a)
    for ordinal,item in enumerate(items):
        v=item.get('value',{});kind=item.get('kind')
        require(v.get('ok') is True,'Original native action refused')
        stage,wanted,identity=plan[ordinal]
        require(kind==wanted and (stage is None or item.get('stage')==stage)
                and (v.get('query') if kind=='query' else item.get('selected') if kind=='choose' else item.get('id'))==identity,
                'Actual native action sequence changed')
        if kind=='query':
            q=v.get('query');require(q in ('@alex','@al','@nomatch'),'Unrequested native query')
            # Query helper replaces any live previous query at the original caret.
            base=token_blocks(a,[S.CANVAS]) if any(x.get('kind')=='choose' and x.get('selected')==S.CANVAS for x in items[:items.index(item)]) else a
            current=G['query_blocks'](base,q)
            n=v.get('native',{})
            require(F['rich'](n.get('blocks'))==F['rich'](current) and n.get('composing') is False
                    and R['integer'](n.get('revision')) and str(n.get('generation','')).isdigit(),
                    'Original query native rich buffer/caret ownership changed')
        elif kind=='choose':
            selected=item.get('selected');catalog=item.get('values');pub=item.get('publication',{});index=v.get('index')
            require(selected in (S.CANVAS,S.LIST) and isinstance(catalog,list) and R['integer'](index)
                    and index<len(catalog) and catalog[index]==dict(value=F['token'](selected),label='Untitled' if selected==S.CANVAS else 'Alex List'),
                    'Selected original file identity/index/label changed')
            require(pub.get('current') is True and pub.get('batchCurrent') is True and pub.get('settledBatch') is True
                    and pub.get('values')==catalog and pub.get('labels')==[c['label'] for c in catalog]
                    and pub.get('sequence')==v.get('sequence') and pub.get('revision')==v.get('revision'),
                    'File insertion borrowed an interim/foreign publication')
            require(set(F['identity'](c) for c in catalog)==set(F['BASE_IDS']+[S.CANVAS,S.LIST])
                and all(c.get('label')==leaf_label(c['value'],case) for c in catalog), 'Original complete choice catalog changed')
            S.query(pub.get('canvasFallback'))
            require(not pub.get('titleUsers'), 'No-user choice borrowed nested member authority')
            require(v.get('label')==catalog[index]['label'] and v.get('keyboard') is (selected==S.CANVAS)
                    and F['rich'](v.get('before',{}).get('blocks'))==F['rich'](current),
                    'File insertion lost native event/buffer custody')
            current=token_blocks(a,[S.CANVAS] if selected==S.CANVAS else [S.CANVAS,S.LIST])
        else:
            require(kind=='click' and item.get('id') in ('slack-list-close','slack-list-comments-back','slack-list-record-back','slack-list-comment-write',
                    'slack-list-comment-title-retry','slack-list-comment-send','slack-list-comments-refresh'),
                    'Unrequested native action')
            require(v.get('id')==item['id'],'Click receipt belongs to another original control')
    if row['stage'] in ('sent','readback'):current=[]
    require(F['rich'](p.get('expectedBlocks'))==F['rich'](current),'Original opaque/style/token wire changed')
    return copy.deepcopy(items)



def writes(rows,case,scope):
    send=case==LIFECYCLE;previous=[];posts=[]
    initial_page=X['original_page']();initial_page['messages'][0]['reply_users']=[G['SECOND']]
    require(rows[0]['model']['comments']['page']==initial_page,'Original acquired Page changed')
    counts={'draft':4,'query-ready':4,'canvas-inserted':4,'list-inserted':4,'reopened':8,'sending':12,'sent':12,'readback':16}
    if case==RECOVERY:counts={s:(4 if i<3 else 8 if i<5 else 12 if i<8 else 16) for i,s in enumerate(STAGES[case])}
    for row in rows:
        m,p=row['model'],row['proof'];stage=row['stage'];ledger=m['ledger'];compose=m['composeLedger'];reads=ledger['requests']
        C['validate_reads'](reads)
        require(reads[:len(previous)]==previous and len(reads)==counts[stage] and ledger.get('completed')==len(reads)
                and ledger.get('cancelled')==0 and ledger.get('held') is False,'Original List reads repeated/omitted/retired')
        previous=copy.deepcopy(reads);count=int(send and stage in ('sending','sent','readback'))
        require(len(compose['posts'])==count and compose['posts']==compose['prepared'] and compose['posts'][:len(posts)]==posts
                and len(compose['messages'])==len(compose['policies'])==len(compose['preflights'])==count
                and compose.get('held') is False and compose.get('heldPost') is (stage=='sending')
                and compose.get('published') is (stage=='readback'),'Title lifecycle sent/replayed an unrequested mutation')
        posts=copy.deepcopy(compose['posts'])
        if count:
            blocks=C['outgoing'](posts[0]);expected=next(r for r in rows if r['stage']=='reopened')['proof']['expectedBlocks']
            require(F['rich'](blocks)==F['rich'](expected),'Derived title was persisted into outgoing rich wire')
            R['_message'](compose['messages'][0],posts[0],scope)
            require(compose['preflights']==[dict(method='conversations.genericInfo',args={'updated_channels':'{"CCOMMENT01":0}'})],
                    'Original post destination preflight changed')
            policy=compose['policies'][0]
            require(policy.get('request')==dict(method='client.userBoot',args={'include_min_version_bump_check':'1'})
                    and policy.get('boot',{}).get('ok') is True and policy['boot'].get('team',{}).get('id')=='T1'
                    and policy['boot'].get('self',{}).get('id')=='ULIST0001' and policy.get('channel',{}).get('id')=='CCOMMENT01'
                    and policy.get('threadReadOnly') is False,'Original separate posting policy changed')
            if stage=='sending':
                attempt=p['journal'].get('attempt',{});record=attempt.get('record',{});args=posts[0]['args']
                require(attempt.get('capture')==p['journal']['capture'] and attempt.get('raw') is None
                        and record.get('client_msg_id')==args['client_msg_id'] and record.get('client_ts')==args['ts']
                        and record.get('blocks')==blocks and record.get('workspace')=='T1' and record.get('user')=='ULIST0001'
                        and record.get('target')==dict(document=m['initialDocument'],record=m['records'][READ['KNOWN']],channel='CCOMMENT01',root=READ['ROOT_TS']),
                        'Held send did not retain original durable request identity')
        if stage in ('sent','readback'):
            require(p.get('postRelease')==dict(delivered=True,cancelledBeforeReply=False) and F['rich'](p.get('sentBlocks'))==F['rich'](blocks),
                    'Original ACK/held rich buffer changed')
        elif stage!='sending':require(p['journal']['attempt'] is None,'Unexpected durable send attempt')
        if stage not in DEPARTED:
            accepted=stage in ('sent','readback')
            expected_ts=['1781524801.000001']+([compose['messages'][0]['ts']] if accepted else [])
            require(m['comments'].get('visibleTs')==expected_ts and [v.get('ts') for v in row['geometry'].get('rows',[])]==expected_ts,
                    'Original reply absent/duplicated or premature')
            page=copy.deepcopy(initial_page)
            if stage=='readback':page['messages'].append(compose['messages'][0])
            require(m['comments'].get('page')==page,'Canonical Page changed before original readback')



def candidate(proof,acquired,invalidated=False,plain=False):
    if not acquired:
        require(proof=={'unavailable':True} or proof==dict(rows=[],unavailableIds=[],excludedIds=[]),'Cold query fabricated file source')
        return
    require(proof.get('excludedIds')==[F['HOSTED']] and proof.get('unavailableIds')==([S.CANVAS] if invalidated else []),
        'Original candidate admission/invalidation changed')
    wanted=[S.LIST] if invalidated else [S.CANVAS,S.LIST]
    rows=proof.get('rows');require(isinstance(rows,list) and [r.get('id') for r in rows]==wanted,'Original candidate row order changed')
    search=json.loads(S.search_text())
    for row in rows:
        fid=row['id'];envelope=json.loads(S.singleton(fid,True)) if plain and fid==S.CANVAS else search
        raw=envelope['file'] if 'file' in envelope else next(r for r in envelope['files'] if r['id']==fid)
        effective=row.get('effective',{})
        require(row.get('source')==raw and row.get('receipt')==envelope,'Original candidate raw/envelope changed')
        channels=['CZMAP001','CAMAP001','CTAIL001'] if fid==S.CANVAS else ['CYMAP002','CBMAP002']
        require(effective.get('id')==fid and effective.get('title')==raw['title'] and effective.get('name')==raw['name']
            and effective.get('title_blocks')==raw.get('title_blocks') and effective.get('permalink')==raw['permalink']
            and effective.get('channels')==channels,'Original candidate/title projection or ordered channel union changed')

def completion_source(row,inputs,prior):
    m=row['model'];source=m['completionSource'];files=F['source'](source,m,False)
    require(not source.get('titleUsers'),'No-user title acquired nested member/policy fixture')
    queries=[i['value']['query'][1:] for i in inputs if i['kind']=='query']
    require(all(q=='alex' for q in queries),'Unexpected original query')
    for method in ('users/search','usergroups/search'):
        require([r['args']['query'] for r in source['requests'] if r['method']==method]==queries,
            'Original query family repeated/omitted')
    require([q['args']['query'] for q,_ in files]==(['alex'] if queries else [])
        and all(r['disposition']=='response' for _,r in files)
        and source['completed']==int(bool(queries)) and source['held']==source['cancelled']==0 and source['releases']==[],
        'Original exact-query cache or immediate file response changed')
    require(source.get('groups')==G['group_state'](G['group_rows']() if queries else G['group_rows']()[:1]),
        'Original group source changed')
    if prior:
        old=prior['model']['completionSource']
        for key in ('requests','responses','releases'):require(source[key][:len(old[key])]==old[key],'Original query receipts rewritten')
        require(source['seed']==old['seed'],'Original Boot/Init query source replaced')

def cold(row,case,index,prior):
    m,p=row['model'],row['proof'];v=m.get('coldFiles',{})
    require(all(v.get(k) is True for k in ('enabled','named','protectedFixture','identityCurrent'))
        and v.get('account')==m['account'] and v.get('epoch')==m['epoch'] and v.get('team')=='T1' and v.get('user')=='ULIST0001'
        and v.get('networkCalls')==0 and v.get('httpCapability') is False,'Original singleton read ownership changed')
    plain=case==RECOVERY and index>=2
    texts={fid:S.singleton(fid,plain and fid==S.CANVAS) for fid in (S.CANVAS,S.LIST)}
    require(v.get('futureResponseTexts')==texts and v.get('futureResponses')=={k:json.loads(t) for k,t in texts.items()},
        'Original future singleton bytes or ordering changed')
    seedtexts={fid:S.singleton(fid) for fid in (S.CANVAS,S.LIST)}
    require(p.get('emptyCanvas')==dict(case=case,label='Untitled',fileSeed=dict(initialResponseTexts=seedtexts,
        initialResponses={k:json.loads(t) for k,t in seedtexts.items()},canonicalSeeded=False)),
        'Wrong named original file fixture seed')
    counts=[0,0,0,1,2,2,2,2,3,3] if case==RECOVERY else [0]*8
    count=counts[index];requests=v.get('requests');responses=v.get('responses')
    require(isinstance(requests,list) and isinstance(responses,list) and len(requests)==len(responses)==count,
        'Independent reply files.info count changed')
    for i,(q,r) in enumerate(zip(requests,responses),1):
        mode='offline' if i==1 else 'held';text='null' if i==1 else S.singleton(S.CANVAS,True);raw=json.loads(text)
        require(q==dict(serial=i,id=S.CANVAS,method='files.info',httpMethod='POST',path='/api/files.info',
            args=dict(file=S.CANVAS,page='1',count='500',truncate='true'),mode=mode,plannedResponse=raw,responseText=text)
            and r==dict(serial=i,id=S.CANVAS,disposition=mode,raw=raw,responseText=text),
            'Original singleton factory/raw/byte receipt changed')
    done=0 if count==0 else 1 if index<6 else 2 if index<9 else 3
    if case==LIFECYCLE:done=0
    # Departure cancellation can settle at draft-replaced or before the closed
    # capture. Preserve actual RAII disposition; require retirement by close.
    if case==RECOVERY and index==5:require(v.get('completed') in (1,2),'Unexpected departure settlement')
    else:require(v.get('completed')==done,'Original file producer completion count changed')
    completed=v['completed'];cancelled=int(completed>=2)
    require(v.get('cancelled')==cancelled and v.get('settlements')==[
        dict(serial=i,cancelled=i==2) for i in range(1,completed+1)],'Original file settlement/cancellation changed')
    held=[]
    if case==RECOVERY and 4<=index<=6:
        closed=True if index==6 else False if index==4 else completed==2
        held=[dict(serial=2,id=S.CANVAS,receiverClosed=closed)]
    elif case==RECOVERY and index==8:held=[dict(serial=3,id=S.CANVAS,receiverClosed=False)]
    require(v.get('held')==held,'Held original file sender changed')
    releases=[]
    if case==RECOVERY and index>=7:releases.append(dict(serial=2,id=S.CANVAS,delivered=False,cancelledBeforeReply=True))
    if case==RECOVERY and index>=9:releases.append(dict(serial=3,id=S.CANVAS,delivered=True,cancelledBeforeReply=False))
    require(v.get('releases')==p.get('releases')==releases,'Original late/current file disposition changed')
    if plain:
        invalid=p.get('plainInvalidation',{});event=invalid.get('invalidation',{})
        require(invalid.get('kind')=='future-plain-response-and-actual-local-invalidation' and invalid.get('id')==S.CANVAS
            and invalid.get('previousResponseText')==S.singleton(S.CANVAS) and invalid.get('futureResponseText')==texts[S.CANVAS]
            and invalid.get('futureResponse')==json.loads(texts[S.CANVAS]) and event.get('id')==S.CANVAS
            and R['integer'](event.get('eventRevisionBefore')) and event.get('eventRevisionAfter')>event['eventRevisionBefore'],
            'Original local invalidation/future response boundary changed')
        candidate(event.get('before'),True);candidate(event.get('after'),True,True)
        require(v.get('invalidations')==[invalid],'Original file invalidation history changed')
        n=p.get('mountedCustody',{});before=n.get('before',{});after=n.get('after',{})
        require(before.get('ok') is True and after.get('ok') is True and after.get('sameNode') is True and after.get('sameSelection') is True
            and all(before.get(k)==after.get(k) for k in ('blocks','tokens','selection','focused'))
            and before.get('focused') is True and before.get('blocks')==p['expectedBlocks'],
            'Metadata change replaced original mounted editor/caret/wire')
        require(before.get('tokens')==[dict(value=t,label=leaf_label(t,case)) for t in R['nodes'](p['expectedBlocks'])
            if t.get('type') in ('canvas','file','user','channel','usergroup')],'Mounted original labels changed on invalidation')
    else:require(v.get('invalidations')==[] and 'plainInvalidation' not in p,'Unexpected metadata invalidation')
    if prior:
        old=prior['model']['coldFiles']
        for k in ('requests','responses','releases','settlements','invalidations'):
            require(v[k][:len(old[k])]==old[k],'Original file history rewritten')

def reply(row,case,scope):
    m,p=row['model'],row['proof'];job=m.get('titleUsers');stage=row['stage']
    if stage=='reopened':
        require(isinstance(job,dict) and job.get('current') is True and job.get('loading') is False and job.get('status')=='ready'
            and job.get('error') is None and job.get('blocks')==p['expectedBlocks'],'Independent restored reply title is not ready')
        tokens=[t for t in R['nodes'](p['expectedBlocks']) if t.get('type')=='canvas']
        require(job.get('tokens')==tokens and isinstance(job.get('scope'),str) and job['scope'],
            'Independent reply title token/Binding ownership changed')
        envelope=json.loads(S.singleton(S.CANVAS,True)) if case==RECOVERY else json.loads(S.search_text())
        S.observed(job.get('observed'),tokens,envelope,case==RECOVERY)
        d=next(d for d in m['draftStore']['drafts'] if d['scope']==scope)
        require(job.get('capture')==d['capture'] and job.get('sequence')==d['sequence'],'Reply borrowed query/another draft capture')
    elif stage in ('draft-failed','retry-held','return-held'):
        failed=stage=='draft-failed'
        require(isinstance(job,dict) and job.get('current') is True and job.get('observed') is None
            and job.get('blocks')==p['expectedBlocks'] and job.get('tokens')==[F['token'](S.CANVAS)]
            and job.get('loading') is (not failed) and job.get('status')==('failed' if failed else 'loading')
            and job.get('error')==('Title details could not load. Your saved draft is retained.' if failed else None),
            'Pending/failed reply borrowed completed query authority or lost durable bytes')
        n=row['geometry'];status=n.get('titleStatus',{})
        require(status.get('text')==('Title details could not load. Your saved draft is retained.' if failed else 'Loading title details...')
            and status.get('visible') is True,'Original pending/failure feedback missing')
        F['rectangle'](status['rect'],n['viewport'])
        retry=n.get('titleRetry')
        if failed:
            require(isinstance(retry,dict) and retry.get('id')=='slack-list-comment-title-retry' and retry.get('visible') is True
                and retry.get('hit') is True and retry.get('disabled') is False,'Retry not original enabled exposed control')
            F['rectangle'](retry['rect'],n['viewport'])
        else:require(retry is None,'Pending title request retained Retry')
    elif stage in DEPARTED:S.retired(job)

def retirement(rows):
    by={r['stage']:r for r in rows};original=by['retry-held'];old=original['model']['titleUsers']
    expected=dict(job=old,journal=original['proof']['journal'],fileSerial=2,fileSource=original['model']['coldFiles']['source'])
    for name in ('draft-replaced','draft-closed','draft-late','return-held','reopened'):
        row=by[name];p=row['proof']
        require(p.get('retiredDraft')==expected and p.get('journal')==expected['journal'],'Original retired reply source/durable provenance rewritten')
    late=by['draft-late']['model']
    require(late['coldFiles']['source']==expected['fileSource'],'Late reply installed canonical source into successor')
    fresh=by['return-held']['model']['titleUsers']
    require(fresh['scope']!=old['scope'] and fresh['blocks']==old['blocks'] and fresh['capture']==old['capture'],
        'Fresh reply reused retired Binding or changed durable capture')
    for name in ('return-held','reopened'):
        require(by[name]['proof'].get('returnedDraft')==dict(job=fresh,fileSerial=3),'Returned title provenance rewritten')
    require(by['reopened']['model']['titleUsers']['scope']==fresh['scope'],'Final reply borrowed old/query title owner')

def validate(fixture,receipt,directory,*,expected_case=None,dimensions=None,png=True):
    case=expected_case or receipt.get('case')
    require(case in STAGES and receipt.get('case')==case and receipt.get('fixture')==fixture and fixture in C['FIXTURES']
        and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True,'Wrong named empty reply terminal')
    ready=receipt.get('readiness',{});H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
    rows=receipt.get('stages',[]);require([r.get('stage') for r in rows]==list(STAGES[case]),'Missing/reordered empty reply stages')
    scope,a=owner(receipt,rows);prior=None;events=[]
    for index,row in enumerate(rows):
        m,p,g=row['model'],row['proof'],row['geometry']
        require(g.get('viewport')==rows[0]['geometry'].get('viewport') and g.get('noOverflow') is True,'Native viewport/overflow changed')
        events=inputs(row,case,a,events);completion_source(row,events,prior);cold(row,case,index,prior)
        acquired=index>0;invalid=case==RECOVERY and 2<=index<9;plain=case==RECOVERY and index==9
        for proof in (p.get('sourceFiles'),m['completionSource']['files']['source'],m['coldFiles']['source']):
            candidate(proof,acquired,invalid,plain)
        q=m.get('completion') or {};cg=g.get('completion') or {}
        if row['stage']=='query-ready':
            require(all(q.get(k) is True for k in ('current','batchCurrent','settledBatch','started')) and q.get('consumed') is False
                and cg.get('open') is True and q['labels']==[v['label'] for v in cg['rows']],'Query lacks current settled DOM')
            S.query(q.get('canvasFallback'));require(not q.get('titleUsers'),'No-user query borrowed member authority')
            require(set(F['identity'](v) for v in q['values'])==set(F['BASE_IDS']+[S.CANVAS,S.LIST]),'Mixed completion identities changed')
            require(all(v['label']==leaf_label(v['value'],case) for v in q['values']),'Original completion labels changed')
        reply(row,case,scope);custody(row,case,scope,prior);popup(row)
        if row['stage'] in ('sent','readback'):
            attempts=m.get('titleAcquisition',{}).get('rows',[])
            live=[v for v in attempts if v.get('current') is True and v.get('id')==S.CANVAS]
            require(len(live)==1,'Sent Canvas lacks unique independent transcript receipt')
            S.observed(live[0].get('titleUsers'),[F['token'](S.CANVAS)],json.loads(S.search_text()))
            painted(row,case,m['composeLedger']['messages'][0])
        path=Path(directory)/f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture',''))==path,'Foreign stage PNG path')
        if png:H['png_signature'](path,dimensions)
        prior=row
    first_choice=rows[2]['proof']['inputs'][1]
    require(first_choice['publication']==rows[1]['model']['completion'] and first_choice['values']==rows[1]['model']['completion']['values'],
        'Keyboard Choose replaced the original captured query publication')
    if case==RECOVERY:retirement(rows)
    writes(rows,case,scope)
    require(receipt.get('final')==rows[-1]['model'],'Final model changed after original capture')
    return receipt

def validator(case):
    require(case in STAGES,'Unknown empty Canvas reply case')
    return lambda fixture,receipt,directory,**kw:validate(fixture,receipt,directory,expected_case=case,**kw)
