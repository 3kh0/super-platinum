"""Bounded List reply edit receipts, original ingress custody and native drafts."""
import copy
import math
from pathlib import Path
import runpy

A = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_ack_socket.py')))
I, R, C, READ, H = A['I'], A['R'], A['C'], A['READ'], A['H']
require, rectangle = R['require'], R['rectangle']
CHANNEL, FIRST, ROOT, SENT = 'CCOMMENT01', '1781524801.000001', '1781524800.000001', '1781525001.000001'
NOTICE = I['ERROR']
STAGES = {
    'edits': ('draft','rich','plain','cleared','duplicate','held','ignored-held','updated-held','released','unsupported','readback'),
    'edit-ack': ('draft','post-held','socket','edited','reacted','acknowledged','b-edited','echo','tail-edited','readback'),
}
EVENT_KEYS = ('read','loading','error','page','pageMessages','live','accepted','editCustody',
              'visibleTs','liveTs','acceptedTs','liveReactions','deleted','reactions','reveal')


def typed_edit(prior, text, blocks, edited, metadata, reactions=None):
    """Expected fixed ordinary fixture output, including source-generated keys."""
    result=copy.deepcopy(prior)
    # The admitted outer channel is an explicit projected write; it does not
    # rewrite the original nested witness or the HTTP Page's absent channel.
    result.update(type='message',channel=CHANNEL,text=text,blocks=copy.deepcopy(blocks),attachments=[],files=[],subtype=None,
                  latest_reply=None,reply_users=[],reply_users_count=None,reply_count=0,
                  no_display=False,_hidden_reply=True,slackbot_feels=None)
    for key in ('replies','language','crm_signal'):result.pop(key,None)
    result['edited']=copy.deepcopy(edited)
    if metadata is not None:result['metadata']=copy.deepcopy(metadata)
    if reactions is not None:result['reactions']=copy.deepcopy(reactions)
    return result


def changed(ts, text, **extra):
    body=dict(ts=ts,thread_ts=ROOT,user='U1',text=text)
    body.update(extra)
    return dict(type='message',subtype='message_changed',channel=CHANNEL,message=body)


def edit_frames():
    rich_frame=changed(FIRST,'Edited rich reply.',blocks=rich('Edited rich reply.',{'bold':True}),
        edited=dict(user='U1',ts='1781525100.000001'),metadata=dict(event_type='list-edit',event_payload=dict(keep=1,old=True)),opaque=dict(ignored=True))
    plain=changed(FIRST,'Edited plain reply.',edited=dict(ts='1781525102.000001'),
        metadata=dict(event_type='plain',event_payload=dict(only=2)))
    cleared=changed(FIRST,'Cleared rich reply.',blocks=[],reactions=[],edited=None,
        metadata=dict(event_type='cleared',event_payload=dict(only=3)))
    held=changed(FIRST,'Held edit reply.')
    unsupported=changed(FIRST,'Unsupported file body.',files=[dict(id='FUNSUPPORTED1',mode='hosted')])
    return rich_frame,plain,cleared,held,unsupported


def rich(text, style):
    return [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[dict(type='text', text=text, style=style)])])]


def b_blocks():
    return [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=[dict(type='text', text=text)])
            for text in A['B_TEXT'].split('\n')])]


def validator(case):
    require(case in STAGES, 'Unknown edit case')
    def validate_case(fixture, receipt, directory, **kwargs):
        return validate(fixture, receipt, directory, expected_case=case, **kwargs)
    return validate_case


