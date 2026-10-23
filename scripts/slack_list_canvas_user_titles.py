"""Independent List Canvas user-title receipt, draft and transcript oracle."""
import copy
import json
import runpy
from pathlib import Path
import slack_list_canvas_user_titles_source as S

F=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_file_completion.py')))
G,R,X,C,H,READ=(F[k] for k in ('G','R','X','C','H','READ'))
require=S.require
STAGES={
 'user-title-known':('draft','query-ready','canvas-inserted','list-inserted','reopened','sending','sent','readback'),
 'user-title-hidden':('draft','query-ready','canvas-inserted','list-inserted','reopened','sending','sent','readback'),
 'user-title-recovery':('draft','discovery','member-held','failure','retry-discovery','retry-member','query-ready','canvas-inserted','reopened'),
 'user-title-draft-recovery':('draft','query-ready','canvas-inserted','draft-discovery','draft-member','draft-failed','retry-discovery','retry-member','draft-replaced','draft-closed','draft-late','return-discovery','return-member','reopened'),
 'user-title-retirement':('draft','discovery','member-held','query-replaced','late-query','second-discovery','second-member','record-replaced','closed','late-closed'),
}
ABSENT={'draft-discovery','draft-member','draft-failed','draft-replaced','draft-closed','draft-late','return-discovery','return-member','record-replaced','closed','late-closed'}
DEPARTED={'draft-replaced','draft-closed','draft-late','record-replaced','closed','late-closed'}
CLOSED={'draft-closed','draft-late','closed','late-closed'}


def action_plan(case):
    q=lambda stage,value:(stage,'query',value)
    choose=lambda stage,id:(stage,'choose',id)
    click=lambda id:(None,'click',id)
    initial=[q('query-ready','@alex'),choose('canvas-inserted',S.CANVAS)]
    if case in ('user-title-known','user-title-hidden'):
        return initial+[q('list-inserted','@alex'),choose('list-inserted',S.LIST),click('slack-list-comment-send'),click('slack-list-comments-refresh')],[0,1,2,4,4,5,5,6]
    if case=='user-title-recovery':
        return [q('discovery','@alex'),q('retry-discovery','@al'),choose('canvas-inserted',S.CANVAS)],[0,1,1,1,2,2,2,3,3]
    if case=='user-title-draft-recovery':
        return initial+[click(x) for x in ('slack-list-close','slack-list-comment-write','slack-list-comment-title-retry','slack-list-comments-back','slack-list-record-back','slack-list-close','slack-list-comment-write')],[0,1,2,4,4,4,5,5,7,8,8,9,9,9]
    return [q('discovery','@alex'),q('query-replaced','@nomatch'),q('second-discovery','@al'),click('slack-list-comments-back'),click('slack-list-record-back'),click('slack-list-close')],[0,1,1,2,2,3,3,5,6,6]


def validator(case):
    require(case in STAGES,'Unknown List title case')
    return lambda fixture,receipt,directory,**kw:validate(fixture,receipt,directory,expected_case=case,**kw)


def leaf_label(v,case):
    kind=v['type']
    if kind in ('canvas','file'):return S.label(case) if v['file_id']==S.CANVAS else 'Alex List'
    if kind=='user':
        # ordinary_members uses opening::member_label, not display-real-name prefs.
        return G['LABELS'].get(v['user_id'],'@'+v['user_id'])
    if kind=='channel':return G['LABELS'][v['channel_id']]
    if kind=='usergroup':return G['LABELS'][v['usergroup_id']]
    return v.get('text','')


def display(blocks,case):
    return '\n'.join(''.join(leaf_label(v,case) for v in s['elements']) for b in blocks for s in b['elements'])


def token_blocks(a,ids):
    values=[]
    for id in ids:values.extend([dict(type='text',text=' '),F['token'](id),dict(type='text',text=' ')])
    return G['with_tail'](a,values)


