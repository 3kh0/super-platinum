"""Closed raw-base family removals, with immutable per-child dispatch custody."""
import copy
import math
from pathlib import Path
import runpy
X = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_reaction_actions.py')))
E, A, I, R, C, READ, H = (X[k] for k in ('E','A','I','R','C','READ','H'))
require, rectangle, exact = X['require'], X['rectangle'], X['exact']
CHANNEL, FIRST, ROOT, USER = X['CHANNEL'], X['FIRST'], X['ROOT'], 'U0'
OWN = ('thumbsup','thumbsup::skin-tone-3','thumbsup::skin-tone-2','thumbsup::skin-tone-4')
SELECTED, ALIAS = 'thumbsup::skin-tone-6', '+1::skin-tone-3'
STAGES = {
 'reaction-family': ('draft','remove-held','duplicate','socket-removed','first-ack','second-ack','third-ack','read-held','final-ack','read-released'),
 'reaction-family-failures': ('draft','mixed','blocked','readback','rate-limited','rate-expired','fresh','retry-held','retry-first-ack','retry-done')}
READS = {'reaction-family': (4,8,8,8,8,8,8,12,12,12), 'reaction-family-failures':(4,8,8,12,16,16,20,24,24,24)}
PENDING = 'Updating the reaction family. Your draft is unchanged.'
NOTICE = I['ERROR']
GLYPHS = dict(zip(OWN+(SELECTED,ALIAS,'wave'),
 ('\U0001f44d','\U0001f44d\U0001f3fc','\U0001f44d\U0001f3fb','\U0001f44d\U0001f3fd',
  '\U0001f44d\U0001f3ff','\U0001f44d\U0001f3fc','\U0001f44b')))


def validator(case):
    require(case in STAGES, 'Unknown family case')
    def bound(fixture, receipt, directory, **kwargs):
        return validate(fixture, receipt, directory, expected_case=case, **kwargs)
    return bound


