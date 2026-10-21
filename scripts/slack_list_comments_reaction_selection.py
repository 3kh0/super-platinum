"""Closed local List picker selection, exact resolved wires and observation custody."""
import copy
import math
import re
from pathlib import Path
import runpy
X = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_reaction_actions.py')))
E, A, I, R, C, READ, H = (X[k] for k in ('E','A','I','R','C','READ','H'))
require, rectangle, exact = X['require'], X['rectangle'], X['exact']
CHANNEL, FIRST, ROOT, USER = X['CHANNEL'], X['FIRST'], X['ROOT'], 'U0'
CUSTOM, ALIAS, TONE = 'fixture_selection_custom', 'fixture_selection_alias', '+1::skin-tone-4'
STAGES = {
 'reaction-selection': ('draft','opened','searched','cancelled','add-held','socket-added','added','exact-removed','custom-added','alias-removed'),
 'reaction-selection-failures': ('draft','rejected','unknown','readback','rate-limited','rate-expired','retry-held','retried'),
 'reaction-selection-custody': ('draft','catalog-bound','catalog-stale','prefs-bound','prefs-stale','preflight-held','preflight-retired')}
READS = {'reaction-selection': (4,4,4,4,8,8,8,12,16,20),
         'reaction-selection-failures': (4,8,12,16,20,20,24,24),
         'reaction-selection-custody': (4,4,4,4,4,8,8)}
OPEN = {'reaction-selection': ('opened','searched'), 'reaction-selection-failures': (),
        'reaction-selection-custody': ('catalog-bound','catalog-stale','prefs-bound','prefs-stale')}


def validator(case):
    require(case in STAGES, 'Unknown selection case')
    def bound(fixture, receipt, directory, **kwargs):
        return validate(fixture, receipt, directory, expected_case=case, **kwargs)
    return bound


def common(fixture, receipt, directory, case, dimensions, png, *, stages=None, open_stages=None, compose_held_stages=None):
    directory = Path(directory)
    stages = STAGES.get(case) if stages is None else stages
    open_stages = OPEN.get(case, ()) if open_stages is None else open_stages
    compose_held_stages = (('preflight-held',) if case=='reaction-selection-custody' else ()) if compose_held_stages is None else compose_held_stages
    require(stages is not None and receipt.get('case') == case
            and fixture in C['FIXTURES'] and receipt.get('fixture') == fixture
            and receipt.get('ok') is True and receipt.get('noLiveNetwork') is True, 'Wrong reaction selection receipt')
    ready = receipt.get('readiness', {})
    H['validate_ready'](ready, C['ready_tuple'](fixture, ready.get('nonce'), ready.get('pid')))
    rows = receipt.get('stages', [])
    require([r.get('stage') for r in rows] == list(stages), 'Missing reaction selection stages')
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
                and all(compose.get(k) is False for k in ('published', 'heldPost'))
                and compose.get('held') is (row['stage'] in compose_held_stages), 'Reaction sent a comment or posting request')
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
        if row['stage'] not in open_stages: X['editor_text'](g)
        require(isinstance(g.get('active'), str), 'Missing native focus witness')
        controls = g.get('controls', []); ids = [c['id'] for c in controls]
        require(len(ids) == len(set(ids)) and set(ids) == {'slack-list-comments-back', 'slack-list-comments-next',
                'slack-list-comments-refresh', 'slack-list-comment-send'}, 'Wrong native controls')
        for control in controls:
            require(control.get('visible') is True and type(control.get('hit')) is bool, 'Native control visibility unavailable')
            if row['stage'] not in open_stages: require(control['hit'] is True, 'Closed picker obscured original controls')
            rectangle(control.get('rect'), v)
        capture = directory / f'{fixture}-{row["stage"]}.png'
        require(Path(row.get('capture', '')) == capture, 'Foreign capture path')
        if png: H['png_signature'](capture, dimensions)
    require(receipt.get('final') == rows[-1]['model'], 'Final snapshot changed')
    return rows, initial, scope