def common(fixture, receipt, directory, expected_case, dimensions, png):
    case = receipt.get('case'); directory = Path(directory)
    require(case in STAGES and (expected_case is None or case == expected_case) and fixture in C['FIXTURES']
            and receipt.get('fixture') == fixture and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True,
            'Wrong edit receipt')
    ready = receipt.get('readiness', {})
    H['validate_ready'](ready, C['ready_tuple'](fixture, ready.get('nonce'), ready.get('pid')))
    rows = receipt.get('stages', [])
    require([row.get('stage') for row in rows] == list(STAGES[case]), 'Missing edit stages')
    initial = rows[0]['model']; seed = rows[0]['proof']['seed']; original_a = rows[0]['proof']['originalA']
    b = rows[6]['proof']['newerB'] if case == 'edit-ack' else b_blocks()
    A['rich_content'](seed, original_a, b)
    touched, unrelated = READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords']) == 73 and unrelated['count'] == 71
            and set(touched) == {READ['KNOWN'], READ['ALIAS']} and initial['initialDocument'].get('id') == READ['LIST']
            and touched[READ['KNOWN']].get('thread_ts') == ROOT, 'Wrong initial source inventory')
    scope = seed.get('scope')
    require(scope == dict(workspace='T1',user='U0',list=READ['LIST'],record=READ['KNOWN'],root=ROOT,
                         created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))
            and seed.get('reopened') is True and R['integer'](seed.get('revision'),1), 'Wrong original journal scope')
    page = initial['comments']['page']
    require(I['message'](page,ROOT) == dict(ts=ROOT,subtype='list_record_comment',
                slack_list=dict(list_id=READ['LIST'],list_record_id=READ['KNOWN']),unknown=dict(keep=True),text='Record comment root')
            and I['message'](page,FIRST).get('reactions') == I['source_reactions'](2), 'Original HTTP source changed')
    first_raw=dict(ts=FIRST,thread_ts=ROOT,user='U1',text='First page comment.',
        blocks=[dict(type='rich_text',elements=[dict(type='rich_text_section',elements=[dict(type='text',text='First page comment.',style=dict(bold=True))]),
                dict(type='rich_text_section',elements=[dict(type='text',text='Review ready \u4e2d\U0001f642.')])])],reactions=I['source_reactions'](2))
    require(page==dict(ok=True,channel=CHANNEL,messages=[I['message'](page,ROOT),first_raw],has_more=True,
                       response_metadata=dict(next_cursor=READ['CURSOR'])), 'Initial closed HTTP response changed')
    first_draft = initial['draftStore']['drafts'][0]; viewport = None
    for index,row in enumerate(rows):
        model,g,proof = row['model'],row['geometry'],row['proof']; comments = model['comments']
        ack = case == 'edit-ack'; pending = ack and 0 < index < 5; settled = ack and index >= 5; newer = ack and index >= 6
        content = b if newer else [] if settled else original_a
        require(proof.get('seed') == seed and proof.get('originalA') == original_a
                and proof.get('newerB') == (b if newer else None), 'Original native rich proof changed')
        require(model.get('account') == initial.get('account') and R['uuid'](model.get('account'))
                and model.get('epoch') == initial.get('epoch') and R['integer'](model.get('epoch'))
                and model.get('noSession') is True and model.get('noTransport') is True
                and type(model.get('writes')) is int and model['writes'] == 0
                and type(model.get('networkCalls')) is int and model['networkCalls'] == 0, 'Owner/isolation changed')
        require(model.get('document') == model.get('initialDocument') == initial['initialDocument']
                and model.get('records') == model.get('initialRecords') == touched
                and model.get('unrelated') == model.get('initialUnrelated') == unrelated
                and model.get('detail') == initial['detail'], 'Canonical List inventory changed')
        require(comments.get('active') is True and comments.get('record') == READ['KNOWN'] and comments.get('root') == ROOT
                and comments.get('hasMore') is (index != len(rows)-1) and comments.get('deleted') == []
                and all(comments.get(k) == initial['comments'].get(k) for k in ('generation','requestTs')), 'Original owner/cursor changed')
        store = model['draftStore']; entries = store.get('drafts', [])
        require(len(entries) == 1, 'Original draft lost/duplicated')
        draft,journal = entries[0],proof.get('journal',{})
        generation = rows[5]['model']['draftStore']['drafts'][0]['generation'] if settled else first_draft['generation']
        require(R['integer'](generation,1) and draft.get('id') == first_draft['id'] and draft.get('generation') == generation
                and (not settled or generation != first_draft['generation']) and draft.get('native') is True
                and store.get('active') == f"{draft['id']}:{comments['generation']}:{generation}"
                and draft.get('scope') == journal.get('scope') == scope
                and draft.get('blocks') == journal.get('blocks') == content
                and draft.get('capture') == journal.get('capture') and R['uuid'](draft.get('capture'))
                and draft.get('savedRevision') == journal.get('revision') and R['integer'](journal.get('revision'),1)
                and draft.get('attempt') == journal.get('attempt') and R['integer'](draft.get('sequence')), 'Draft/native independent journal disagree')
        require(draft.get('busy') is pending and all(draft.get(k) is False for k in ('saving','loading','captureFailed','terminalPending'))
                and draft.get('error') is None and draft.get('status') == ('Comment sent.' if ack and index == 5 else None), 'Draft capture/storage state changed')
        if not ack:
            require(journal == rows[0]['proof']['journal'] and draft == first_draft, 'Edit disturbed original A custody')
        elif pending:
            require(journal == rows[1]['proof']['journal'] and draft.get('sequence') == first_draft['sequence'], 'Edit changed pending original attempt')
        elif newer:
            require(journal == rows[6]['proof']['journal'] and draft == rows[6]['model']['draftStore']['drafts'][0], 'Edit disturbed newer B custody')
        if not pending: require(journal.get('attempt') is None, 'Unattempted/settled draft stayed locked')
        if ack and index in (5,6):
            prior = rows[index-1]['proof']['journal']
            require(journal['revision'] > prior['revision'] and journal['capture'] != prior['capture'], 'ACK/B did not persist new capture')
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('dialog') is True
                and g.get('comments') is True and g.get('compose') is True, 'Native edit surface unavailable')
        v = g.get('viewport')
        require(isinstance(v,list) and len(v)==2 and all(type(n) in (int,float) and math.isfinite(n) and n>0 for n in v), 'Bad viewport')
        if viewport is None: viewport=v
        require(v==viewport, 'Viewport changed')
        e = g.get('editor',{})
        require(e.get('visible') is True and e.get('editable') is (not pending) and e.get('blocks') == content
                and e.get('generation') == str(generation) and e.get('sameNode') is (not (ack and index==5))
                and e.get('bold') is (not settled), 'Native editor text/style/node changed')
        rectangle(e.get('rect'),v)
        require(e.get('text') in ('','\n') if ack and index==5 else e.get('text')==R['visible_text'](content), 'Native paragraph boundary/text changed')
        require(isinstance(g.get('active'),str), 'Native focus witness missing')
        controls=g.get('controls',[]); ids=[c['id'] for c in controls]
        expected_ids={'slack-list-comments-back','slack-list-comments-refresh','slack-list-comment-send'}
        if index != len(rows)-1:expected_ids.add('slack-list-comments-next')
        require(len(ids)==len(set(ids)) and set(ids)==expected_ids, 'Wrong native controls')
        for control in controls:
            require(control.get('visible') is True and control.get('hit') is True, 'Native control obscured');rectangle(control.get('rect'),v)
            disabled = pending or ack and index==5 if control['id']=='slack-list-comment-send' else comments.get('loading') if control['id'] in ('slack-list-comments-next','slack-list-comments-refresh') else False
            require(control.get('disabled') is disabled, 'Native control admission changed')
        if ack and index==5:require('Comment sent.' in g.get('notices',[]), 'ACK success notice missing')
        if comments.get('error') == NOTICE:require(NOTICE in g.get('notices',[]), 'Retained-content guidance missing')
        require(any(isinstance(n,str) and n.startswith('Loading comments') for n in g.get('notices',[])) is comments.get('loading'), 'Loading state differs from native notice')
        capture=directory/f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture',''))==capture, 'Foreign capture path')
        if png:H['png_signature'](capture,dimensions)
    require(receipt.get('final')==rows[-1]['model'], 'Final model changed')
    return rows,initial,scope,original_a,page