def common(fixture, receipt, directory, case, dimensions, png):
    directory = Path(directory)
    require(case in STAGES and receipt.get('case') == case
            and fixture in C['FIXTURES'] and receipt.get('fixture') == fixture
            and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True, 'Wrong reaction action receipt')
    ready = receipt.get('readiness', {})
    H['validate_ready'](ready, C['ready_tuple'](fixture, ready.get('nonce'), ready.get('pid')))
    rows = receipt.get('stages', [])
    require([r.get('stage') for r in rows] == list(STAGES[case]), 'Missing reaction action stages')
    initial = rows[0]['model']; proof0 = rows[0]['proof']; seed = proof0['seed']; a = proof0['originalA']
    A['rich_content'](seed, a, E['b_blocks']())
    touched, unrelated = READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords']) == 73 and unrelated['count'] == 71
            and set(touched) == {READ['KNOWN'], READ['ALIAS']}
            and initial['initialDocument'].get('id') == READ['LIST']
            and touched[READ['KNOWN']].get('thread_ts') == ROOT, 'Wrong original source inventory')
    scope = dict(workspace='T1', user=USER, list=READ['LIST'], record=READ['KNOWN'], root=ROOT,
                 created=touched[READ['KNOWN']].get('date_created'), creator=touched[READ['KNOWN']].get('created_by'))
    require(seed.get('scope') == scope and seed.get('reopened') is True
            and R['integer'](seed.get('revision'), 1), 'Wrong restored journal scope')
    store0 = initial['draftStore']; draft0 = store0['drafts'][0]; journal0 = proof0['journal']
    generation = draft0['generation']; viewport = None
    require(R['integer'](generation, 1), 'Missing native generation')
    previous_draft = None
    for row in rows:
        model, g, proof = row['model'], row['geometry'], row['proof']; comments = model['comments']
        require(model.get('account') == initial.get('account') and R['uuid'](model.get('account'))
                and model.get('epoch') == initial.get('epoch') and R['integer'](model.get('epoch'))
                and model.get('noSession') is True and model.get('noTransport') is True
                and type(model.get('writes')) is int and model['writes'] == 0
                and type(model.get('networkCalls')) is int and model['networkCalls'] == 0, 'Owner/isolation changed')
        require(model.get('document') == model.get('initialDocument') == initial['initialDocument']
                and model.get('records') == model.get('initialRecords') == touched
                and model.get('unrelated') == model.get('initialUnrelated') == unrelated
                and model.get('detail') == initial['detail'], 'Original List source/visit changed')
        require(comments.get('active') is True and comments.get('record') == READ['KNOWN']
                and comments.get('root') == ROOT and comments.get('hasMore') is True
                and all(comments.get(k) == initial['comments'].get(k) for k in ('generation', 'requestTs'))
                and all(comments.get(k) == [] for k in ('deleted', 'live', 'liveTs', 'accepted', 'acceptedTs', 'liveReactions', 'editCustody'))
                and comments.get('reveal') is None, 'Original thread ownership/projection changed')
        compose = model['composeLedger']
        require(all(compose.get(k) == [] for k in ('prepared', 'posts', 'messages', 'policies'))
                and all(compose.get(k) is False for k in ('published', 'held', 'heldPost')), 'Reaction sent a comment or posting request')
        require(proof.get('seed') == seed and proof.get('originalA') == a
                and proof.get('newerB') is None,
                'Original rich draft or independent disk proof changed')
        store = model['draftStore']; entries = store.get('drafts', [])
        require(len(entries) == 1 and store.get('active') == store0.get('active'), 'Action changed native draft custody')
        draft, journal = entries[0], proof.get('journal', {})
        require(draft.get('id') == draft0['id'] and draft.get('generation') == generation
                and draft.get('native') is True and store.get('active') == f"{draft['id']}:{comments['generation']}:{generation}"
                and draft.get('scope') == journal.get('scope') == scope
                and draft.get('blocks') == journal.get('blocks') == a
                and draft.get('capture') == journal.get('capture') and R['uuid'](draft.get('capture'))
                and draft.get('savedRevision') == journal.get('revision') and R['integer'](journal.get('revision'), 1)
                and R['integer'](draft.get('sequence')) and draft.get('attempt') is journal.get('attempt') is None
                and all(draft.get(k) is False for k in ('busy', 'saving', 'loading', 'captureFailed', 'terminalPending'))
                and draft.get('error') is None and draft.get('status') is None, 'Draft/model/journal disagreed or became busy')
        if previous_draft is not None:
            require(draft['sequence'] >= previous_draft['sequence'] and draft['savedRevision'] >= previous_draft['savedRevision'],
                    'Draft capture/revision went backwards')
        previous_draft = draft
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('dialog') is True
                and g.get('comments') is True and g.get('compose') is True, 'Native action surface unavailable')
        v = g.get('viewport')
        require(isinstance(v, list) and len(v) == 2 and all(type(n) in (int, float) and math.isfinite(n) and n > 0 for n in v), 'Invalid viewport')
        if viewport is None: viewport = v
        require(v == viewport, 'Viewport changed')
        editor = g.get('editor', {})
        require(editor.get('visible') is True and editor.get('editable') is True and editor.get('blocks') == a
                and editor.get('generation') == str(generation) and editor.get('sameNode') is True
                and editor.get('bold') is True and editor.get('text') == R['visible_text'](a), 'Original native rich text/style/node changed')
        rectangle(editor.get('rect'), v)
        X['editor_text'](g)
        require(isinstance(g.get('active'), str), 'Missing native focus witness')
        controls = g.get('controls', []); ids = [c['id'] for c in controls]
        require(len(ids) == len(set(ids)) and set(ids) == {'slack-list-comments-back', 'slack-list-comments-next',
                'slack-list-comments-refresh', 'slack-list-comment-send'}, 'Wrong native controls')
        for control in controls:
            require(control.get('visible') is True and control.get('hit') is True, 'Native control obscured')
            rectangle(control.get('rect'), v)
        capture = directory / f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture', '')) == capture, 'Foreign capture path')
        if png: H['png_signature'](capture, dimensions)
    require(receipt.get('final') == rows[-1]['model'], 'Final snapshot changed')
    return rows, initial, scope


def reactions(removed=()):
    rows = []
    for name in OWN:
        users = ['U0','U1'] if name == OWN[0] else ['U0']
        if name in removed: users.remove('U0')
        if users: rows.append(dict(name=name, count=len(users), users=users))
    return rows + [dict(name=name,count=1,users=[user]) for name,user in
                   ((SELECTED,'U2'),(ALIAS,'U0'),('wave','U1'))]