def owner(receipt,rows):
    # Reuse the original exact canonical/seed checks for the initial owned surface.
    initial,scope,a=G['owner_and_records'](receipt,rows[:1])
    for row in rows:
        m,p=row['model'],row['proof']
        require(all(m.get(k)==initial.get(k) for k in ('account','epoch','initialDocument','initialRecords','initialUnrelated'))
                and m.get('noSession') is False and m.get('noTransport') is False
                and m.get('writes')==m.get('networkCalls')==0,'Closed original List owner changed')
        require(p.get('seed')==rows[0]['proof']['seed'] and p.get('originalA')==a,
                'Original rich seed changed')
        if row['stage'] in CLOSED:
            require(all(k in m and m[k] is None for k in ('document','records','unrelated','detail'))
                    and m.get('comments',{}).get('active') is False,
                    'Closed List retained a current loaded view')
        else:
            require(m.get('document')==initial['initialDocument'] and m.get('records')==initial['records']
                    and m.get('unrelated')==initial['unrelated'],'Mounted canonical List records changed')
        if row['stage'] not in DEPARTED:
            c=m['comments']
            require(c.get('active') is True and c.get('record')==READ['KNOWN'] and c.get('root')==READ['ROOT_TS']
                    and c.get('loading') is False and c.get('error') is None,'Original comment target lost')
    return scope,a


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
                    and index<len(catalog) and catalog[index]==dict(value=F['token'](selected),label=S.label(case) if selected==S.CANVAS else 'Alex List'),
                    'Selected original file identity/index/label changed')
            require(pub.get('current') is True and pub.get('batchCurrent') is True and pub.get('settledBatch') is True
                    and pub.get('values')==catalog and pub.get('labels')==[c['label'] for c in catalog]
                    and pub.get('sequence')==v.get('sequence') and pub.get('revision')==v.get('revision'),
                    'File insertion borrowed an interim/foreign publication')
            source=row['model']['completionSource']
            S.query_users(pub.get('titleUsers'),case,pub['sequence'],source['requests'],source['responses'])
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


