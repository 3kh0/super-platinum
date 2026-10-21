"""Closed existing-pill actions: original request custody and native draft isolation."""
import copy
import math
from pathlib import Path
import runpy

E = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_edits.py')))
A, I, R, C, READ, H = (E[k] for k in ('A', 'I', 'R', 'C', 'READ', 'H'))
require, rectangle = R['require'], R['rectangle']
CHANNEL, FIRST, ROOT, USER = E['CHANNEL'], E['FIRST'], E['ROOT'], 'U0'
STAGES = {
    'reaction-actions': ('draft', 'add-held', 'duplicate', 'socket-added', 'added', 'remove-held',
                         'socket-removed', 'removed', 'add-again-held', 'socket-add-again',
                         'socket-opposite', 'acknowledged', 'local-add-held', 'read-held',
                         'local-acknowledged', 'read-released'),
    'reaction-failures': ('draft', 'rejected', 'offline', 'rate-limited', 'rate-expired',
                          'unconfirmed', 'readback', 'lone-pill', 'malformed', 'resolved'),
}
NOTICES = {
    'sent': 'Updating the reaction. Your draft is unchanged.',
    'not-sent': 'The reaction was not sent. Refresh comments before trying again.',
    'rejected': 'Slack rejected the reaction. Your draft is unchanged.',
    'unknown': 'Reaction delivery is unconfirmed. Refresh comments to check before trying again.',
    'confirmed': 'Reaction updated.',
    'observed': 'The requested reaction state was observed in refreshed comments.',
}


def validator(case):
    require(case in STAGES, 'Unknown reaction action case')
    def validate_case(fixture, receipt, directory, **kwargs):
        return validate(fixture, receipt, directory, expected_case=case, **kwargs)
    return validate_case


def exact(value, expected):
    """JSON equality without Python's bool/int equivalence."""
    if type(value) is not type(expected): return False
    if isinstance(expected, dict):
        return value.keys() == expected.keys() and all(exact(value[k], v) for k, v in expected.items())
    if isinstance(expected, list):
        return len(value) == len(expected) and all(exact(a, b) for a, b in zip(value, expected))
    return value == expected


def source(own):
    return [dict(name='wave', count=3 if own else 2, users=['U1', 'U2'] + ([USER] if own else []))]


def request(adding):
    return dict(method='reactions.add' if adding else 'reactions.remove',
                args=dict(channel=CHANNEL, timestamp=FIRST, name='wave'))


def original_page():
    return dict(ok=True, channel=CHANNEL, messages=[
        dict(ts=ROOT, subtype='list_record_comment', slack_list=dict(list_id=READ['LIST'], list_record_id=READ['KNOWN']),
             unknown=dict(keep=True), text='Record comment root'),
        dict(ts=FIRST, thread_ts=ROOT, user='U1', text='First page comment.',
             blocks=[dict(type='rich_text', elements=[
                 dict(type='rich_text_section', elements=[dict(type='text', text='First page comment.', style=dict(bold=True))]),
                 dict(type='rich_text_section', elements=[dict(type='text', text='Review ready \u4e2d\U0001f642.')])])],
             reactions=source(False))], has_more=True, response_metadata=dict(next_cursor=READ['CURSOR']))