def page(removed=()):
    raw = X['original_page']()
    raw['messages'][1].update(reactions=reactions(removed),family_fixture_opaque={'keep':'raw bases are distinct'})
    return raw


def full_read():
    return dict(request=dict(method='reactions.get',args=dict(channel=CHANNEL,timestamp=FIRST,full='true')),
                response=dict(ok=True,type='message',channel=CHANNEL,
                              message=dict(ts=FIRST,thread_ts=ROOT,reactions=reactions((OWN[0],OWN[2])))))


def stage_spec(case, i):
    success = case == 'reaction-family'
    if success:
        completed = (0,0,0,0,1,2,3,3,4,4)[i]
        prepared = (0,1,1,1,2,3,4,4,4,4)[i]
        phases = [] if i == 0 else ['confirmed' if n < completed else 'sent' if n == completed else 'queued' for n in range(4)]
        removed = OWN[:(0,0,0,1,1,2,3,3,4,4)[i]]
        indices = list(range(prepared))
        groups = [] if not i else [OWN]
        status = None if not i else PENDING if i < 8 else 'Reaction family: 4 confirmed. Newer comment updates are shown.'
    else:
        completed = (0,3,3,3,4,4,4,4,5,6)[i]
        prepared = (0,3,3,3,4,4,4,5,6,6)[i]
        phases = [] if not i else ['confirmed','rejected','unknown' if i < 3 else 'observed','not-sent']
        if i >= 4: phases += ['rejected','not-sent']
        if i >= 7: phases += ['sent','queued'] if i == 7 else ['confirmed','sent'] if i == 8 else ['confirmed','confirmed']
        removed = () if not i else (OWN[0],) if i < 3 else (OWN[0],OWN[2]) if i < 8 else OWN[:3] if i == 8 else OWN
        indices = [0,1,2,4,6,7][:prepared]
        groups = [] if not i else [OWN] + [(OWN[1],OWN[3])] * (int(i >= 4)+int(i >= 7))
        status = (None if not i else
                  'Reaction family: 1 confirmed, 1 rejected, 1 unconfirmed, 1 not sent. Refresh comments to check delivery.' if i < 3 else
                  'Reaction family: 1 confirmed, 1 observed, 1 rejected, 1 not sent. Newer comment updates are shown.' if i == 3 else
                  'Reaction family: 1 rejected, 1 not sent.' if i < 7 else
                  PENDING if i < 9 else 'Reaction family: 2 confirmed.')
    return dict(completed=completed,prepared=prepared,phases=phases,removed=removed,indices=indices,groups=groups,status=status)


def probes(case, i):
    spec = stage_spec(case,i)
    global_block = (case == 'reaction-family' and i == 7) or (case != 'reaction-family' and i == 4)
    family_block = any(p in ('queued','sent','unknown') for p in spec['phases'])
    return [dict(name=r['name'],count=r['count'],own=USER in r['users'],glyph=GLYPHS[r['name']],
                 available=not global_block and (not family_block if r['name'] in OWN+(SELECTED,) else True))
            for r in reactions(spec['removed'])]


def attempts(case, i, ids, target, deadline):
    s = stage_spec(case,i); success = case == 'reaction-family'; rows=[]; start=0
    for plan in s['groups']:
        for offset,name in enumerate(plan):
            n=start+offset; phase=s['phases'][n]
            raw = dict(ok=True) if phase=='confirmed' else dict(ok=False,error='invalid_name') if not success and n==1 else None
            socket = success and n==0 and i>=3
            observed = not success and n==2 and i>=3
            issued = None if phase in ('queued','sent') else phase!='not-sent'
            rows.append(dict(id=ids[n],target=target,ts=FIRST,name=name,user=USER,adding=False,phase=phase,raw=raw,
                             socketObserved=socket,socketRevision=int(socket),canonicalObserved=observed,
                             readbackResolved=observed,published=phase=='confirmed' and not socket,
                             retryAt=deadline if not success and n==4 else 0,
                             fullReadback=full_read()['response'] if observed else None,
                             parent=ids[start],family='thumbsup',selected=SELECTED,planned=list(plan),issued=issued,
                             failure='transport' if not success and n==2 else 'rate-limited' if not success and n==4 else None))
        start+=len(plan)
    return dict(attempts=rows,pending=sum(p in ('sent','queued','unknown') for p in s['phases']))