def nested_counts(row,case):
    stage=row['stage'];v=row['model']['completionSource']['titleUsers'];requests=v['requests']
    tables={
      'user-title-recovery':[(0,0),(1,0),(2,1),(2,2),(3,2),(4,3),(4,4),(4,4),(4,4)],
      'user-title-draft-recovery':[(0,0),(0,0),(0,0),(1,0),(2,1),(2,2),(3,2),(4,3),(4,3),(4,3),(4,4),(5,4),(6,5),(6,6)],
      'user-title-retirement':[(0,0),(1,0),(2,1),(2,1),(2,2),(3,2),(4,3),(4,3),(4,3),(4,4)]}
    count,released=tables[case][STAGES[case].index(stage)] if case in tables else (0,0)
    require(len(requests)==count and sum(x['released'] for x in requests)==released,'Nested acquisition count/release order changed')
    for r in requests:
        require(r['released'] is (r['serial']<=released),'Out-of-order original nested callback')
        if r['released']:
            failure=case in ('user-title-recovery','user-title-draft-recovery') and r['serial']==2
            require(('error' in r['response']) is failure,'Original nested failure replaced')
    require(row['proof']['releases']==[r['disposition'] for r in requests if r['released']],
            'Native release evidence differs from original callback ledger')
    expected_present=case in ('user-title-known','user-title-hidden','user-title-draft-recovery') or (case=='user-title-recovery' and stage in ('query-ready','canvas-inserted','reopened'))
    require(v['sourcePresent'] is expected_present,'Late/failed nested member became canonical authority')
    invalidated=case=='user-title-draft-recovery' and stage not in ('draft','query-ready','canvas-inserted','reopened')
    require(v.get('sourceReady') is (not invalidated),'Invalidated known source was treated as Ready')
    if case=='user-title-draft-recovery':
        known=v['knownDriver'];require(known['started']==(count+1)//2,'Known recovery omitted/replayed an original attempt')
        if stage in ('draft-failed','draft-late','reopened'):
            require(known['active']==0 and known['finished']==known['started'],'Completed known recovery still active')
        if invalidated:
            original=row['proof']['afterInvalidation']['completionSource']['titleUsers']
            require(original['sourceReady'] is False and original['sourcePresent'] is True
                    and original['sourceRaw']==v['sourceRaw'] and original['native']==v['native'],
                    'Invalidation/late reply replaced retained original known member')
    require(v.get('files')==[],'Nested recovery reacquired unchanged original file metadata')


def draft_receipt(row,case,raw,scope):
    m,p=row['model'],row['proof'];job=m.get('titleUsers');stage=row['stage']
    if stage=='reopened':
        require(isinstance(job,dict) and job.get('current') is True and job.get('loading') is False
                and job.get('status')=='ready' and job.get('error') is None
                and job.get('blocks')==p['expectedBlocks'],'Restored draft did not own its exact independent title job')
        tokens=[v for v in R['nodes'](p['expectedBlocks']) if v.get('type')=='canvas']
        require(job.get('tokens')==tokens,'Unrelated plain List token borrowed title admission')
        generation='list-draft-title-'+job['scope']+('' if case=='user-title-draft-recovery' else '-file-title-users')
        S.observed(job.get('observed'),case,raw,tokens,generation,m['completionSource']['titleUsers']['policy'])
        d=next(d for d in m['draftStore']['drafts'] if d['scope']==scope)
        require(job.get('capture')==d['capture'] and job.get('sequence')==d['sequence'],
                'Independent title job borrowed another draft capture/sequence')
    if case=='user-title-draft-recovery' and stage in ('draft-discovery','draft-member','draft-failed','retry-discovery','retry-member','return-discovery','return-member'):
        require(isinstance(job,dict) and job.get('current') is True and job.get('observed') is None
                and job.get('blocks')==p['expectedBlocks'],'Pending restored draft borrowed a completed query title receipt')
        failed=stage=='draft-failed'
        require(job.get('loading') is (not failed) and job.get('status')==('failed' if failed else 'loading')
                and job.get('error')==('Title details could not load. Your saved draft is retained.' if failed else None),
                'Independent title recovery failure/pending state changed')


def custody(row,case,scope,previous):
    m,p,g=row['model'],row['proof'],row['geometry'];stage=row['stage'];j=p['journal']
    require(j.get('scope')==scope and j.get('blocks')==p['expectedBlocks'] and R['uuid'](j.get('capture'))
            and R['integer'](j.get('revision'),1),'Original A journal scope/capture/rich draft lost')
    if previous:require(j['revision']>=previous['proof']['journal']['revision'],'Journal revision regressed')
    pending=case=='user-title-draft-recovery' and stage in ('retry-discovery','retry-member') or stage in ABSENT
    require(g.get('ok') is True and isinstance(g.get('viewport'),list),'Native capture rejected')
    if pending:
        require(g.get('editorAbsent') is True and g.get('editor') is None,'Unresolved/retired draft mounted an editor')
        if stage in CLOSED:require(g.get('dialogPresent') is False,'Closed List reappeared after late callback')
        elif stage in DEPARTED:require(g.get('record')=='R_LIST_001','Successor record was not actually opened')
        else:
            require(g.get('record')==READ['KNOWN'] and g.get('dialogPresent') is True,'Pending draft lost current record')
            status=g.get('titleStatus');require(isinstance(status,dict) and status.get('visible') is True,'Missing visible independent title status')
            if stage=='draft-failed':
                retry=g.get('titleRetry',{});require(retry.get('id')=='slack-list-comment-title-retry' and retry.get('visible') is True
                    and retry.get('hit') is True and retry.get('disabled') is False,'Original Retry is not actionable')
        require(j.get('attempt') is None,'Pending/retired title job sent a comment')
        return
    d=next((d for d in m['draftStore']['drafts'] if d.get('scope')==scope),None)
    require(isinstance(d,dict) and d.get('blocks')==j['blocks'] and d.get('capture')==j['capture']
            and d.get('savedRevision')==j['revision'] and d.get('attempt')==j['attempt']
            and d.get('native') is True and d.get('error') is None and all(d.get(k) is False for k in ('saving','loading','captureFailed','terminalPending')),
            'Native draft differs from original SQLite inventory')
    e=g.get('editor',{});n=g.get('native',{}).get('snapshot',{})
    sending=stage=='sending';sent=stage in ('sent','readback')
    require(g.get('noOverflow') is True and e.get('visible') is True and e.get('editable') is (not sending)
            and e.get('blocks')==n.get('blocks')==p['expectedBlocks'] and e.get('generation')==n.get('generation')==str(d['generation'])
            and n.get('composing') is False and R['integer'](n.get('revision')) and d.get('busy') is sending,
            'Original native editor/capture changed')
    require(e.get('sameNode') is (p.get('runtimeForgotten') is False),'Original editor remounted without durable reload')
    require(e.get('text') in ('','\n') if sent else e.get('text')==display(p['expectedBlocks'],case),'Visible rich title/content changed')
    F['rectangle'](e['rect'],g['viewport'])
    # rich_editor.resolved() reports atomic noneditable leaves only. Ordinary
    # links stay editable and remain covered by the exact blocks/text/journal.
    expected=[dict(value=v,label=leaf_label(v,case)) for v in R['nodes'](p['expectedBlocks']) if v.get('type') in ('canvas','file','user','channel','usergroup')]
    require(g['native'].get('tokens')==expected,'Original token occurrence/style/label changed')
    if sending:G['held_text'](g)
    elif not g.get('completion',{}).get('open') and not sent:X['editor_text'](g)
    controls=g.get('controls',[])
    require(len(controls)==4 and {c.get('id') for c in controls}=={'slack-list-comments-back','slack-list-comments-next','slack-list-comments-refresh','slack-list-comment-send'},
            'Original mounted comment controls changed')
    for c in controls:
        require(c.get('visible') is True and type(c.get('hit')) is bool
                and c.get('disabled') is ((sending or sent) if c['id']=='slack-list-comment-send' else False),
                'Original send/read control admission changed')
        F['rectangle'](c['rect'],g['viewport'])
        if not g.get('completion',{}).get('open'):require(c['hit'] is True,'Original comment control occluded')
    if stage=='reopened':
        before=p.get('beforeReopen',{});old=next(x for x in before['draftStore']['drafts'] if x['scope']==scope)
        require(p.get('runtimeForgotten') is True and d['id']!=old['id'] and d['generation']!=old['generation']
                and d['blocks']==old['blocks'] and d['capture']==old['capture'] and d['savedRevision']==old['savedRevision'],
                'Reopen did not restore original durable draft into a new native owner')


def retired(rows,case):
    by={r['stage']:r for r in rows}
    if case=='user-title-retirement':
        require(by['late-query']['model']['completion']==by['query-replaced']['model']['completion']
                and by['late-query']['geometry']['completion']==by['query-replaced']['geometry']['completion'],
                'Late original query response replaced successor suggestions')
        for name in ('record-replaced','closed','late-closed'):
            p=by[name]['proof'];require(p['journal']==p['originalJournal']==by['second-member']['proof']['journal'],
                                      'Retired query callback changed original durable draft')
        require(by['late-closed']['model']['completionSource']['titleUsers']['driver']['active']==0,'Retired query producer still active')
    if case=='user-title-draft-recovery':
        original=by['retry-member'];old=original['model']['titleUsers']
        for name in ('draft-replaced','draft-closed','draft-late','return-discovery','return-member','reopened'):
            r=by[name];p=r['proof'];require(p.get('retiredDraft')==dict(job=old,journal=original['proof']['journal'],memberSerial=4)
                and p['journal']==original['proof']['journal'],'Retired independent draft identity/journal changed')
        late=by['draft-late']['model'];job=late.get('titleUsers') or {}
        require(job.get('current') is not True and job.get('observed') is None
                and late['completionSource']['titleUsers']['knownDriver']['active']==0,'Late independent draft receipt adopted by successor')
        source=late['completionSource']['titleUsers'];invalid=by['draft-late']['proof']['afterInvalidation']['completionSource']['titleUsers']
        require(source.get('sourcePresent') is True and source.get('sourceReady') is False
                and source.get('sourceRaw')==invalid['sourceRaw'] and source.get('native')==invalid['native'],
                'Late known-member response changed retained source/readiness')
        fresh=by['return-discovery']['model']['titleUsers']
        require(fresh['scope']!=old['scope'] and fresh['blocks']==old['blocks'] and fresh['capture']==old['capture']
                and by['reopened']['model']['titleUsers']['scope']==fresh['scope'],'Returned draft reused retired Binding')
        for name in ('return-discovery','return-member','reopened'):require(by[name]['proof'].get('returnedDraft')==fresh,'Fresh return provenance rewritten')


def writes(rows,case,scope):
    send=case in ('user-title-known','user-title-hidden');previous=[];posts=[]
    initial_page=X['original_page']();initial_page['messages'][0]['reply_users']=[G['SECOND']]
    require(rows[0]['model']['comments']['page']==initial_page,'Original acquired Page changed')
    counts={'draft':4,'query-ready':4,'canvas-inserted':4,'list-inserted':4,'reopened':8,'sending':12,'sent':12,'readback':16}
    if case=='user-title-recovery':counts={s:(8 if s=='reopened' else 4) for s in STAGES[case]}
    if case=='user-title-draft-recovery':counts={s:(4 if i<3 else 8 if i<8 else 12 if i<11 else 16) for i,s in enumerate(STAGES[case])}
    if case=='user-title-retirement':counts={s:(4 if i<7 else 8) for i,s in enumerate(STAGES[case])}
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
                    and policy.get('boot',{}).get('ok') is True and policy['boot'].get('team',{}).get('id')==S.TEAM
                    and policy['boot'].get('self',{}).get('id')==S.SELF and policy.get('channel',{}).get('id')=='CCOMMENT01'
                    and policy.get('threadReadOnly') is False,'Original separate posting policy changed')
            if stage=='sending':
                attempt=p['journal'].get('attempt',{});record=attempt.get('record',{});args=posts[0]['args']
                require(attempt.get('capture')==p['journal']['capture'] and attempt.get('raw') is None
                        and record.get('client_msg_id')==args['client_msg_id'] and record.get('client_ts')==args['ts']
                        and record.get('blocks')==blocks and record.get('workspace')==S.TEAM and record.get('user')==S.SELF
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


def popup(row):
    g,m=row['geometry'],row['model'];cg=g.get('completion')
    if not isinstance(cg,dict):return # Honest pre-mount/closed capture has no query surface.
    q=m.get('completion')
    if cg.get('open'):
        require(isinstance(q,dict) and q.get('current') is True and q.get('consumed') is False
                and q.get('friendly') is False and q.get('sequence')==cg.get('sequence')
                and q.get('labels')==[v['label'] for v in cg.get('rows',[])],
                'Original query DOM is not owned by current publication')
        panel=cg.get('popup',{});require(panel.get('visible') is True,'Completion popup hidden')
        F['rectangle'](panel['rect'],g['viewport'])
        # Source failure/settled-empty feedback takes precedence over the initial
        # revision-zero loading publication; retained nonempty interim rows do not.
        expected_status=None
        if row['stage']=='failure':expected_status=F['FAILURE']
        elif row['stage'] in ('query-replaced','late-query'):expected_status='No matching mentions'
        elif q.get('revision')==0 and q.get('labels')==[]:
            require(cg.get('revision')==0 and q.get('settledBatch') is False,
                    'Loading feedback borrowed a settled query revision')
            expected_status='Searching mentions...'
        if expected_status is not None:
            require((cg.get('status') or {}).get('text')==expected_status,
                    'Original member query feedback changed')
        for index,v in enumerate(cg['rows']):
            require(v.get('id')==f"slack-list-channel-choice-{g['editor']['generation']}-{cg['sequence']}-{index}"
                    and v.get('type')=='button' and v.get('role')=='option' and v.get('selected')==str(index==0).lower()
                    and type(v.get('visible')) is bool and type(v.get('hit')) is bool,'Original completion node identity changed')
            if v.get('fullyVisible'):F['rectangle'](v['rect'],g['viewport'])
        if cg.get('status'):
            notice=cg['status'];require(notice.get('visible') is True and notice.get('hit') is True,'Original title feedback obscured')
            F['rectangle'](notice['rect'],g['viewport'])
    else:require(cg.get('popup') is None,'Retired completion popup retained')


def painted(row,case,message):
    proof=row['proof'].get('renderedFiles',{});g=row['geometry'];n=proof.get('before')
    require(proof.get('message')==message and isinstance(n,dict) and n==proof.get('after')
            and n.get('ok') is True and n.get('ts')==message['ts'] and n.get('viewport')==g['viewport'],
            'Transcript title proof lost original ACK/paint ownership')
    visible=[r for r in g['rows'] if r.get('ts')==message['ts']]
    require(len(visible)==1 and visible[0]['rect']==n.get('rowRect'),'Painted title detached from original reply row')
    clip=n.get('clip',{});F['rectangle'](clip,g['viewport'])
    tokens=n.get('tokens',[]);require(len(tokens)==2,'Missing/duplicate transcript title occurrence')
    for item,id in zip(tokens,(S.CANVAS,S.LIST)):
        label=S.label(case) if id==S.CANVAS else 'Alex List';r=item.get('rect',{})
        require(item.get('id')==id and item.get('kind')==F['token'](id)['type']
                and item.get('text')==item.get('renderedText')==label and n.get('rowText','').count(label)==1
                and visible[0].get('text','').count(label)==1,'Original transcript file label/wire identity changed')
        F['rectangle'](r,g['viewport'])
        full=lambda b:b['left']>=clip['left']-1 and b['right']<=clip['right']+1 and b['top']>=clip['top']-1 and b['bottom']<=clip['bottom']+1
        require(full(r) and item.get('visible') is True and item.get('hit') is True,'Painted title clipped/occluded')
        fragments=item.get('textRects');require(isinstance(fragments,list) and 0<len(fragments)<=128,'Missing original title text fragments')
        for f in fragments:
            b=f.get('rect',{});F['rectangle'](b,g['viewport'])
            require(full(b) and f.get('hit') is True and b['left']>=r['left']-1 and b['right']<=r['right']+1
                    and b['top']>=r['top']-1 and b['bottom']<=r['bottom']+1,'Title text range not visibly owned')


def validate(fixture,receipt,directory,*,expected_case=None,dimensions=None,png=True):
    case=expected_case or receipt.get('case')
    require(case in STAGES and receipt.get('case')==case and fixture in C['FIXTURES'] and receipt.get('fixture')==fixture
            and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True,'Wrong original List title terminal')
    ready=receipt.get('readiness',{});H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
    rows=receipt.get('stages',[]);require([r.get('stage') for r in rows]==list(STAGES[case]),'Missing/reordered List title stages')
    scope,a=owner(receipt,rows);baseline={};prior_inputs=[];previous=None
    for row in rows:
        m,p,g=row['model'],row['proof'],row['geometry']
        require(g.get('viewport')==rows[0]['geometry'].get('viewport') and g.get('noOverflow') is True,
                'Original capture viewport/overflow changed')
        prior_inputs=inputs(row,case,a,prior_inputs)
        src=m['completionSource'];S.member_ledger(src['titleUsers'],m,case,baseline)
        raw=S.completion(src,m,case,prior_inputs,baseline,G);nested_counts(row,case)
        fileproof=p.get('sourceFiles');acquired=bool([x for x in src['requests'] if x['method']=='search.autocomplete.files'])
        if acquired:
            require(fileproof.get('excludedIds')==[F['HOSTED']] and fileproof.get('unavailableIds')==[],'File eligibility/source changed')
            S.file_rows(fileproof.get('rows'),raw,[S.CANVAS,S.LIST])
        else:require(fileproof=={'unavailable':True} or fileproof==dict(rows=[],unavailableIds=[],excludedIds=[]),'Cold file authority fabricated')
        q=m.get('completion') or {};cg=g.get('completion') or {}
        if row['stage']=='query-ready':
            require(all(q.get(k) is True for k in ('current','batchCurrent','settledBatch','started')) and q.get('consumed') is False
                    and cg.get('open') is True and q['labels']==[v['label'] for v in cg['rows']],'Ready query lacks original settled DOM publication')
            S.query_users(q.get('titleUsers'),case,q['sequence'],src['requests'],src['responses'])
            require(set(F['identity'](x) for x in q['values'])==set(F['BASE_IDS']+[S.CANVAS,S.LIST]),'Wrong mixed completion identities')
            for choice in q['values']:
                require(choice.get('label')==leaf_label(choice['value'],case),'Mixed catalog name policy/title label changed')
        if row['stage']=='failure':require((cg.get('status') or {}).get('text')==F['FAILURE'] and q.get('batchCurrent') is False,'Missing original query failure')
        draft_receipt(row,case,raw,scope);custody(row,case,scope,previous);popup(row)
        if row['stage'] in ('sent','readback'):
            attempts=m.get('titleAcquisition',{}).get('rows',[])
            live=[v for v in attempts if v.get('titleUsers') is not None and v.get('current') is True]
            require(live,'Transcript painted user title without independent occurrence receipt')
            for attempt in live:
                S.observed(attempt['titleUsers'],case,raw,[F['token'](S.CANVAS)],'list-transcript-title-'+str(attempt['serial'])+'-file-title-users',src['titleUsers']['policy'])
            painted(row,case,m['composeLedger']['messages'][0])
        path=Path(directory)/f'{fixture}-{row["stage"]}.png';require(Path(row.get('capture',''))==path,'Foreign native PNG path')
        if png:H['png_signature'](path,dimensions)
        previous=row
    retired(rows,case);writes(rows,case,scope)
    require(receipt.get('final')==rows[-1]['model'],'Terminal model differs from original last capture')
    return receipt