def requests_and_post(rows,case,scope,a,initial):
    counts=(4,4,4,4,4,8,8,8,8,8,12) if case=='edits' else (4,8,8,8,8,8,8,8,8,12)
    completed=(4,4,4,4,4,7,7,8,8,8,12) if case=='edits' else counts
    template=initial['ledger']['requests'];require(len(template)==4,'Wrong initial read count')
    raw=None;post=None
    if case=='edit-ack':
        raw=rows[1]['proof']['originalMessage'];posts=rows[1]['model']['composeLedger']['posts']
        require(len(posts)==1 and C['outgoing'](posts[0])==a,'Wrong original explicit send');post=posts[0]
        R['_message'](raw,post,scope)
        require(raw.get('ts')==SENT and raw.get('reactions')==[dict(name='wave',count=1,users=['U_RT_REACTOR'])], 'Original held ACK missing reaction source')
    for i,row in enumerate(rows):
        model,proof=row['model'],row['proof'];ledger=model['ledger'];compose=model['composeLedger'];comments=model['comments']
        reads=[dict(copy.deepcopy(r),serial=batch*4+j+1) for batch in range(counts[i]//4) for j,r in enumerate(template)]
        require(ledger.get('requests')==reads,'Missing/automatic/rewritten reads');C['validate_reads'](reads)
        for key,value in (('completed',completed[i]),('replies',counts[i]//4),('cancelled',int(case=='edits' and i>=7))):
            require(type(ledger.get(key)) is int and ledger[key]==value,'Read lifecycle changed')
        require(ledger.get('held') is (case=='edits' and i in (5,6,7)),'Held read owner changed')
        sent=case=='edit-ack' and i>0;pending=sent and i<5
        require(compose.get('prepared')==compose.get('posts')==([post] if sent else [])
                and compose.get('messages')==([raw] if sent else []) and compose.get('held') is False
                and compose.get('heldPost') is pending,'Original POST/response changed or replayed')
        require(proof.get('originalMessage')==(raw if sent else None),'Original immutable ACK proof changed')
        require(compose.get('preflights')==([dict(method='conversations.genericInfo',args={'updated_channels':'{"CCOMMENT01":0}'})] if sent else [])
                and len(compose.get('policies',[]))==int(sent),'Wrong posting observations')
        if sent:
            require(compose['policies']==rows[1]['model']['composeLedger']['policies'],'Policy source replaced')
            policy=compose['policies'][0]
            require(policy.get('request')==dict(method='client.userBoot',args={'include_min_version_bump_check':'1'})
                    and policy.get('boot',{}).get('ok') is True and policy['boot'].get('team',{}).get('id')==scope['workspace']
                    and policy['boot'].get('self',{}).get('id')==scope['user']
                    and policy.get('channel',{}).get('id')==CHANNEL and policy.get('threadReadOnly') is False,'Wrong original posting authority')
        if pending:
            journal=proof['journal'];attempt=journal.get('attempt');require(isinstance(attempt,dict),'Pending original attempt lost')
            record=attempt.get('record',{});target=record.get('target',{});args=post['args']
            require(attempt.get('raw') is None and attempt.get('capture')==journal['capture']==rows[0]['proof']['journal']['capture']
                    and record.get('version')==1 and type(record.get('version')) is int
                    and record.get('root')==I['typed'](I['message'](initial['comments']['page'],ROOT))
                    and record.get('workspace')==scope['workspace'] and record.get('user')==scope['user']
                    and record.get('blocks')==a and record.get('text')==raw.get('text')
                    and record.get('client_msg_id')==args['client_msg_id'] and record.get('client_ts')==args['ts']
                    and target.get('channel')==CHANNEL and target.get('root')==ROOT
                    and target.get('document')==initial['initialDocument'] and target.get('record')==initial['records'][READ['KNOWN']],
                    'Socket edit changed original POST/journal authority')
        require(R['integer'](comments.get('read'),1),'Read serial missing')
        if i:
            changed_at=(5,7,10) if case=='edits' else (1,9)
            prior=rows[i-1]['model']['comments']['read']
            require(comments['read']>prior if i in changed_at else comments['read']==prior,'Read serial changed outside explicit read/real edit cancellation')
    return raw,counts


def projection(row,expected_page,expected_live,expected_accepted,visible,body,custody,live_reactions):
    c,g=row['model']['comments'],row['geometry']
    require(c.get('pageMessages')==expected_page and c.get('live')==expected_live and c.get('accepted')==expected_accepted,
            'Typed edited message fields/body/scalars differ from projected contract')
    require(c.get('liveTs')==[m['ts'] for m in expected_live] and c.get('acceptedTs')==[m['ts'] for m in expected_accepted]
            and c.get('editCustody')==custody and c.get('liveReactions')==live_reactions,'Edit/reaction custody not retained or retired exactly')
    require(c.get('visibleTs')==visible and [r.get('ts') for r in g.get('rows',[])]==visible,'Edited reply missing/duplicated')
    messages={m['ts']:m for m in expected_page+expected_live+expected_accepted}
    expected=[dict(ts=ts,reactions=[dict(name=p['name'],count=p['count']) for p in messages[ts]['reactions']]) for ts in visible]
    require(c.get('reactions')==expected and [dict(ts=r.get('ts'),reactions=r.get('reactions')) for r in g['rows']]==expected,'Omitted/empty reaction semantics changed')
    for r in g['rows']:
        require(isinstance(r.get('text'),str) and r['text'] and all(type(p['count']) is int for p in r['reactions']),'Missing native body/pill')
        if r['ts'] in body:
            require(all(r['text'].count(text)==1 for text in body[r['ts']]),'Native edited content missing/duplicated')


def body_probe(row,ts,text,bold,italic,wave,full=True):
    g=row['geometry'];body=g.get('editBody',{})
    require(body.get('ts')==ts and isinstance(body.get('text'),str) and body['text'].count(text)==1
            and body.get('bold') is bold and body.get('italic') is italic
            and type(body.get('tools')) is int and body['tools']==0,'Native edited body/style/read-only contract changed')
    require(type(body.get('visible')) is bool and (not full or body['visible'] is True),'Edited body clipped')
    if full:rectangle(body.get('rect'),g['viewport'])
    pills=body.get('pills',[])
    require([p.get('name') for p in pills]==(['wave'] if wave else []),'Wrong native reaction pills')
    for p in pills:
        require(p.get('visible') is True and p.get('tag')=='SPAN','Reaction pill clipped or became interactive')
        rectangle(p.get('rect'),g['viewport'])


def canonical_page(page,raw):
    result=copy.deepcopy(page)
    for i,message in enumerate(result['messages']):
        if message['ts']==raw['ts']:result['messages'][i]=copy.deepcopy(raw);break
    else:result['messages'].append(copy.deepcopy(raw))
    result.update(has_more=False,response_metadata=dict(next_cursor=''))
    return result


def witness(frame):
    if frame.get('subtype') not in ('message_changed','message_replied'):return None
    raw=frame['message']
    return dict(kind='Changed' if frame['subtype']=='message_changed' else 'Replied',outerChannel=frame['channel'],
                typedChannel=raw.get('channel',frame['channel']),typedTs=raw['ts'],raw=copy.deepcopy(raw))


def expected_events(rows,specs):
    return [dict(stage=stage,team=team,generation=generation,frame=copy.deepcopy(frame),parsed=True,witness=witness(frame),changed=changed_flag,
                 before={key:rows[before]['model']['comments'][key] for key in EVENT_KEYS},
                 after={key:rows[after]['model']['comments'][key] for key in EVENT_KEYS})
            for stage,team,generation,frame,changed_flag,before,after in specs]


def validate_edits(rows,page):
    frames=edit_frames();root=I['typed'](I['message'](page,ROOT));first=I['typed'](I['message'](page,FIRST))
    rich_frame,plain,cleared,held,unsupported=frames
    rich_message=typed_edit(first,'Edited rich reply.',rich_frame['message']['blocks'],rich_frame['message']['edited'],rich_frame['message']['metadata'])
    plain_message=typed_edit(rich_message,'Edited plain reply.',[],plain['message']['edited'],plain['message']['metadata'])
    cleared_message=typed_edit(plain_message,'Cleared rich reply.',[],None,cleared['message']['metadata'],[])
    held_message=typed_edit(cleared_message,'Held edit reply.',[],None,cleared['message']['metadata'])
    raw=dict(type='message',channel=CHANNEL,ts=FIRST,thread_ts=ROOT,user='U1',text='Canonical readback reply.',
             blocks=rich('Canonical readback reply.',{'italic':True}),reactions=[dict(name='wave',count=1,users=['U1'])],edited=dict(ts='1781525109.000001'))
    readback=canonical_page(page,raw)
    for i,row in enumerate(rows):
        c,g,p=row['model']['comments'],row['geometry'],row['proof']
        expected=(first,rich_message,plain_message,cleared_message,cleared_message,cleared_message,cleared_message,
                  held_message,held_message,held_message,I['typed'](raw))[i]
        projection(row,[root,expected],[],[],[FIRST],{FIRST:[expected['text']]},[],[])
        require(c.get('page')==(readback if i==10 else page),'Original HTTP Page was rewritten')
        require(c.get('loading') is (i in (5,6)) and c.get('error')==(NOTICE if i in (7,8,9) else None)
                and c.get('reveal') is None,'Edit read/notice/reveal state changed')
        require(row['model']['composeLedger'].get('published') is False,'Unexpected POST readback publication')
        require(p.get('serverResponse')==(readback if i==10 else None)
                and p.get('heldResponse')==(page if i>=5 else None),'Original held/canonical source changed')
        release=dict(delivered=False,cancelledBeforeReply=True) if i>=8 else None
        require(p.get('release')==release,'Held read cancellation was not retained')
        if release:require(p['release']['delivered'] is False and p['release']['cancelledBeforeReply'] is True,'Wrong release types')
        if 1<=i<=4:require(g['active']==rows[0]['geometry']['active'],'Edit moved original native focus')
        if 6<=i<=9:require(g['active']==rows[5]['geometry']['active'],'Held edit/refusal moved native focus')
        body_probe(row,FIRST,expected['text'],i in (0,1),i==10,len(expected['reactions']) and expected['reactions'][0]['count'],full=i!=0)
    specs=[(name,'T1',7,frame,change,before,after) for name,frame,change,before,after in (
        ('rich',rich_frame,True,0,1),('plain',plain,True,1,2),('cleared',cleared,True,2,3),('duplicate',cleared,False,3,3),
        ('held-duplicate',cleared,False,5,5))]
    for name,team,generation,outer,ts,thread,nested,kind in (
        ('wrong-team','T_OTHER',7,CHANNEL,FIRST,ROOT,None,'message_changed'),
        ('stale-generation','T1',6,CHANNEL,FIRST,ROOT,None,'message_changed'),
        ('foreign-channel','T1',7,'COTHER',FIRST,ROOT,None,'message_changed'),
        ('conflicting-channel','T1',7,CHANNEL,FIRST,ROOT,'COTHER','message_changed'),
        ('wrong-root','T1',7,CHANNEL,FIRST,'1781524998.000001',None,'message_changed'),
        ('unknown-reply','T1',7,CHANNEL,'1999999999.000001',ROOT,None,'message_changed'),
        ('root','T1',7,CHANNEL,ROOT,ROOT,None,'message_changed'),
        ('replied','T1',7,CHANNEL,FIRST,ROOT,None,'message_replied')):
        frame=changed(ts,'Ignored edit.',thread_ts=thread);frame.update(channel=outer,subtype=kind)
        if nested:frame['message']['channel']=nested
        specs.append((name,team,generation,frame,False,5,5))
    specs.extend([('updated-held','T1',7,held,True,6,7),('unsupported','T1',7,unsupported,False,8,8)])
    return expected_events(rows,specs)


def validate_edit_ack(rows,page,raw):
    root,first=I['typed'](I['message'](page,ROOT)),I['typed'](I['message'](page,FIRST))
    original=I['typed'](raw)
    edited=typed_edit(original,'Socket edited reply.',rich('Socket edited reply.',{'bold':True}),dict(ts='1781525200.000001'),None)
    removed=copy.deepcopy(edited);removed['reactions']=[]
    tail=typed_edit(removed,'Accepted tail edited.',rich('Accepted tail edited.',{'italic':True}),removed['edited'],None)
    canonical=dict(type='message',channel=CHANNEL,ts=SENT,thread_ts=ROOT,user=raw['user'],client_msg_id=raw['client_msg_id'],
                   text='Canonical sent reply.',blocks=rich('Canonical sent reply.',{'italic':True}),reactions=[])
    readback=canonical_page(page,canonical)
    for i,row in enumerate(rows):
        c,g,p=row['model']['comments'],row['geometry'],row['proof']
        current=original if i<3 else edited if i==3 else removed if i<8 else tail if i==8 else I['typed'](canonical)
        visible=[FIRST]+([SENT] if i>=2 else [])
        expected_page=[root,first]+([I['typed'](canonical)] if i==9 else [])
        live=[current] if 2<=i<=4 else [];accepted=[current] if 5<=i<=8 else []
        texts=C['REACTION_TEXT'].split('\n') if i==2 else [current['text']]
        projection(row,expected_page,live,accepted,visible,{SENT:texts} if i>=2 else {},[SENT] if i in (3,4) else [],[SENT] if i==4 else [])
        require(c.get('page')==(readback if i==9 else page) and p.get('serverResponse')==(readback if i==9 else None)
                and p.get('heldResponse') is None,'Original Page/post readback source changed')
        require(c.get('loading') is False and c.get('error') is None and row['model']['composeLedger'].get('published') is False,
                'Edit/ACK issued or disturbed read/publication')
        release=dict(delivered=True,cancelledBeforeReply=False) if i>=5 else None
        require(p.get('release')==release,'Actual original held ACK not released exactly')
        if release:require(p['release']['delivered'] is True and p['release']['cancelledBeforeReply'] is False,'Invalid ACK release types')
        reveal=c.get('reveal')
        require(reveal==rows[5]['model']['comments']['reveal'] if i>=5 else reveal is None,'Edit/echo changed reveal')
        if i>=5:
            require(isinstance(reveal,list) and len(reveal)==2 and R['integer'](reveal[0],1) and reveal[1]==SENT
                    and g.get('reveal',{}).get('serial')==str(reveal[0]) and g['reveal'].get('ts')==SENT,'ACK reveal identity missing')
        if 2<=i<=4:require(g['active']==rows[1]['geometry']['active'],'Edit/reaction moved held draft focus')
        if 6<=i<=8:require(g['active']==rows[6]['geometry']['active']=='slack-list-comment-draft','Edit/echo moved newer B focus')
        text='First page comment.' if i<2 else C['REACTION_TEXT'].split('\n')[0] if i==2 else current['text']
        body_probe(row,FIRST if i<2 else SENT,text,i<=7,i>=8,2 if i<2 else 1 if i in (2,3) else 0,full=i>=3)
    edit_frame=changed(SENT,'Socket edited reply.',user=raw['user'],blocks=rich('Socket edited reply.',{'bold':True}),edited=dict(ts='1781525200.000001'))
    tail_frame=changed(SENT,'Accepted tail edited.',user=raw['user'],blocks=rich('Accepted tail edited.',{'italic':True}))
    reaction=dict(type='reaction_removed',user='U_RT_REACTOR',reaction='wave',item=dict(type='message',channel=CHANNEL,ts=SENT))
    specs=[('socket','T1',7,raw,True,1,2),('edited','T1',7,edit_frame,True,2,3),('reacted','T1',7,reaction,True,3,4),
           ('echo','T1',7,raw,False,6,6),('tail-edited','T1',7,tail_frame,True,7,8)]
    return expected_events(rows,specs)


def validate(fixture,receipt,directory,*,expected_case=None,dimensions=None,png=True):
    rows,initial,scope,a,page=common(fixture,receipt,directory,expected_case,dimensions,png)
    case=receipt['case'];raw,_=requests_and_post(rows,case,scope,a,initial)
    events=validate_edits(rows,page) if case=='edits' else validate_edit_ack(rows,page,raw)
    require(receipt.get('editEvents')==events and all(e.get('parsed') is True and type(e.get('changed')) is bool
            and type(e.get('generation')) is int for e in receipt['editEvents']), 'Original parsed frame/witness or mutation evidence changed')
    return receipt