def reactions(case, i):
    names = ['+1::skin-tone-2', 'thumbsup::skin-tone-4', 'eyes']
    if case == 'reaction-selection':
        if i in (5, 6): names.append(TONE)
        if i == 8: names.append(CUSTOM)
    elif case == 'reaction-selection-failures':
        if 3 <= i < 7: names.remove('eyes')
    return [dict(name=n, count=1, users=[USER]) for n in names]


def page(rx):
    raw = X['original_page']()
    raw['messages'][1].update(reactions=rx, selection_fixture_opaque={'keep':'exact selection differs from raw-base family'})
    return raw


def full_read():
    return dict(request=dict(method='reactions.get', args=dict(channel=CHANNEL,timestamp=FIRST,full='true')),
                response=dict(ok=True,type='message',channel=CHANNEL,
                              message=dict(ts=FIRST,thread_ts=ROOT,reactions=reactions('reaction-selection-failures',3))))


def specs(case, i):
    if case == 'reaction-selection':
        count = (0,0,0,0,1,1,1,2,3,4)[i]
        done = (0,0,0,0,0,0,1,2,3,4)[i]
        names, adding = [TONE,TONE,CUSTOM,CUSTOM], [True,False,True,False]
        phases = ['confirmed' if n < done else 'sent' for n in range(count)]
    elif case == 'reaction-selection-failures':
        count = (0,1,2,2,3,3,4,4)[i]
        done = (0,1,2,2,3,3,3,4)[i]
        names, adding = ['eyes']*4, [False,False,True,True]
        phases = ['rejected','observed' if i>=3 else 'unknown','rejected','confirmed' if i==7 else 'sent'][:count]
    else:
        count, done = 0, 0
        names, adding = ['eyes'], [False]
        phases = ['preparing'] if i == 5 else ['not-sent'] if i == 6 else []
    return dict(count=count,done=done,names=names,adding=adding,phases=phases)


def attempts(case, i, ids, target, deadline):
    s=specs(case,i); rows=[]
    for n, phase in enumerate(s['phases']):
        socket=case=='reaction-selection' and n==0 and i>=5
        observed=case=='reaction-selection-failures' and n==1 and i>=3
        raw=dict(ok=True) if phase=='confirmed' else dict(ok=False,error='invalid_name') if case=='reaction-selection-failures' and n==0 else None
        rows.append(dict(id=ids[n],target=target,ts=FIRST,name=s['names'][n],user=USER,adding=s['adding'][n],phase=phase,raw=raw,
                         socketObserved=socket,socketRevision=int(socket),canonicalObserved=observed,readbackResolved=observed,
                         published=phase=='confirmed' and not socket,retryAt=deadline if case=='reaction-selection-failures' and n==2 else 0,
                         fullReadback=full_read()['response'] if observed else None))
    return dict(attempts=rows,pending=sum(p in ('preparing','sent','unknown') for p in s['phases']))


def wire(case, i, ids):
    s=specs(case,i)
    entries=[dict(id=ids[n],user=USER,request=dict(method='reactions.add' if s['adding'][n] else 'reactions.remove',
             args=dict(channel=CHANNEL,timestamp=FIRST,name=s['names'][n]))) for n in range(s['count'])]
    outcomes=[]
    for n in range(s['done']):
        value=dict(id=ids[n],disposition='response',raw=dict(ok=True))
        if case=='reaction-selection-failures':
            if n==0:value['raw']=dict(ok=False,error='invalid_name')
            if n in (1,2):value.update(disposition='unconfirmed',raw=None)
            if n==2:value.update(failure='rate-limited',retryAfterSecs=2)
        outcomes.append(value)
    return dict(prepared=entries,issued=copy.deepcopy(entries),outcomes=outcomes,completed=s['done'],held=s['count']>s['done'],networkCalls=0,
                fullReads=[full_read()] if case=='reaction-selection-failures' and i>=3 else [])


