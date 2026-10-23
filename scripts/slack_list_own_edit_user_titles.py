"""Independent original own-edit title receipt and durable custody oracle."""
import copy
import json
import runpy
from pathlib import Path
import slack_list_canvas_user_titles_source as T

BASE=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit.py')))
O=BASE['validate'].__globals__
original_received=O['received']
def received():
    value=original_received()
    value['blocks'][0]['elements'][0]['elements'].extend([
        dict(type='text',text=' Canvas: '),
        dict(type='canvas',file_id=T.CANVAS,url='https://fixture.invalid/files/FLISTFILE1',text='https://fixture.invalid/files/FLISTFILE1',style={'bold':True},opaque_title_occurrence={'index':0}),
        dict(type='text',text=' / '),
        dict(type='canvas',file_id=T.CANVAS,url='https://fixture.invalid/files/FLISTFILE1',text='https://fixture.invalid/files/FLISTFILE1',style={'italic':True},opaque_title_occurrence={'index':1})])
    return value
# Private execution globals preserve every baseline module and receipt byte.
O['received']=received
N,G,R,C,READ,H,S=(O[k] for k in ('N','G','R','C','READ','H','S'))
for _name in ('require','exact','reply_content','edited_content','fixture_ledger','one_message','geometry','boot','update_row','update_wire','native_button'):
    globals()[_name]=O[_name]
TS,SELF,CHANNEL,REPLY,CANCEL,CHANGE,READS,RELEASE=(O[k] for k in ('TS','SELF','CHANNEL','REPLY','CANCEL','CHANGE','READS','RELEASE'))
CASES=tuple('own-edit-title-'+s for s in ('known','hidden','recovery','held-cancel'))
STAGES={c:('draft','editing','cancelled','saving','saved','readback','reopened') for c in CASES[:2]}
STAGES[CASES[2]]=('draft','discovery','member-held','failure','retry-discovery','retry-member','editing','saving','saved','readback')
STAGES[CASES[3]]=('draft','editing','visit-retired','restored-discovery','restored-member','cancelled','late','fresh-discovery','fresh-member','fresh-edit','final-cancel')

def rich(blocks):
    require(isinstance(blocks,list),'Missing original rich edit')
    out=[]
    for b in blocks:
        require(b.get('type')=='rich_text' and isinstance(b.get('elements'),list),'Unsupported original edit block')
        sections=[]
        for section in b['elements']:
            require(section.get('type')=='rich_text_section' and isinstance(section.get('elements'),list),'Unsupported original edit section')
            leaves=[]
            for leaf in section['elements']:
                if leaf.get('type')=='text':
                    require(isinstance(leaf.get('text'),str),'Invalid original text')
                    leaves.extend(('text',c,{k:v for k,v in leaf.items() if k!='text'}) for c in leaf['text'])
                else:
                    require(leaf.get('type')=='canvas','Unexpected edit leaf')
                    leaves.append(('opaque',leaf))
            sections.append(({k:v for k,v in section.items() if k!='elements'},leaves))
        out.append(({k:v for k,v in b.items() if k!='elements'},sections))
    return out

def edited_content(blocks,suffix):
    expected=received()['blocks'][:1]
    expected[0]['elements'][-1]['elements'][-1]['text']+=suffix
    require(exact(rich(blocks),rich(expected)),'Original text/style/file wire or appended content changed')