def editor_text(geometry):
    """A visible editor box is insufficient when its actual text is clipped."""
    text = geometry.get('action', {}).get('editorText', {})
    clip = text.get('clip', {})
    def valid_rect(rect):
        return (isinstance(rect, dict) and set(rect) == {'left','top','right','bottom','width','height'}
                and all(type(rect[k]) in (int, float) and math.isfinite(rect[k]) for k in rect)
                and rect['width'] > 0 and rect['height'] > 0
                and abs(rect['right']-rect['left']-rect['width']) <= 0.01
                and abs(rect['bottom']-rect['top']-rect['height']) <= 0.01)
    require(valid_rect(clip), 'Missing real draft text clip')
    left, top, right, bottom = (clip[k] for k in ('left','top','right','bottom'))
    viewport = geometry['viewport']; editor = geometry['editor']['rect']
    require(left >= -1 and top >= -1 and right <= viewport[0]+1 and bottom <= viewport[1]+1
            and left >= editor['left']-1 and top >= editor['top']-1
            and right <= editor['right']+1 and bottom <= editor['bottom']+1, 'Draft text clip escaped actual editor/viewport')
    lines = text.get('lines')
    require(isinstance(lines, list) and 0 < len(lines) <= 128, 'Missing actual text range lines')
    visible = 0
    for line in lines:
        rect = line.get('rect', {})
        require(valid_rect(rect), 'Malformed actual draft text range')
        fragments = line.get('fragments')
        require(isinstance(fragments, list) and 0 < len(fragments) <= 256
                and all(valid_rect(f.get('rect')) and type(f.get('hit')) is bool for f in fragments),
                'Missing actual draft text fragments')
        for key, extent in (('left',min),('top',min),('right',max),('bottom',max)):
            require(abs(rect[key]-extent(f['rect'][key] for f in fragments)) <= 0.01, 'Draft line is not the actual fragment union')
        full = rect['left'] >= left-1 and rect['right'] <= right+1 and rect['top'] >= top-1 and rect['bottom'] <= bottom+1
        require(line.get('fullVisible') is full and line.get('hit') is all(f['hit'] for f in fragments),
                'Draft text line clipping/hit witness disagrees')
        visible += int(full and line['hit'])
    require(text.get('ok') is True and type(text.get('visibleLines')) is int
            and text['visibleLines'] == visible and visible >= 1, 'No complete hit-testable draft text line is visible')