def wire(case, i, ids):
    s=stage_spec(case,i); success=case=='reaction-family'
    names=list(OWN) if success else list(OWN)+[OWN[1],OWN[3],OWN[1],OWN[3]]
    entries=[dict(id=ids[n],request=dict(method='reactions.remove',args=dict(channel=CHANNEL,timestamp=FIRST,name=names[n])),user=USER)
             for n in s['indices']]
    outcomes=[]
    for n in s['indices'][:s['completed']]:
        raw=dict(ok=True); disposition='response'; extra={}
        if not success:
            if n==1: raw=dict(ok=False,error='invalid_name')
            if n in (2,4): raw=None; disposition='unconfirmed'
            if n==4: extra=dict(failure='rate-limited',retryAfterSecs=2)
        outcomes.append(dict(id=ids[n],disposition=disposition,raw=raw,**extra))
    return dict(prepared=entries,issued=copy.deepcopy(entries),outcomes=outcomes,completed=s['completed'],
                held=s['prepared']>s['completed'],networkCalls=0,
                fullReads=[full_read()] if not success and i>=3 else [])


def native(row, expected, status):
    g=row['geometry']; f=g.get('family',{})
    require(exact(f.get('tools'),0),'Generic toolbar introduced')
    pills=f.get('pills',[])
    require(len(pills)==len(expected),'Family pills missing/duplicated')
    for p,e in zip(pills,expected):
        require(all(exact(p.get(k),e[k]) for k in ('name','glyph','count','own'))
                and p.get('tag')=='BUTTON' and p.get('type')=='button'
                and p.get('disabled') is (not e['available']) and p.get('ariaDisabled')==str(not e['available']).lower()
                and p.get('pressed')==str(e['own']).lower()
                and all(p.get(k) is True for k in ('visible','hit','glyphVisible','glyphHit')),
                'Family pill/raw alias/count/own/glyph/admission changed')
        rectangle(p.get('rect'),g['viewport']); rectangle(p.get('glyphRect'),g['viewport'])
        art,box=p['glyphRect'],p['rect']
        require(art['left']>=box['left']-1 and art['right']<=box['right']+1
                and art['top']>=box['top']-1 and art['bottom']<=box['bottom']+1,'Family glyph escaped pill')
    notice=f.get('status')
    if status is None: require(notice is None,'Unexpected family status')
    else:
        require(notice and notice.get('text')==status and notice.get('role')=='status'
                and notice.get('visible') is True and status in g.get('notices',[]),'Partial family outcome guidance missing/changed')
        rectangle(notice.get('rect'),g['viewport'])