def validate_standard(fixture, receipt, directory, *, expected_case='own-edit-title-known', dimensions=None, png=True):
    require(expected_case in ('own-edit-title-known','own-edit-title-hidden') and receipt.get('case') == expected_case
        and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True
        and receipt.get('fixture') == fixture and fixture in C['FIXTURES'], 'Wrong own-edit terminal receipt')
    ready = receipt.get('readiness', {})
    H['validate_ready'](ready, C['ready_tuple'](fixture, ready.get('nonce'), ready.get('pid')))
    rows = receipt.get('stages')
    require(isinstance(rows, list) and [r.get('stage') for r in rows] == list(STAGES[expected_case]), 'Missing own-edit stages')
    touched, unrelated = READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords']) == 73 and unrelated['count'] == 71, 'Original canonical inventory changed')
    initial = rows[0]['model']['comments']
    seed, reply = rows[0]['proof']['replySeed'], rows[0]['proof']['replyBlocks']
    scope = dict(workspace='T1', user=SELF, list=READ['LIST'], record=READ['KNOWN'], root=READ['ROOT_TS'],
        created=touched[READ['KNOWN']].get('date_created'), creator=touched[READ['KNOWN']].get('created_by'))
    require(exact(seed.get('scope'), scope) and exact(seed.get('blocks'), G['seed_blocks']())
        and seed.get('reopened') is True and R['integer'](seed.get('revision'), 1), 'Wrong original persisted reply seed')
    reply_content(reply, seed['blocks'])
    cancelled, edited = rows[1]['proof']['cancelledBlocks'], rows[3]['proof']['editedBlocks']
    edited_content(cancelled, CANCEL)
    edited_content(edited, CHANGE)
    previous_fixture, previous_reads, previous_draft = None, [], None
    initial_journal = rows[0]['journal']
    for index, row in enumerate(rows):
        p, model = row['proof'], row['model']
        m, f, edit = model['comments'], model['fixture'], model['edit']
        require(exact(p.get('source'), dict(received=received(), boot=boot()))
            and exact(p.get('replySeed'), seed) and exact(p.get('replyBlocks'), reply)
            and p.get('actions') == [], 'Original edit/reply source proof changed')
        require(exact(p.get('cancelledBlocks'), cancelled if index >= 1 else None)
            and exact(p.get('editedBlocks'), edited if index >= 3 else None)
            and exact(p.get('doubleSave'), dict(ok=True, disabled=True) if index >= 3 else None)
            and exact(p.get('release'), RELEASE if index >= 4 else None)
            and p.get('runtimeForgotten', False) is (index == 6), 'Edit/release/forget proof changed')
        require(m.get('account') == initial.get('account') and R['uuid'](m.get('account'))
            and exact(m.get('epoch'), initial.get('epoch')) and R['integer'](m.get('epoch'))
            and m.get('noSession') is False and m.get('noTransport') is False
            and exact(m.get('writes'), 0) and exact(m.get('networkCalls'), 0), 'Original disconnected owner changed')
        require(exact(m.get('document'), initial.get('initialDocument')) and exact(m.get('initialDocument'), initial.get('initialDocument'))
            and initial['initialDocument'].get('id') == READ['LIST']
            and exact(m.get('records'), touched) and exact(m.get('initialRecords'), touched)
            and exact(m.get('unrelated'), unrelated) and exact(m.get('initialUnrelated'), unrelated), 'Editing mutated canonical List records')
        comments = m.get('comments', {})
        require(comments.get('active') is True and comments.get('record') == READ['KNOWN']
            and comments.get('root') == READ['ROOT_TS'] and comments.get('loading') is False
            and comments.get('error') is None and m.get('detail', {}).get('record') == READ['KNOWN']
            and comments.get('visibleTs') == [TS] and comments.get('deleted') == [], 'Original visible comment owner changed')
        if index < 6:
            require(comments.get('generation') == initial['comments']['generation'], 'Edit replaced original comment visit')
        else:
            require(comments.get('generation') != initial['comments']['generation'], 'Reopen did not create a fresh comment visit')
        ledger = m.get('ledger', {})
        requests = ledger.get('requests')
        require(isinstance(requests, list) and len(requests) == READS[index]
            and exact(requests[:len(previous_reads)], previous_reads)
            and exact(ledger.get('completed'), READS[index]) and exact(ledger.get('cancelled'), 0)
            and ledger.get('held') is False, 'Unexpected read, missing completion or rewritten read history')
        C['validate_reads'](requests)
        require([q['method'] for q in requests] == ['files.info', 'lists.records.info', 'experiments.getByUser', 'conversations.replies'] * (READS[index] // 4)
            and all(q['args'].get('id', READ['KNOWN']) == READ['KNOWN']
                and (q['method'] != 'conversations.replies' or q['args']['ts'] == READ['ROOT_TS']) for q in requests)
            and exact(ledger.get('replies'), READS[index] // 4), 'Wrong original item/root or acquisition order')
        previous_reads = requests
        compose = m.get('composeLedger', {})
        require(all(compose.get(k) == [] for k in ('prepared', 'posts', 'messages', 'preflights', 'policies'))
            and all(compose.get(k) is False for k in ('published', 'held', 'heldPost')), 'Own edit sent a new reply or used reply dispatch')
        require(exact(m.get('reactionLedger'), dict(prepared=[], issued=[], outcomes=[], completed=0, held=False, networkCalls=0, fullReads=[])), 'Edit dispatched an unrelated reaction')
        ack = fixture_ledger(f, index, edited, previous_fixture)
        previous_fixture = f
        raw = one_message(comments.get('page', {}).get('messages'))
        require(exact(raw, received() if index < 5 else ack['message']), 'Immutable HTTP page changed before explicit readback')
        expected_page = copy.deepcopy(initial['comments']['page'])
        if index >= 5:
            expected_page['messages'] = [copy.deepcopy(ack['message']) if m.get('ts') == TS else m for m in expected_page['messages']]
        require(exact(comments.get('page'), expected_page)
            and exact(comments.get('hasMore'), initial['comments'].get('hasMore')), 'Root, pagination or original Page custody changed')
        known = one_message(comments.get('pageMessages'))
        expected = received() if index < 4 else ack['message']
        require(all(exact(known.get(k), expected[k]) for k in ('ts', 'thread_ts', 'user', 'text', 'blocks', 'reactions')),
            'Typed accepted comment body/context/reactions changed')
        require(exact(known.get('edited'), expected.get('edited')), 'Typed edit metadata did not match original ACK/readback')
        require(exact([m for m in comments['pageMessages'] if m.get('ts') != TS],
            [m for m in initial['comments']['pageMessages'] if m.get('ts') != TS]), 'Edit changed another typed comment/root')
        require(comments.get('reactions') == [dict(ts=TS, reactions=[dict(name='wave', count=1)])], 'Edit erased unrelated reaction')
        store = m.get('draftStore', {})
        drafts = store.get('drafts')
        require(isinstance(drafts, list) and len(drafts) == 1, 'Independent reply draft missing')
        draft = drafts[0]
        require(exact(draft.get('scope'), scope) and exact(draft.get('blocks'), reply)
            and all(draft.get(k) is False for k in ('busy', 'saving', 'loading', 'terminalPending', 'captureFailed'))
            and draft.get('attempt') is None and draft.get('error') is None and draft.get('native') is True
            and store.get('active') == f"{draft['id']}:{comments['generation']}:{draft['generation']}", 'Edit consumed or locked independent reply draft')
        S['inventory'](row['journal'], scope)
        S['paired'](draft, row['journal'])
        require(exact(row['journal'], initial_journal), 'Unchanged reply journal was rewritten or consumed')
        if previous_draft:
            require((draft['id'] == previous_draft['id']) is (index != 6)
                and (draft['generation'] == previous_draft['generation']) is (index != 6), 'Wrong original/reopened reply binding')
        previous_draft = draft
        require(edit.get('opening') is False and R['integer'](edit.get('events')), 'Edit preflight did not settle')
        attempts = edit.get('attempts')
        require(isinstance(attempts, list) and len(attempts) == int(index >= 3), 'Double Save created another attempt')
        if attempts:
            attempt = attempts[0]
            require(R['integer'](attempt.get('id'), 1) and attempt.get('ts') == TS
                and exact(attempt.get('blocks'), edited + received()['blocks'][1:])
                and attempt.get('phase') == ('sent' if index == 3 else 'confirmed')
                and exact(attempt.get('receipt'), None if index == 3 else ack)
                and exact(attempt.get('retryAt'), 0), 'Original attempt/ACK custody changed')
            if index > 3:
                require(exact(attempt, rows[4]['model']['edit']['attempts'][0]), 'Confirmed original attempt was rewritten')
        active = edit.get('active')
        if index in (1, 3):
            require(isinstance(active, dict) and active.get('current') is True and active.get('ts') == TS
                and R['integer'](active.get('generation'), 1) and R['integer'](active.get('revision'), 1)
                and exact(active.get('source'), received()) and exact(active.get('preserved'), received()['blocks'][1:])
                and exact(active.get('blocks'), cancelled if index == 1 else edited)
                and active.get('busy') is (index == 3) and active.get('composing') is False
                and active.get('attempt') == (attempts[0]['id'] if attempts else None)
                and active.get('scope') == f"list-edit:{comments['generation']}:{active['generation']}"
                and active.get('notice') == ('Saving the edited comment\u2026' if index == 3 else None), 'Original edit buffer/session changed')
        else:
            require(active is None, 'Cancelled/accepted edit session still active')
        geometry(row['geometry'], index, reply, cancelled, edited)
        path = Path(directory) / f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture', '')) == path, 'Foreign own-edit screenshot')
        if png:
            H['png_signature'](path, dimensions)
    require(exact(receipt.get('final'), rows[-1]['model']), 'Final own-edit model differs')
    return receipt



import slack_list_own_edit_titles_source as U
RECOVERED=' Recovered edit B \u65e5\U0001f98a'
DURABLE=' Durable title edit B \u65e5\U0001f98a'
COUNTS={CASES[0]:(0,0,0,0,0,0,0),CASES[1]:(0,0,0,0,0,0,0),
 CASES[2]:(0,1,2,2,3,4,4,4,4,4),CASES[3]:(0,0,0,1,2,2,2,3,4,4,4)}
RELEASED={CASES[0]:(0,)*7,CASES[1]:(0,)*7,CASES[2]:(0,0,1,2,2,3,4,4,4,4),
 CASES[3]:(0,0,0,0,1,1,2,2,3,4,4)}
READY_COUNTS={CASES[0]:(0,1,1,2,2,2,2),CASES[1]:(0,1,1,2,2,2,2),
 CASES[2]:(0,0,0,0,0,0,1,1,1,1),CASES[3]:(0,1,1,1,1,1,1,1,1,2,2)}


def durable_inventory(v,scope,seed,blocks,*,phase=None,ack=None):
    require(isinstance(v,dict) and set(v)=={'scope','ts','revision','capture','seed','blocks','attempt','retry_at'}
        and v['scope']==scope and v['ts']==TS and R['integer'](v['revision']) and v['retry_at']==0
        and v['seed']==seed and v['blocks']==blocks,'Original edit SQLite identity/seed/blocks changed')
    require((v['revision']==0 and v['capture']=='') or (v['revision']>0 and R['uuid'](v['capture'])),
            'Original durable capture identity missing')
    a=v['attempt']
    if phase is None:require(a is None,'Unissued/cleared edit acquired durable attempt');return
    require(isinstance(a,dict) and set(a)=={'id','record','capture','phase','raw','readback','retry_at'}
        and R['uuid'](a['id']) and R['uuid'](a['capture']) and a['phase']==phase and a['retry_at']==0
        and a['readback'] is None and a['raw']==ack
        and a['record']==dict(version=1,seed=seed,blocks=blocks+received()['blocks'][1:],text=G['display'](blocks)),
        'Original durable update/ACK changed')


def durable_seed(initial):
    return dict(workspace='T1',user=SELF,ts=TS,
        target=dict(document=initial['initialDocument'],record=initial['records'][READ['KNOWN']],channel=CHANNEL,root=READ['ROOT_TS']),
        http=initial['comments']['page'],editable=received()['blocks'][:1],preserved=received()['blocks'][1:])


def pending_geometry(row,reply,failed=False):
    g=row['geometry'];a=row['model']['edit']['active'];j=a['titleUsers'];v=g.get('viewport')
    require(g.get('ok') is True and g.get('editorAbsent') is True and g.get('editing') is False
        and g.get('scope')==a['scope'] and str(g.get('generation'))==str(a['generation'])
        and g.get('reply',{}).get('blocks')==reply and g['reply'].get('sameNode') is True
        and g['reply'].get('hidden') is True and g['reply'].get('inert') is True,
        'Pending title exposed/replaced original editor or reply')
    controls=g.get('controls');names=['slack-list-comment-edit-cancel','slack-list-comment-edit-save']
    if failed:names.append('slack-list-comment-edit-title-retry')
    require(isinstance(controls,list) and [c.get('id') for c in controls]==names,'Wrong pre-mount controls')
    for c in controls:
        require(c.get('visible') is True and c.get('hit') is True
                and c.get('disabled') is (c['id']=='slack-list-comment-edit-save'),'Pre-mount control guard changed')
        N['rectangle'](c.get('rect',{}),v)
    status=g.get('status',{})
    expected='Title details could not load. Your saved edit and reply are retained.' if failed else 'Loading title details...'
    require(status.get('id')=='slack-list-comment-edit-title-status' and status.get('visible') is True
            and status.get('text')==expected,'Wrong actual title loading/failure guidance')
    N['rectangle'](status.get('rect',{}),v)
    require(a.get('nativeReady') is False and j.get('current') is True and j.get('observed') is None
            and j.get('loading') is (not failed) and j.get('status')==('failed' if failed else 'loading'),
            'Pre-mount job manufactured readiness or retained stale observation')


def title_proofs(receipt,case):
    rows=receipt['stages'];initial=rows[0]['model']['comments'];scope=rows[0]['journal']['scope']
    seed=durable_seed(initial);source_history={};old_files=[];ready_history=[];old_proof=None
    for index,row in enumerate(rows):
        name=row['stage'];p=row['proof'];model=row['model'];source=model['titleSource']
        requests,policies=U.member_ledger(source,model['comments'],case,source_history)
        require(len(requests)==COUNTS[case][index] and sum(q['released'] for q in requests)==RELEASED[case][index],
                'Missing/replayed known-member acquisition or early release')
        require([q['response'] for q in requests if q['released'] and q['kind']=='member' and 'error' in q['response']]
                ==([dict(error='offline')] if case==CASES[2] and index>=3 else []),
                'Known-member failure/retry disposition changed')
        expected_ready=(case in CASES[:2] or index==0 or (case==CASES[2] and index>=6) or (case==CASES[3] and (index<3 or index>=9)))
        require(source.get('sourceReady') is expected_ready and source['knownDriver']['started']==(len(requests)+1)//2,
                'Known source readiness/actual recovery starts changed')
        if not source['pending']:
            require(source['knownDriver']['active']==0,'Completed original reader remains active')
        U.metadata(source,old_files);old_files=copy.deepcopy(source['files'])
        if index==0:
            require(p.get('initialTitleSource')==source,'Initial title source was not independently settled')
            require(p.get('initialTranscript')==model.get('transcriptTitles'),'Initial transcript receipt changed')
            transcript(p['initialTranscript'],p['initialTranscriptNative'],case,source)
        else:
            for key in ('initialTitleSource','initialTranscript','initialTranscriptNative'):
                require(p.get(key)==rows[0]['proof'].get(key),'Retained initial transcript/title witness rewritten')
        ready=p.get('titleReady')
        require(isinstance(ready,list) and len(ready)==READY_COUNTS[case][index]
                and ready[:len(ready_history)]==ready_history,'Pre-input title admission history changed')
        for n,entry in enumerate(ready):
            U.ready(entry,case,source,received()['blocks'][:1],recovered=(case==CASES[2] or (case==CASES[3] and n==1)))
        ready_history=copy.deepcopy(ready)
        a=model['edit'].get('active')
        if a is not None:
            j=a.get('titleUsers')
            require(isinstance(j,dict) and j.get('scope')==a.get('scope') and j.get('generation')==a.get('generation')
                    and j.get('ts')==TS and j.get('tokens')==U.tokens(received()['blocks'][:1]),
                    'Title job borrowed another edit owner/token')
            admitted=[v['job'] for v in ready if v['job']['scope']==a['scope']]
            if a.get('nativeReady') is True:
                require(len(admitted)==1,'Mounted edit lacks its own pre-input title receipt')
                old=admitted[0]
                require(all(j.get(k)==old.get(k) for k in ('scope','generation','ts','revision','blocks','tokens')),
                        'Mounted edit title provenance changed after input')
            else:require(j.get('blocks')==a.get('blocks') and j.get('revision')==a.get('revision'),
                         'Pending title source differs from durable edit')
        require(row['journal']==rows[0]['journal'],'Title work modified independent reply journal')
        inv=row['editInventory']
        phase=None;ack=None
        if name=='saving':
            blocks=p['editedBlocks'];phase='Pending';ack=None
            durable_inventory(inv,scope,seed,blocks,phase=phase,ack=ack)
        elif name in ('draft','cancelled','late','final-cancel','saved','readback','reopened'):
            durable_inventory(inv,scope,None,[])
        else:
            blocks=p['editedBlocks'] if (case==CASES[3] and name in ('editing','visit-retired','restored-discovery','restored-member')) else (
                p.get('cancelledBlocks',p.get('editedBlocks',received()['blocks'][:1])) if name=='editing' else received()['blocks'][:1])
            durable_inventory(inv,scope,seed,blocks)
        actions=p.get('titleActions')
        expected_actions=[]
        if case==CASES[2]:
            if index>=4:expected_actions.append('slack-list-comment-edit-title-retry')
            if index>=7:expected_actions.append('slack-list-comment-edit-save')
            if index>=9:expected_actions.append('slack-list-comments-refresh')
        elif case==CASES[3]:
            if index>=3:expected_actions.append('slack-list-record-comments')
            if index>=5:expected_actions.append('slack-list-comment-edit-cancel')
            if index>=10:expected_actions.append('slack-list-comment-edit-cancel')
        require(isinstance(actions,list) and [v.get('id') for v in actions]==expected_actions,
                'Missing/unrequested original title native action')
        for action in actions:native_button(action.get('native'),action.get('id'),row['geometry'].get('viewport',rows[0]['geometry']['viewport']))
        require(p.get('releases')==[q['disposition'] for q in requests if q['released']],
                'Original member callback release proof changed')
        if old_proof:
            for key in ('titleActions','releases'):
                require(p.get(key,[])[:len(old_proof.get(key,[]))]==old_proof.get(key,[]),'Original native actions/callbacks rewritten')
        old_proof=copy.deepcopy(p)
    return receipt


inert_text=O["inert_text"]
def extended_geometry(value, index, reply, cancelled, edited, marker):
    require(value.get('ok') is True and value.get('noOverflow') is True, 'Own-edit controls overflow or native capture failed')
    viewport = value.get('viewport')
    require(isinstance(viewport, list) and len(viewport) == 2 and all(R['integer'](v, 1) for v in viewport), 'Missing native viewport')
    editing = index in (1, 3)
    require(value.get('editing') is editing, 'Wrong native edit mount')
    controls = value.get('controls')
    names = ['slack-list-comment-edit-cancel', 'slack-list-comment-edit-save'] if editing else ['slack-list-comments-back', 'slack-list-comment-send']
    require(isinstance(controls, list) and [v.get('id') for v in controls] == names, 'Wrong edit/reply controls')
    for control in controls:
        require(control.get('visible') is True and control.get('hit') is True
            and control.get('disabled') is (index == 3), 'Pending/interactive edit controls changed')
        N['rectangle'](control.get('rect', {}), viewport)
    if index == 3:
        inert_text(value.get('text'), value.get('inertTextPaint'), viewport)
    else:
        require(value.get('inertTextPaint') is None, 'Inert paint evidence escaped the held-saving stage')
        N['text_proof'](value.get('text'), viewport)
    native = value.get('reply')
    require(isinstance(native, dict) and exact(native.get('blocks'), reply)
        and native.get('sameNode') is (index != 6)
        and native.get('hidden') is editing and native.get('inert') is editing
        and native.get('visible') is (not editing) and native.get('composing') is False
        and value.get('replyNotice') is None, 'Original mounted reply was changed, replaced, or exposed during edit')
    if not editing:
        N['rectangle'](native.get('rect', {}), viewport)
        require(value.get('edit') is None, 'Dismissed edit editor survived')
    else:
        active = value.get('edit', {})
        require(exact(active.get('blocks'), cancelled if index == 1 else edited)
            and active.get('editable') is (index == 1), 'Pending edit became mutable or native rich buffer changed')
        N['rectangle'](active.get('rect', {}), viewport)
    if index == 3:
        notice = value.get('notice')
        require(isinstance(notice, dict) and notice.get('text') == 'Saving the edited comment\u2026'
            and notice.get('visible') is True and notice.get('hit') is True, 'Held edit lost visible saving guidance')
        N['rectangle'](notice.get('rect', {}), viewport)
    else:
        require(value.get('notice') is None, 'Unexpected edit status after cancel/save')
    row = value.get('row')
    require(isinstance(row, dict) and isinstance(row.get('text'), str), 'Missing original comment DOM row')
    wanted = [v['text'] for b in (received()['blocks'][:1] if index < 4 else edited) for section in b['elements'] for v in section['elements'] if v.get('type')=='text']
    require(all(text in row['text'] for text in wanted) and 'Preserved original context' in row['text'], 'Edited body/context not present in native row')
    require((marker in row['text']) is (index >= 4) and CANCEL not in row['text'], 'Cancelled/unacknowledged edit leaked to transcript')
    return viewport



def standard_geometry(value,index,reply,cancelled,edited):
    return extended_geometry(value,index,reply,cancelled,edited,CHANGE)
geometry=standard_geometry
EXT_READS={CASES[2]:(4,8,8,8,8,8,8,12,12,16),CASES[3]:(4,8,8,12,12,12,12,16,16,16,16)}
EXT_POLICY={CASES[2]:(0,1,1,1,1,1,1,2,2,2),CASES[3]:(0,1,1,1,1,1,1,2,2,2,2)}
PENDING={'discovery','member-held','failure','retry-discovery','retry-member','restored-discovery','restored-member','fresh-discovery','fresh-member'}


def extended_fixture(f,case,index,blocks,previous):
    name=STAGES[case][index];issued=case==CASES[2] and index>=7
    require(f.get('enabled') is True and f.get('networkCalls')==0 and f.get('httpCapability') is False
            and f.get('received')==received() and f.get('boot')==boot(),'Own-edit fixture/source lost closed authority')
    expected=[dict(request=q,response=r) for _ in range(EXT_POLICY[case][index]) for q,r in O['policy_cycle']()]
    require(f.get('policy')==expected,'Original edit policy acquisition count/body changed')
    require(f.get('prepared')==f.get('updates') and len(f.get('updates',[]))==int(issued),'Repeated/omitted own edit dispatch')
    for request in f['updates']:update_wire(request,blocks)
    ack=dict(ok=True,channel=CHANNEL,ts=TS,message=update_row(blocks)) if issued else None
    require(f.get('responses')==([dict(disposition='accepted',raw=ack)] if issued else [])
        and f.get('futureRows')==([ack['message']] if issued else [])
        and f.get('held') is (name=='saving')
        and f.get('releases')==([RELEASE] if issued and name!='saving' else [])
        and f.get('published') is (name=='readback')
        and f.get('publishedRow')==(ack['message'] if name=='readback' else None),
        'Original Save/ACK/readback disposition changed')
    if previous:
        for k in ('policy','prepared','updates','responses','futureRows','releases'):
            require(f[k][:len(previous[k])]==previous[k],'Own-edit original protocol history rewritten')
    return ack


def validate_extended(fixture,receipt,directory,*,expected_case,dimensions=None,png=True):
    case=expected_case
    require(case in CASES[2:] and receipt.get('case')==case and receipt.get('fixture')==fixture
        and fixture in C['FIXTURES'] and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True,
        'Wrong own-edit title extended terminal')
    ready=receipt.get('readiness',{})
    H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
    rows=receipt.get('stages')
    require(isinstance(rows,list) and [r.get('stage') for r in rows]==list(STAGES[case]),'Missing own-edit title stages')
    touched,unrelated=READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords'])==73 and unrelated['count']==71,'Original canonical inventory changed')
    initial=rows[0]['model']['comments'];seed=rows[0]['proof']['replySeed'];reply=rows[0]['proof']['replyBlocks']
    scope=dict(workspace='T1',user=SELF,list=READ['LIST'],record=READ['KNOWN'],root=READ['ROOT_TS'],
        created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))
    require(seed.get('scope')==scope and seed.get('blocks')==G['seed_blocks']() and seed.get('reopened') is True
            and R['integer'](seed.get('revision'),1),'Original reply seed changed')
    reply_content(reply,seed['blocks'])
    editing=rows[6 if case==CASES[2] else 1]['proof']['editedBlocks']
    edited_content(editing,RECOVERED if case==CASES[2] else DURABLE)
    previous_reads=[];previous_fixture=None
    for i,row in enumerate(rows):
        name=row['stage'];p=row['proof'];model=row['model'];m=model['comments'];edit=model['edit'];f=model['fixture']
        require(p.get('source')==dict(received=received(),boot=boot()) and p.get('replySeed')==seed
                and p.get('replyBlocks')==reply and p.get('actions')==[],'Original edit/reply source proof changed')
        require(m.get('account')==initial.get('account') and R['uuid'](m.get('account')) and m.get('epoch')==initial.get('epoch')
                and m.get('noSession') is False and m.get('noTransport') is False
                and m.get('writes')==0 and m.get('networkCalls')==0,'Original disconnected owner changed')
        require(m.get('document')==initial.get('initialDocument') and m.get('initialDocument')==initial.get('initialDocument')
                and m.get('records')==touched and m.get('initialRecords')==touched
                and m.get('unrelated')==unrelated and m.get('initialUnrelated')==unrelated,'Original canonical List changed')
        reads=m.get('ledger',{});qs=reads.get('requests',[]);count=EXT_READS[case][i]
        require(len(qs)==count and qs[:len(previous_reads)]==previous_reads and reads.get('completed')==count
                and reads.get('cancelled')==0 and reads.get('held') is False and reads.get('replies')==count//4,
                'Original List reads repeated/omitted')
        C['validate_reads'](qs)
        require([q['method'] for q in qs]==['files.info','lists.records.info','experiments.getByUser','conversations.replies']*(count//4)
                and all(q['args'].get('id',READ['KNOWN'])==READ['KNOWN'] and (q['method']!='conversations.replies' or q['args']['ts']==READ['ROOT_TS']) for q in qs),
                'Unrelated List record/root read')
        previous_reads=qs
        compose=m.get('composeLedger',{})
        require(all(compose.get(k)==[] for k in ('prepared','posts','messages','preflights','policies'))
                and all(compose.get(k) is False for k in ('published','held','heldPost')),'Own edit sent independent reply')
        require(m.get('reactionLedger')==dict(prepared=[],issued=[],outcomes=[],completed=0,held=False,networkCalls=0,fullReads=[]),
                'Own edit mutated reactions')
        ack=extended_fixture(f,case,i,editing,previous_fixture);previous_fixture=f
        require(row['journal']==rows[0]['journal'],'Original reply journal mutated')
        S['inventory'](row['journal'],scope)
        drafts=m.get('draftStore',{}).get('drafts',[])
        require(len(drafts)==1 and drafts[0].get('scope')==scope and drafts[0].get('blocks')==reply
                and drafts[0].get('attempt') is None and drafts[0].get('error') is None,'Independent reply missing/consumed')
        S['paired'](drafts[0],row['journal'])
        a=edit.get('active');detached=name=='visit-retired'
        active=name in PENDING or name in ('editing','saving','fresh-edit')
        require(edit.get('opening') is False and R['integer'](edit.get('events'))
                and isinstance(a,dict) is active,'Wrong original edit lifecycle')
        if active:
            blocks=editing if (name in ('editing','saving','restored-discovery','restored-member')) else received()['blocks'][:1]
            require(a.get('current') is True and a.get('ts')==TS and a.get('source')==received()
                    and a.get('preserved')==received()['blocks'][1:] and a.get('blocks')==blocks
                    and a.get('composing') is False and a.get('busy') is (name=='saving'),
                    'Original active edit seed/content/currentness changed')
        attempts=edit.get('attempts');issued=case==CASES[2] and i>=7
        require(isinstance(attempts,list) and len(attempts)==int(issued),'Duplicate/unrequested edit attempt')
        if issued:
            attempt=attempts[0]
            require(R['integer'](attempt.get('id'),1) and attempt.get('ts')==TS and attempt.get('blocks')==editing+received()['blocks'][1:]
                    and attempt.get('phase')==('sent' if name=='saving' else 'confirmed')
                    and attempt.get('receipt')==(None if name=='saving' else ack) and attempt.get('retryAt')==0,
                    'Original update attempt/ACK changed')
        comments=m.get('comments',{})
        if not detached:
            require(comments.get('active') is True and comments.get('record')==READ['KNOWN'] and comments.get('root')==READ['ROOT_TS']
                    and comments.get('loading') is False and comments.get('error') is None,'Original comment ownership changed')
            require(one_message(comments['page']['messages'])==(ack['message'] if name=='readback' else received()),'Original HTTP row changed before readback')
            expected=ack['message'] if name in ('saved','readback') else received()
            actual=one_message(comments['pageMessages'])
            require(all(actual.get(k)==expected.get(k) for k in ('ts','thread_ts','user','text','blocks','reactions','edited')),
                    'Original accepted comment body/context/reaction changed')
        if name in PENDING:pending_geometry(row,reply,name=='failure')
        elif detached:
            g=row['geometry'];require(g.get('ok') is True and g.get('originalDetached') is True and g.get('originalRunning') is False
                and g.get('editorAbsent') is True and g.get('recordPresent') is True,'Original detached native capture not finished')
        else:
            role=1 if name in ('editing','fresh-edit') else 3 if name=='saving' else 4 if name in ('saved','readback') else 0
            content=received()['blocks'][:1] if name=='fresh-edit' else editing
            extended_geometry(row['geometry'],role,reply,content,editing,RECOVERED if case==CASES[2] else DURABLE)
        path=Path(directory)/f'{fixture}-{name}.png'
        require(Path(row.get('capture',''))==path,'Foreign original screenshot')
        if png:H['png_signature'](path,dimensions)
    require(receipt.get('final')==rows[-1]['model'],'Final original model differs')
    if case==CASES[3]:held_custody(rows)
    return receipt


def held_custody(rows):
    p=rows[-1]['proof'];initial=rows[0]['model']['comments'];before=rows[1];retired=rows[2]
    change=p.get('visitReplacement',{})
    require(change.get('kind')=='synthetic-list-visit-replacement'
        and change.get('account')==initial['account'] and change.get('epoch')==initial['epoch']
        and change.get('team')=='T1' and change.get('user')==SELF
        and R['integer'](change.get('beforeVisit'),1) and R['integer'](change.get('afterVisit'),1)
        and change['beforeVisit']!=change['afterVisit'] and change.get('before')==before['model']['edit']
        and change.get('oldCurrent') is False and change.get('oldVisible') is False and change.get('oldRetained') is True,
        'Synthetic original visit replacement broadened authority')
    require(change.get('after',{}).get('active') is None and retired['model']['edit'].get('active') is None,
            'Synthetic visit retirement kept active edit grant')
    require(rows[5]['model']['edit']==rows[6]['model']['edit'] and rows[5]['editInventory']==rows[6]['editInventory']
            and rows[5]['geometry']==rows[6]['geometry'],'Late member completion changed cancelled edit or native reply')
    finished=p.get('detachedFinished',{})
    require(finished.get('current') is False and finished.get('visible') is False and finished.get('retained') is False
        and finished.get('geometry')==retired['geometry'],'Old edit native holder survived retirement')
    require(p.get('durableBeforeRetirement')==before['editInventory']==retired['editInventory'],
            'Real detached capture changed original durable modified edit')
    restored=rows[4]['model']['edit']['active']
    require(p.get('heldOriginal')==restored and restored['blocks']==p['editedBlocks']
        and restored['scope']!=before['model']['edit']['active']['scope']
        and restored.get('nativeReady') is False,'Durable modified edit was not restored under fresh pending owner')
    invalid=p.get('invalidated',{})
    require(invalid.get('sourcePresent') is True and invalid.get('sourceReady') is False,
            'Known recovery fabricated source absence')
    for index in (5,6):
        row=rows[index];s=row['model']['titleSource']
        require(row['model']['edit']['active'] is None and row['editInventory']['seed'] is None and row['editInventory']['blocks']==[]
            and s['sourceReady'] is False and s['sourceRaw']==invalid['sourceRaw'] and s['native']==invalid['native']
            and s['knownDriver']['active']==0,'Cancel/late reply repopulated durable edit or canonical source')
    fresh=rows[9]['model']['edit']['active']
    require(p.get('freshSeed')==fresh and fresh['blocks']==received()['blocks'][:1] and fresh['scope']!=restored['scope']
        and rows[9]['model']['titleSource']['sourceReady'] is True,'Fresh edit resurrected cancelled modified blocks')
    require(rows[10]['model']['edit']['active'] is None and rows[10]['editInventory']['seed'] is None,
            'Final Cancel did not restore independent reply')


def validate(fixture,receipt,directory,*,expected_case,dimensions=None,png=True):
    require(expected_case in CASES,'Unknown own-edit title case')
    if expected_case in CASES[:2]:validate_standard(fixture,receipt,directory,expected_case=expected_case,dimensions=dimensions,png=png)
    else:validate_extended(fixture,receipt,directory,expected_case=expected_case,dimensions=dimensions,png=png)
    return title_proofs(receipt,expected_case)


def validator(case):
    require(case in CASES,'Unknown own-edit title case')
    return lambda fixture,receipt,directory,**kw:validate(fixture,receipt,directory,expected_case=case,**kw)


def transcript(value,native,case,source):
    rows=value.get('rows',[]);expected=U.tokens(received()['blocks'][:1])
    require(rows and all(r.get('id')==T.CANVAS and r.get('current') is True and r.get('retryable') is False for r in rows),
            'Original transcript title display not settled/current')
    for row in rows:
        observed=row.get('titleUsers');tokens=observed.get('titles',{}).get('tokens') if isinstance(observed,dict) else None
        require(isinstance(tokens,list) and len(tokens)==1 and tokens[0] in expected,'Transcript borrowed different original occurrence')
        U.observed(observed,case,source,tokens,'list-transcript-title-'+str(row['serial'])+'-file-title-users')
    require(native.get('ok') is True and native.get('ts')==TS and len(native.get('tokens',[]))==2,'Original transcript native title missing')
    for token in native['tokens']:
        require(token.get('id')==T.CANVAS and token.get('kind')=='canvas' and token.get('text')==U.label(case)
                and token.get('renderedText')==U.label(case) and token.get('visible') is True and token.get('hit') is True,
                'Original transcript title was hidden/fallback/occluded')
        N['rectangle'](token.get('rect',{}),native['viewport'])
        require(token.get('textRects') and all(x.get('hit') is True for x in token['textRects']),
                'Original transcript text fragments not exposed')