def common(fixture, receipt, directory, expected_case, dimensions, png):
    case = receipt.get('case'); directory = Path(directory)
    require(case in STAGES and (expected_case is None or case == expected_case)
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
    require(exact(initial['comments']['page'], original_page()), 'Original closed HTTP source changed')
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
        editor_text(g)
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


def projected(row, raw_page, members, disabled, status):
    model, g = row['model'], row['geometry']; comments = model['comments']
    own, count = USER in members, len(members)
    reaction = [dict(name='wave', count=count, users=members)] if members else []
    typed = [I['typed'](m) for m in raw_page['messages']]
    typed[1]['reactions'] = reaction
    require(exact(comments.get('page'), raw_page) and exact(comments.get('pageMessages'), typed),
            'Raw page or typed reaction/body/source identity changed')
    projected_reactions = [dict(ts=FIRST, reactions=[dict(name='wave', count=count)] if count else [])]
    require(comments.get('visibleTs') == [FIRST] and comments.get('reactions') == projected_reactions,
            'Reaction projection duplicated or changed another reply')
    dom = g.get('rows', [])
    require([r.get('ts') for r in dom] == [FIRST]
            and [dict(ts=r.get('ts'), reactions=r.get('reactions')) for r in dom] == projected_reactions
            and isinstance(dom[0].get('text'), str) and 'First page comment.' in dom[0]['text']
            and 'Review ready \u4e2d\U0001f642.' in dom[0]['text'], 'Native reply body/count changed')
    expected_probe = dict(ts=FIRST, name='wave', generation=comments['generation'], own=own,
                          count=count, available=not disabled, status=status)
    require(model.get('actionProbe') == expected_probe
            and type(model['actionProbe'].get('count')) is int
            and type(model['actionProbe'].get('own')) is bool
            and type(model['actionProbe'].get('available')) is bool, 'Independent own/count/admission probe disagrees')
    action = g.get('action', {}); button = action.get('button')
    require(type(action.get('tools')) is int and action['tools'] == 0, 'Generic message toolbar appeared')
    if count == 0:
        require(button is None, 'Removed final reaction left a stale action pill')
    else:
        require(isinstance(button, dict) and button.get('tag') == 'BUTTON' and button.get('type') == 'button' and button.get('name') == 'wave'
            and button.get('disabled') is disabled and button.get('ariaDisabled') == str(disabled).lower()
            and button.get('pressed') == str(own).lower() and button.get('own') is own
            and type(button.get('count')) is int and button['count'] == count
            and button.get('visible') is True and button.get('hit') is True,
            'Pill accessible state/visibility or row capabilities changed')
        rectangle(button.get('rect'), g['viewport'])
    if status is None:
        require(action.get('status') is None, 'Unexpected initial action status')
    else:
        notice = action.get('status', {})
        require(notice.get('text') == status and notice.get('role') == 'status'
                and notice.get('visible') is True and notice.get('outsideScroll') is True,
                'Original action disposition guidance missing or clipped')
        rectangle(notice.get('rect'), g['viewport'])


def reads(row, template, count, preflights, *, cancelled=0, held=False, completed=None):
    ledger = row['model']['ledger']; expected = [dict(copy.deepcopy(r), serial=4*batch+n+1)
        for batch in range(count//4) for n, r in enumerate(template)]
    require(len(template) == 4 and [r.get('method') for r in template] ==
            ['files.info', 'lists.records.info', 'experiments.getByUser', 'conversations.replies']
            and template[1]['args']['id'] == READ['KNOWN'] and template[3]['args']['ts'] == ROOT,
            'Wrong initial acquisition target/order')
    require(ledger.get('requests') == expected, 'Unexpected, duplicate or rewritten reads')
    C['validate_reads'](expected)
    for key, value in (('completed', count if completed is None else completed), ('cancelled', cancelled), ('replies', count//4)):
        require(type(ledger.get(key)) is int and ledger[key] == value, 'Read completion/cancellation changed')
    require(ledger.get('held') is held, 'Read hold changed')
    require(row['model']['composeLedger'].get('preflights') ==
            [dict(method='conversations.genericInfo', args={'updated_channels':'{"CCOMMENT01":0}'})] * preflights,
            'Reaction channel admission requests changed')


def action_ledger(row, ids, adding, prepared, issued, outcomes, held):
    ledger = row['model']['reactionLedger']
    entries = [dict(id=ids[n], request=request(adding[n]), user=USER) for n in prepared]
    expected_issued = [dict(id=ids[n], request=request(adding[n]), user=USER) for n in issued]
    expected_outcomes = [dict(id=ids[n], disposition=disposition, raw=raw) for n, disposition, raw in outcomes]
    require(exact(ledger.get('prepared'), entries) and exact(ledger.get('issued'), expected_issued),
            'Original reaction method/target/self changed or duplicate dispatch occurred')
    require(exact(ledger.get('outcomes'), expected_outcomes) and type(ledger.get('completed')) is int
            and ledger['completed'] == len(outcomes) and ledger.get('held') is held
            and type(ledger.get('networkCalls')) is int and ledger['networkCalls'] == 0,
            'Original reaction disposition/raw response changed or live dispatch occurred')


def reaction_frame(added):
    return dict(type='reaction_added' if added else 'reaction_removed', user=USER, reaction='wave',
                item=dict(type='message', channel=CHANNEL, ts=FIRST))


def server_update(stage, members):
    page = original_page(); reactions = [dict(name='wave', count=len(members), users=members)] if members else []
    page['messages'][1]['reactions'] = reactions
    return dict(stage=stage, value=dict(ts=FIRST, own=USER in members, reactions=reactions, response=page))


def full_response():
    return dict(ok=True, type='message', channel=CHANNEL, message=dict(ts=FIRST, thread_ts=ROOT, reactions=[]))


def full_read():
    return dict(request=dict(method='reactions.get', args=dict(channel=CHANNEL, timestamp=FIRST, full='true')),
                response=full_response())


def attempts(row, initial, ids, specs):
    target = dict(document=initial['initialDocument'], record=initial['records'][READ['KNOWN']], channel=CHANNEL, root=ROOT)
    expected = []
    for n, spec in enumerate(specs):
        adding, phase, raw, socket_revision, canonical, resolved, published, retry_at, full = spec
        expected.append(dict(id=ids[n], target=target, ts=FIRST, name='wave', user=USER, adding=adding, phase=phase,
            raw=raw, socketObserved=socket_revision > 0, socketRevision=socket_revision, canonicalObserved=canonical,
            readbackResolved=resolved, published=published, retryAt=retry_at, fullReadback=full))
    pending = sum(s[1] in ('preparing', 'sent', 'unknown') for s in specs)
    require(exact(row['model'].get('reactionActions'), dict(attempts=expected, pending=pending)),
            'Original action owner/outcome/socket/readback custody changed')
    require(type(row['model']['reactionActions'].get('pending')) is int, 'Pending count type changed')


def input_proof(stage, own, disabled=False):
    return dict(stage=stage, value=dict(ok=True, disabled=disabled, pressed=str(own).lower(), type='button', focused=not disabled))


def validate(fixture, receipt, directory, *, expected_case=None, dimensions=None, png=True):
    rows, initial, scope = common(fixture, receipt, directory, expected_case, dimensions, png)
    success = receipt['case'] == 'reaction-actions'
    final_attempts = rows[-1]['model']['reactionActions']['attempts']
    ids = [a['id'] for a in final_attempts]
    require(len(ids) == (4 if success else 5) and len(set(ids)) == len(ids)
            and all(R['integer'](n, 1) for n in ids) and ids == sorted(ids), 'Original action IDs missing/duplicated')
    template = initial['ledger']['requests']; page = original_page(); previous_read = None
    expected_events = []; server_updates = []; releases = []; inputs = []
    for i, row in enumerate(rows):
        model, g, proof = row['model'], row['geometry'], row['proof']; c = model['comments']
        if success:
            count = (0,1,1,1,1,2,2,2,3,3,3,3,4,4,4,4)[i]
            done = (0,0,0,0,1,1,1,2,2,2,2,3,3,3,4,4)[i]
            read_count = (4,8,8,8,8,12,12,12,16,16,16,16,20,24,24,24)[i]
            own = i in (3,4,5,9,14,15); members = ['U1','U2'] + ([USER] if own else [])
            disabled = count > done; phase = None if i == 0 else 'sent' if disabled else 'confirmed'
            reads(row, template, read_count, count, cancelled=int(i >= 14), held=i in (13,14), completed=read_count-int(i==13))
            adding = [True,False,True,True]
            action_ledger(row, ids, adding, range(count), range(count), [(n,'response',dict(ok=True)) for n in range(done)], disabled)
            specs = []
            for n in range(count):
                revision = sum(i >= event for event in ((3,6,9,10),(6,9,10),(9,10),())[n])
                specs.append((adding[n], 'confirmed' if n < done else 'sent', dict(ok=True) if n < done else None,
                              revision, False, False, n==3 and i>=14, 0, None))
            status = None if phase is None else ('Reaction response retained. Newer comment updates are shown.'
                if phase == 'confirmed' and i < 12 else NOTICES[phase])
            if i in (3,6,9,10):
                server_updates.append(server_update(row['stage'], members))
                expected_events.append(dict(stage=row['stage'], team='T1', generation=7, frame=reaction_frame(own), parsed=True,
                    before=rows[i-1]['model']['actionProbe'], after=model['actionProbe']))
            if i in (4,7,11,14):
                releases.append(dict(stage=row['stage'], value=dict(delivered=True,cancelledBeforeReply=False)))
            if i in (1,2,5,8,12):
                inputs.append(input_proof(row['stage'], i==5, i==2))
            require(proof.get('heldResponse') == (original_page() if i>=13 else None)
                    and proof.get('release') == (dict(delivered=False,cancelledBeforeReply=True) if i==15 else None),
                    'Local confirmation rewrote/delivered an older held response')
            require(proof.get('loneEvents') in (None, []), 'Unexpected foreign reactor removals')
            serial_changes = (1,5,8,12,13,14)
        else:
            count = (0,1,2,3,3,4,4,4,5,5)[i]
            prepared = (0,1,2,2,2,3,3,3,4,4)[i]
            read_count = (4,8,12,16,16,20,24,24,28,32)[i]
            members = ['U1','U2'] if i<6 else ['U1','U2',USER] if i==6 else [USER] if i<9 else []
            disabled = i in (3,5,8,9)
            phase = (None,'rejected','not-sent','not-sent','not-sent','unknown','observed','observed','unknown','observed')[i]
            status = NOTICES[phase] if phase else None
            reads(row, template, read_count, prepared)
            adding = [True,True,True,True,False]
            indices = [0,1,3,4][:prepared]
            outcomes = [(n, 'not-requested' if n==1 else 'unconfirmed' if n==3 else 'response',
                         dict(ok=False,error='invalid_name') if n==0 else dict(ok='true',opaque='malformed-reaction-receipt') if n==4 else None)
                        for n in indices]
            action_ledger(row, ids, adding, indices, [n for n in indices if n!=1], outcomes, False)
            deadline = rows[3]['model']['reactionActions']['attempts'][2]['retryAt']
            require(R['integer'](deadline,1), 'Actual rate deadline missing')
            specs = []
            for n in range(count):
                p = 'rejected' if n==0 else 'not-sent' if n in (1,2) else 'observed' if n==3 and i>=6 or n==4 and i==9 else 'unknown'
                raw = dict(ok=False,error='invalid_name') if n==0 else dict(ok='true',opaque='malformed-reaction-receipt') if n==4 else None
                observed = n==3 and i>=6 or n==4 and i==9
                specs.append((adding[n],p,raw,0,observed,observed,False,deadline if n==2 else 0,full_response() if n==4 and i==9 else None))
            if i in (1,2,3,5,8):inputs.append(input_proof(row['stage'], i==8))
            if i in (6,7,9):server_updates.append(server_update(row['stage'], members))
            if i==6:page=server_update('readback',members)['value']['response']
            if i==9:page=server_update('resolved',[])['value']['response']
            require(proof.get('heldResponse') is None and proof.get('release') is None, 'Failure invented held delivery')
            if i>=7:
                before=copy.deepcopy(rows[6]['model']['actionProbe']); middle=copy.deepcopy(before); middle['count']=2
                expected_lone=[]
                for user,before_probe,after_probe in [('U1',before,middle),('U2',middle,rows[7]['model']['actionProbe'])]:
                    frame=reaction_frame(False);frame['user']=user
                    expected_lone.append(dict(stage='lone-pill',team='T1',generation=7,frame=frame,parsed=True,before=before_probe,after=after_probe))
                require(exact(proof.get('loneEvents'), expected_lone), 'Parsed original foreign reactor provenance changed')
            else:require(proof.get('loneEvents') in (None,[]), 'Premature reactor events')
            serial_changes = (1,2,3,5,6,8,9)
        attempts(row, initial, ids, specs)
        require(exact(model['reactionLedger'].get('fullReads'), [full_read()] if not success and i==9 else []),
                'Full membership request/response changed or absence inferred from Page')
        require(exact(proof.get('serverUpdates'),server_updates) and exact(proof.get('releases'),releases) and exact(proof.get('inputs'),inputs),
                'Original input/server/response evidence rewritten or duplicated')
        require(proof.get('originalMessage') is None and proof.get('serverResponse') is None, 'Unrelated original POST/readback introduced')
        require(c.get('loading') is (success and i==13)
                and c.get('error') == (I['ERROR'] if success and i>=14 else None), 'Read busy/retirement guidance changed')
        if not success and i==3:
            require(type(c.get('retryAfter')) is int and 0<c['retryAfter']<=2 and c.get('retryAt')==deadline, '429 deadline not enforced')
        else:require(c.get('retryAfter')==0 and c.get('retryAt')==0, 'Cooldown not expired or spurious cooldown')
        for control in g['controls']:
            rate_limited = not success and i==3
            block = (success and i==13 or rate_limited) if control['id'] in ('slack-list-comments-next','slack-list-comments-refresh') else rate_limited if control['id']=='slack-list-comment-send' else False
            require(control.get('disabled') is block, 'Read/draft controls changed with reaction action')
        if success and i>=14:require(I['ERROR'] in g.get('notices',[]), 'Held read retirement notice missing')
        require(R['integer'](c.get('read'),1), 'Read serial missing')
        if previous_read is not None:
            require(c['read']>previous_read if i in serial_changes else c['read']==previous_read, 'Unexpected read retirement or automatic retry')
        previous_read=c['read']
        projected(row, page, members, disabled, status)
        # Explicit clicks/seals may move focus; only intervening socket/release
        # activity must retain the captured post-click focus owner.
        baseline = ({2:1,3:2,4:3,6:5,7:6,9:8,10:9,11:10,14:13,15:14}.get(i) if success else {7:6}.get(i))
        if baseline is not None:require(g['active']==rows[baseline]['geometry']['active'], 'Socket/receipt stole native focus')
    require(exact(receipt.get('reactionEvents'),expected_events), 'Parsed own socket event provenance/order changed')
    return receipt