def action_status(case, i):
    phases=specs(case,i)['phases']
    if not phases:return None
    if case=='reaction-selection' and i==6:return 'Reaction response retained. Newer comment updates are shown.'
    p=phases[-1]
    return X['NOTICES'].get(p, 'Checking the current reaction. Your draft is unchanged.')


PREFS = dict(preferred_skin_tone=4,emoji_mode='native',emoji_use={},frecency='{}')
STALE = 'The comment or emoji preferences changed. Close and reopen the picker.'
GLYPHS = {'+1::skin-tone-2':'\U0001f44d\U0001f3fb','thumbsup::skin-tone-4':'\U0001f44d\U0001f3fd',
          TONE:'\U0001f44d\U0001f3fd','eyes':'\U0001f440',CUSTOM:None}


def source_proof(rows, case):
    original=rows[0]['model']['selectionSource']['original']
    require(exact(original.get('raw'),dict(ok=True,prefs=PREFS,fixture_opaque='original complete prefs'))
            and exact(original.get('parsed'),PREFS), 'Original complete preferences changed')
    pref=original.get('preferences',{}); rev=pref.get('revision'); custom=original.get('customRevision')
    require(exact(pref,dict(loaded=True,revision=rev,raw=PREFS)) and R['integer'](rev,1) and R['integer'](custom,1)
            and exact(original.get('custom'),{CUSTOM:dict(name=CUSTOM,value='https://fixture.invalid/selection-custom.png'),
                       ALIAS:dict(name=ALIAS,value='alias:'+CUSTOM)}), 'Original catalog/custom/preference authority changed')
    token=None; changed_token=None
    for i,row in enumerate(rows):
        s=row['model']['selectionSource']; custody=case=='reaction-selection-custody'
        require(exact(s.get('original'),original), 'Initial catalog/prefs seed rewritten')
        expected_pref=copy.deepcopy(pref)
        if custody and i>=4:expected_pref['revision']+=1;expected_pref['raw']['preferred_skin_tone']=5
        if custody and i>=6:expected_pref['revision']+=1;expected_pref['raw']['preferred_skin_tone']=6
        expected_custom=copy.deepcopy(original['custom'])
        if custody and i>=2:expected_custom[ALIAS]['value']='alias:eyes'
        require(exact(s.get('preferences'),expected_pref) and exact(s.get('customRevision'),custom+int(custody and i>=2)),
                'Observed preference/custom revision changed without evidence')
        require(exact(s.get('custom'),expected_custom),'Current custom alias observation changed without evidence')
        c=s.get('catalog')
        if i==0:require(c is None,'Draft prematurely borrowed a loaded catalog')
        else:
            require(isinstance(c,dict) and set(c)=={'token','current'} and c['current'] is True and isinstance(c['token'],str)
                    and re.fullmatch(r'[1-9][0-9]*/local',c['token']) is not None
                    and int(c['token'].split('/')[0]) <= 2**64-1,
                    'Current immutable catalog missing')
            if token is None:token=c['token']
            if custody and i>=2:
                if changed_token is None:changed_token=c['token'];require(int(changed_token.split('/')[0])>int(token.split('/')[0]),'Catalog replacement reused allocation')
                require(c['token']==changed_token,'Unexplained successor catalog')
            else:require(c['token']==token,'Catalog allocation changed unexpectedly')
    return original