def validate(fixture, receipt, directory, *, expected_case=None, dimensions=None, png=True):
    case=receipt.get('case'); require(expected_case is None or case==expected_case,'Wrong family case')
    rows,initial,_=common(fixture,receipt,directory,case,dimensions,png)
    success=case=='reaction-family'; original=page(); future=page((OWN[0],OWN[2]))
    ids=[a['id'] for a in rows[-1]['model']['reactionActions']['attempts']]
    require(len(ids)==(4 if success else 8) and all(R['integer'](n,1) for n in ids)
            and len(set(ids))==len(ids) and ids==sorted(ids),'Family child IDs missing/reused/unordered')
    target=dict(document=initial['initialDocument'],record=initial['records'][READ['KNOWN']],channel=CHANNEL,root=ROOT)
    deadline=0 if success else rows[4]['model']['reactionActions']['attempts'][4]['retryAt']
    require(success or R['integer'](deadline,1),'Issued429 lacks original deadline')
    inputs=[]; releases=[]; server=[]; previous_read=None
    socket=dict(team='T1',generation=7,parsed=True,
                frame=dict(type='reaction_removed',user=USER,reaction=OWN[0],item=dict(type='message',channel=CHANNEL,ts=FIRST)),
                before=probes('reaction-family',2),after=probes('reaction-family',3))
    for i,row in enumerate(rows):
        m,g,p=row['model'],row['geometry'],row['proof']; c=m['comments']; s=stage_spec(case,i)
        expected_attempts=attempts(case,i,ids,target,deadline)
        require(exact(m.get('reactionActions'),expected_attempts),'Original per-child plan/issued/outcome/readback custody changed')
        require(exact(m.get('reactionLedger'),wire(case,i,ids)),'Original once-only requests or raw results changed')
        raw=original if success or i<3 else future
        typed=[I['typed'](v) for v in raw['messages']]; typed[1]['reactions']=reactions(s['removed'])
        require(exact(c.get('page'),raw) and exact(c.get('pageMessages'),typed)
                and exact(p.get('initialResponse'),original),'Original raw Page/typed family membership changed')
        require(c.get('visibleTs')==[FIRST] and len(g.get('rows',[]))==1 and g['rows'][0].get('ts')==FIRST,'Reply identity duplicated/changed')
        rx=[dict(name=r['name'],count=r['count']) for r in reactions(s['removed'])]
        require(exact(c.get('reactions'),[dict(ts=FIRST,reactions=rx)]) and exact(g['rows'][0].get('reactions'),rx)
                and all(g['rows'][0].get('text','').count(t)==1 for t in ('First page comment.','Review ready \u4e2d\U0001f642.')),
                'Family removal altered another message/body/reaction')
        expected_probe=probes(case,i)
        require(exact(m.get('familyProbe'),expected_probe)
                and exact(m.get('actionProbe'),next(x for x in expected_probe if x['name']==SELECTED))
                and m.get('familyStatus')==s['status'],'Independent family admission/status differs')
        native(row,expected_probe,s['status'])
        held_read=success and i in (7,8); loading=success and i==7
        X['reads'](row,initial['ledger']['requests'],READS[case][i],len(s['groups']),
                   cancelled=int(success and i>=8),held=held_read,completed=READS[case][i]-int(loading))
        require(c.get('loading') is loading and c.get('error')==(NOTICE if success and i>=8 else None),
                'Read loading/retirement guidance changed')
        rate=not success and i==4
        if rate: require(c.get('retryAt')==deadline and type(c.get('retryAfter')) is int and 0<c['retryAfter']<=2,'Issued429 cooldown absent')
        else: require(exact(c.get('retryAt'),0) and exact(c.get('retryAfter'),0),'Spurious/unretired cooldown')
        require(R['integer'](c.get('read'),1),'Missing read serial')
        if previous_read is not None:
            changes=(1,7,8) if success else (1,3,4,6,7)
            require(c['read']>previous_read if i in changes else c['read']==previous_read,'Unexpected read/replay/cancellation authority')
        previous_read=c['read']
        for control in g['controls']:
            disabled=loading or rate if control['id'] in ('slack-list-comments-next','slack-list-comments-refresh') else rate if control['id']=='slack-list-comment-send' else False
            require(control.get('disabled') is disabled,'Family action changed unrelated settled controls')
        if success and i>=8: require(NOTICE in g.get('notices',[]),'Retained-comments cancellation notice hidden')
        input_indices=(1,2) if success else (1,2,4,7)
        if i in input_indices:
            inputs.append(dict(stage=row['stage'],value=dict(ok=True,name=SELECTED,disabled=i==2,pressed='false')))
        if i in ((4,5,6,8) if success else (8,9)):
            releases.append(dict(stage=row['stage'],value=dict(delivered=True,cancelledBeforeReply=False)))
        if not success and i in (3,6): server.append(dict(stage=row['stage'],value=future))
        require(exact(p.get('inputs'),inputs) and exact(p.get('releases'),releases)
                and exact(p.get('serverUpdates'),server) and exact(p.get('socket'),socket if success and i>=3 else None),
                'Original click/release/socket/future-server provenance changed')
        require(exact(p.get('heldResponse'),original if success and i>=7 else None)
                and exact(p.get('release'),dict(delivered=False,cancelledBeforeReply=True) if success and i==9 else None)
                and p.get('serverResponse') is None and p.get('originalMessage') is None,'Original held read or POST custody changed')
        if i==2 or success and i==3:
            require(g['active']==rows[i-1]['geometry']['active'],'Disabled click/socket moved focus')
    return receipt