def native(row,case,i):
    m,g=row['model'],row['geometry']; v=g['viewport']; s=g.get('selection',{}); opened=row['stage'] in OPEN[case]
    ready=not(case=='reaction-selection-custody' and i in (2,4)); rate=case=='reaction-selection-failures' and i==4
    loading=case=='reaction-selection-custody' and i==5
    phases=specs(case,i)['phases']; pending=any(p in ('preparing','sent') for p in phases)
    available=not opened and not rate and not pending
    status=action_status(case,i)
    require(exact(m.get('actionProbe'),dict(available=available,status=status)), 'Picker action admission/status changed')
    expected=[dict(name=r['name'],count=1,own=True,glyph=GLYPHS[r['name']]) for r in reactions(case,i)]
    require(exact(m.get('selectionProbe'),expected), 'Exact selection changed alias/sibling projection')
    def box(value,need_hit=True):
        require(isinstance(value,dict) and value.get('visible') is True and type(value.get('hit')) is bool
                and (not need_hit or value['hit']), 'Picker/control obscured')
        rectangle(value.get('rect'),v)
    pills=s.get('pills',[]);require(len(pills)==len(expected),'Reaction pill count changed')
    for pill,e in zip(pills,expected):
        blocked=opened or rate or loading
        if case=='reaction-selection' and i in (4,5) and pill.get('name','').startswith('+1::'):blocked=True
        if case=='reaction-selection-failures' and i==2 and pill.get('name')=='eyes':blocked=True
        require(all(exact(pill.get(k),e[k]) for k in ('name','count','own'))
                and pill.get('tag')=='BUTTON' and pill.get('type')=='button' and pill.get('pressed')=='true'
                and pill.get('disabled') is blocked and pill.get('ariaDisabled')==str(blocked).lower(),
                'Reaction control semantics or unrelated alias admission changed')
        if not opened:box(pill)
    add=s.get('add');require(add and add.get('type')=='button' and add.get('disabled') is (not available),'Add reaction control changed')
    if not opened:box(add)
    feedback=s.get('status')
    if status is None:require(feedback is None,'Spurious action status')
    else:
        require(feedback and feedback.get('text')==status and feedback.get('role')=='status' and status in g.get('notices',[]),'Action status missing/rewritten')
        box(feedback)
    picker=s.get('picker'); model=m.get('picker')
    if not opened:
        require(picker is None and model is None,'Picker leaked after close/selection')
        return
    require(isinstance(model,dict) and set(model)=={'generation','token','ready','notice'}
            and model['generation']==m['comments']['generation'] and R['integer'](model['token'],1)
            and model['ready'] is ready and model['notice']==(None if ready else STALE), 'Picker ownership/readiness changed')
    require(picker and picker.get('token')==str(model['token']) and picker.get('ready')==str(ready).lower()
            and picker.get('query')==('' if case=='reaction-selection' and i==1 else '+1')
            and g.get('action',{}).get('editorText') is None, 'Picker DOM/model/search custody differs')
    for key in ('queryBox','close','dialog'):box(picker.get(key))
    options=picker.get('options',[]);names=[o.get('name') for o in options]
    require(0<len(options)<=4096 and len(set(names))==len(names) and all(isinstance(n,str) and n for n in names), 'Picker options missing/duplicated')
    for option in options:
        require(option.get('disabled') is (not ready) and option.get('selected') in ('true','false')
                and isinstance(option.get('glyph'),str) and type(option.get('media')) is bool
                and type(option.get('visible')) is bool and type(option.get('hit')) is bool,'Stale picker left selectable options')
    if picker['query']=='+1':
        require(names==['+1'],'Searched selected base absent or stale unfiltered options retained')
        box(next(o for o in options if o['name']=='+1'))
    notice=picker.get('notice')
    if ready:
        total=1974 if picker['query']=='' else 1
        require(len(options)==min(120,total) and notice and notice.get('text')==f'{len(options)} of {total} emoji',
                'Ready picker result count changed')
    else:require(notice and notice.get('text')==STALE,'Stale source warning missing')
    box(notice)


EVENT_KEYS=('read','loading','error','page','pageMessages','live','accepted','editCustody','visibleTs','liveTs','acceptedTs','liveReactions','deleted','reactions','reveal')


def validate(fixture,receipt,directory,*,expected_case=None,dimensions=None,png=True):
    case=receipt.get('case');require(expected_case is None or case==expected_case,'Wrong selection case')
    rows,initial,_=common(fixture,receipt,directory,case,dimensions,png);original=source_proof(rows,case)
    ids=[a['id'] for a in rows[-1]['model']['reactionActions']['attempts']]
    require(len(ids)==(1 if case=='reaction-selection-custody' else 4) and all(R['integer'](n,1) for n in ids)
            and ids==sorted(set(ids)), 'Action IDs missing/reused')
    target=dict(document=initial['initialDocument'],record=initial['records'][READ['KNOWN']],channel=CHANNEL,root=ROOT)
    deadline=rows[4]['model']['reactionActions']['attempts'][2]['retryAt'] if case=='reaction-selection-failures' else 0
    require(case!='reaction-selection-failures' or R['integer'](deadline,1),'Issued429 deadline missing')
    raw=page(reactions(case,0));future=page(reactions('reaction-selection-failures',3))
    previous_read=None;inputs=[];releases=[];server=[];socket=[];changes=[];tokens=[]
    for i,row in enumerate(rows):
        m,g,p=row['model'],row['geometry'],row['proof'];c=m['comments'];spec=specs(case,i)
        require(exact(m.get('reactionActions'),attempts(case,i,ids,target,deadline)),'Exact choice attempt outcome/custody changed')
        require(exact(m.get('reactionLedger'),wire(case,i,ids)),'Resolved original wire/disposition duplicated or changed')
        expected_page=future if case=='reaction-selection-failures' and i>=3 else raw
        typed=[I['typed'](v) for v in expected_page['messages']];typed[1]['reactions']=reactions(case,i)
        require(exact(c.get('page'),expected_page) and exact(c.get('pageMessages'),typed) and exact(p.get('initialResponse'),raw),
                'Original raw Page or exact typed reaction body changed')
        rx=[dict(name=r['name'],count=1) for r in reactions(case,i)]
        require(exact(c.get('reactions'),[dict(ts=FIRST,reactions=rx)]) and c.get('visibleTs')==[FIRST]
                and len(g.get('rows',[]))==1 and g['rows'][0].get('ts')==FIRST and exact(g['rows'][0].get('reactions'),rx)
                and all(g['rows'][0].get('text','').count(t)==1 for t in ('First page comment.','Review ready \u4e2d\U0001f642.')),'Reply or sibling reactions changed')
        native(row,case,i)
        loading=case=='reaction-selection-custody' and i==5;rate=case=='reaction-selection-failures' and i==4
        X['reads'](row,initial['ledger']['requests'],READS[case][i],int(i>=5) if case=='reaction-selection-custody' else spec['count'])
        require(c.get('loading') is loading and c.get('error') is None,'Unexpected read busy/error state')
        if rate:require(c.get('retryAt')==deadline and type(c.get('retryAfter')) is int and 0<c['retryAfter']<=2,'Issued429 cooldown absent')
        else:require(exact(c.get('retryAt'),0) and exact(c.get('retryAfter'),0),'Cooldown not actually retired')
        for control in g['controls']:
            blocked=loading or rate if control['id'] in ('slack-list-comments-next','slack-list-comments-refresh') else rate if control['id']=='slack-list-comment-send' else False
            require(control.get('disabled') is blocked,'Picker changed unrelated control admission')
        serial_changes=(4,7,8,9) if case=='reaction-selection' else (1,2,3,4,6) if case=='reaction-selection-failures' else (5,)
        require(R['integer'](c.get('read'),1),'Read serial missing')
        if previous_read is not None:require(c['read']>previous_read if i in serial_changes else c['read']==previous_read,'Unexpected automatic read/retry')
        previous_read=c['read']
        if case=='reaction-selection':
            if i==1:inputs.append(dict(stage=row['stage'],value=dict(ok=True,ts=FIRST)))
            if i==2:
                names=p['inputs'][-1]['value'].get('names');require(isinstance(names,list) and '+1' in names and names==[o['name'] for o in g['selection']['picker']['options']],'Search result provenance changed')
                inputs.append(dict(stage=row['stage'],value=dict(ok=True,query='+1',names=names)))
            if i==3:inputs.append(dict(stage=row['stage'],value=dict(ok=True)))
            picked={4:'+1',7:'+1',8:CUSTOM,9:ALIAS}.get(i)
            if i==5:
                socket=[dict(stage=row['stage'],team='T1',generation=7,parsed=True,frame=dict(type='reaction_added',user=USER,reaction=TONE,item=dict(type='message',channel=CHANNEL,ts=FIRST)),
                             before={k:rows[4]['model']['comments'][k] for k in EVENT_KEYS},after={k:c[k] for k in EVENT_KEYS})]
                server.append(dict(stage='socket-added',raw=page(reactions(case,5))))
            if i==6:releases.append(dict(stage=row['stage'],value=dict(delivered=True,cancelledBeforeReply=False)))
            if i==8:server.append(dict(stage='exact-removed',raw=raw))
            if i==9:server.append(dict(stage='custom-added',raw=page(reactions(case,8))))
        elif case=='reaction-selection-failures':
            picked='eyes' if i in (1,2,4,6) else None
            if i==3:server.append(dict(stage=row['stage'],raw=future))
            if i==7:releases.append(dict(stage=row['stage'],value=dict(delivered=True,cancelledBeforeReply=False)))
        else:
            picked='+1' if i in (2,4) else 'eyes' if i==5 else None
            if i in (2,4,6):
                before=copy.deepcopy(rows[i-1]['model']['selectionSource']);after=copy.deepcopy(m['selectionSource'])
                if i==2:
                    after['catalog']=None
                    changes.append(dict(stage=row['stage'],kind='accepted-custom-metadata',name=ALIAS,value='alias:eyes',before=before,after=after))
                else:changes.append(dict(stage=row['stage'],kind='pref_change',frame=dict(type='pref_change',name='preferred_skin_tone',value=5 if i==4 else 6),team='T1',generation=7,before=before,after=after))
            if i==6:releases.append(dict(stage=row['stage'],value=dict(delivered=True,cancelledBeforeReply=False)))
        if picked is not None:
            inp=p['inputs'][-1];token=inp.get('value',{}).get('token');require(isinstance(token,str) and token.isdecimal() and int(token)>0,'Original choice token missing')
            stale=case=='reaction-selection-custody' and i in (2,4)
            if stale:require(token==str(m['picker']['token']),'Refused choice used successor picker')
            else:require(token not in tokens,'Choice replayed original picker');tokens.append(token)
            inputs.append(dict(stage=row['stage'],value=dict(ok=True,name=picked,disabled=stale,token=token),**({'unchanged':True} if stale else {})))
        require(exact(p.get('inputs'),inputs) and exact(p.get('releases'),releases) and exact(p.get('serverUpdates'),server)
                and exact(p.get('socket'),socket) and exact(p.get('sourceChanges'),changes),'Original input/source/socket/release provenance changed')
        require(all(p.get(k) is None for k in ('heldResponse','release','originalMessage','serverResponse')),'Unrelated original response introduced')
    if case=='reaction-selection':
        require(rows[1]['model']['picker']['token']==rows[2]['model']['picker']['token'],'Search replaced original picker')
        require(rows[4]['geometry']['active']==rows[5]['geometry']['active']==rows[6]['geometry']['active'],'Socket/ACK moved focus')
    if case=='reaction-selection-custody':
        require(rows[1]['model']['picker']['token']==rows[2]['model']['picker']['token']
                and rows[3]['model']['picker']['token']==rows[4]['model']['picker']['token']
                and rows[1]['model']['picker']['token']!=rows[3]['model']['picker']['token'],'Retirement borrowed successor picker')
    return receipt

