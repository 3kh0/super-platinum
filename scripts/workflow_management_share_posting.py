"""Independent restricted-send metadata recovery; never a retry or permission UI claim."""
import copy
import json
from pathlib import Path

import workflow_management_share as S
import workflow_management_share_resend as R
import workflow_management_share_cancel as C
import workflow_management_first_publish as F
from workflow_management_delete import exact, require, integer

CASES = ('success', 'read-failure', 'held-newer', 'retry-restricted', 'missing-policy')
STAGES = {c: (('posting-retry-pending',) if c == 'retry-restricted' else ()) +
          ('posting-pending',) + (('posting-newer',) if c == 'held-newer' else ()) +
          ('posting-settled',) for c in CASES}
REASON = 'posting_permission_send_message_error'
SCHEDULING = 'closed-fixture-original-response-held'


def mode(case, share_case, drafts, cancel, resend):
    require(case is None or case in CASES and drafts and not cancel and resend is None
            and share_case == ('lost-response' if case == 'retry-restricted' else 'rejected'),
            'Posting refresh requires exact initial failure, draft custody and no public recovery mode')


def identity(receipt, case):
    require(receipt.get('sharePostingCheck', False) is (case is not None)
            and receipt.get('sharePostingCase') == case, 'Posting refresh mode identity')
    require(case is not None or receipt.get('sharePosting') is None, 'Unrequested posting refresh evidence')


def response(newer=False):
    channel = copy.deepcopy(S.A.CHANNELS[S.A.DEST])
    channel.update(is_read_only=not newer, is_thread_only=newer,
                   properties={'posting_refresh_marker': 'newer-source' if newer else 'original-refresh'})
    return dict(ok=True, channel=channel,
                opaque_receipt='newer-posting-response' if newer else 'original-posting-response')


def empty(case):
    return dict(case=case, requests=[], responses=[], settlements=[], releases=[], held=[], newer=[],
                networkCalls=0, scheduling=SCHEDULING)


def data(value):
    return value['server']['confirmationAdd']['share']['posting']


def seed_policy(case, server):
    """Original Boot/Init acquisition, not a privately assigned permission flag."""
    team=dict(id='TWORKFLOW',name='Home fixture')
    if case!='missing-policy':team['plan']='plus'
    seed=server.get('seed',{})
    require(exact(seed.get('boot',{}).get('team'),team)
            and exact(seed.get('init',{}).get('team'),team)
            and exact(seed.get('init',{}).get('workspaces'),[team])
            and exact(seed.get('canonicalTeams'),{'TWORKFLOW':team}),
            'Posting policy lacks original acquired plan/known-free source')


def ledger(case, phase, value, original, newer=None):
    """All attempts retain exact wire/raw provenance independently from publication."""
    current = data(value)
    require(isinstance(current, dict) and set(current) == set(empty(case)), 'Posting ledger inventory')
    if phase == 'empty':
        require(exact(current, empty(case)), 'Posting read occurred before a restricted failure')
        return
    requests = current.get('requests')
    require(isinstance(requests, list) and len(requests) == 1, 'Posting read duplicated/absent')
    request = requests[0]
    generation = integer(request.get('generation'), 'Posting original generation')
    require(exact(request, dict(serial=1, generation=generation, method='conversations.info',
            body=[['channel', S.A.DEST], ['_x_reason', REASON]],
            endpoint='https://example.test/api/conversations.info', retrySafe=True, owner=S.OWNER)),
            'Posting request method/reason/original owner changed')
    expected = empty(case)
    expected.update(requests=requests, responses=[dict(serial=1, error='offline') if case == 'read-failure'
                    else dict(serial=1, raw=response())], newer=[] if newer is None else [newer])
    if phase == 'settled':
        expected.update(settlements=[dict(serial=1, returned=True)],
                        releases=[dict(serial=1, receiverRetired=False, delivered=True)])
    else:
        require(phase in ('pending', 'newer'), 'Unknown posting ledger phase')
        expected['held'] = [1]
    require(exact(current, expected), 'Posting original response/hold/settlement changed')
    # This excludes only independently checked new ledgers; every original send,
    # About read and server-applied message stays immutable.
    a, b = copy.deepcopy(value['server']), copy.deepcopy(original['server'])
    for server in (a, b):
        share = server['confirmationAdd']['share']
        share.pop('posting', None)
        if case == 'retry-restricted': share.pop('resend', None)
    require(exact(a, b), 'Background metadata read changed original Slack operations/server delivery')


def policy(value, original, published=None, *, missing=False):
    """Canonical, raw authority and effective flags are separate measured facts."""
    p = value['model'].get('sharePosting'); old = original['model']['sharePosting']
    keys = {'authorityCurrent','rawCurrent','raw','originalRaw','originalCurrent','effective','effectiveFailure',
            'canonicalCurrent','canonical','typed'}
    require(isinstance(p, dict) and set(p) == keys, 'Posting observation inventory')
    require(all(p.get(k) is True for k in ('authorityCurrent','rawCurrent','canonicalCurrent')),
            'Posting publication lost current account/source authority')
    require(exact(p.get('originalRaw'), old.get('originalRaw'))
            and isinstance(p['originalRaw'], dict), 'Original raw policy witness replaced')
    if published is None:
        require(exact(p, old), 'Failed/pending read changed current posting metadata')
        return
    raw = response(published)['channel']
    require(exact(p.get('raw'), raw) and p.get('originalCurrent') is False,
            'Published raw response or old-source retirement incorrect')
    if missing:
        require(published is False and p.get('effective') is None
                and p.get('effectiveFailure')=='Policy(FeatureAccess)',
                'Missing feature policy must remain explicitly unknown, never a grant')
    else:
        require(exact(p.get('effective'), dict(readOnly=not published, threadOnly=published, permitsRootPost=False))
                and p.get('effectiveFailure') is None,
                'Effective posting companion did not follow acquired policy')
    for key in ('canonical', 'typed'):
        expected = copy.deepcopy(old[key])
        require(isinstance(expected, dict) and expected.get('id') == S.A.DEST, 'Original channel source missing')
        for field in ('is_read_only','is_thread_only','properties'): expected[field] = copy.deepcopy(raw[field])
        require(exact(p.get(key), expected), 'Canonical/typed publication changed unrelated fields or missed metadata')


def unchanged(value, original, *, retry=False):
    expected = copy.deepcopy(original['model'])
    expected['sharePosting'] = value['model']['sharePosting']
    if retry: expected['shareCustody']['delivery'] = value['model']['shareCustody']['delivery']
    require(exact(value['model'], expected), 'Posting refresh changed message, pending identity or independent drafts')
    expected = copy.deepcopy(original['firstPublish'])
    target = expected['attempts'][-1]['confirmationShare']['operation']
    op = C.operation(value)
    target['postingRefresh'] = op.get('postingRefresh')
    if retry: target['resends'] = op.get('resends')
    require(exact(value['firstPublish'], expected), 'Posting refresh changed immutable original operation')


def refresh(case, phase, value, original):
    rows = C.operation(value).get('postingRefresh')
    if phase == 'empty':
        require(rows == [], 'Posting effect preceded restricted failure'); return
    require(isinstance(rows, list) and len(rows) == 1, 'Posting effect duplicated/absent')
    generation = data(value)['requests'][0]['generation']
    outcome = {'read-failure':'failed','held-newer':'retired'}.get(case,'applied') if phase == 'settled' else 'reading'
    expected = dict(generation=generation, request=dict(method='conversations.info',channel=S.A.DEST,_x_reason=REASON),
                    raw=response() if phase == 'settled' and case != 'read-failure' else None,
                    outcome=outcome, finished=phase == 'settled', applied=outcome == 'applied')
    require(exact(rows, [expected]), 'Posting receipt confused with ACK, failed read, or stale publication')
    expected_generation = 0
    if case == 'retry-restricted': expected_generation = C.operation(value)['resends'][0]['generation']
    require(generation == expected_generation, 'Posting read lost original failure generation')


def failed_native(value, original, viewport, rectangle):
    # R's suppressed native policy is exactly Cancel-only restricted failure.
    R.native('suppressed', 'before', value, original, viewport, rectangle)
    require(value['native']['shareResendRow']['nodeId'] == original['native']['shareResendRow']['nodeId'],
            'Background refresh replaced original failed row')


def retry_proof(value, original, pending, viewport, rectangle):
    """One actual explicit retry with a restricted response, separate from initial unknown."""
    ledger_value = value['server']['confirmationAdd']['share']['resend']
    initial = original['server']['confirmationAdd']['share']
    a,b=copy.deepcopy(value['server']),copy.deepcopy(original['server'])
    for server in (a,b):
        server['confirmationAdd']['share'].pop('posting',None)
        server['confirmationAdd']['share'].pop('resend',None)
    require(exact(a,b), 'Explicit retry altered original server/application ledger')
    require(isinstance(ledger_value, dict) and set(ledger_value) ==
            {'case','requests','responses','attempts','settlements','releases','held','networkCalls','scheduling'},
            'Posting retry ledger inventory')
    raw = dict(ok=False,error='restricted_action_read_only_channel',opaque_receipt='original-share-resend-restricted')
    requests = ledger_value['requests']
    require(isinstance(requests,list) and len(requests)==1, 'Posting retry duplicated dispatch')
    request = requests[0]
    require(set(request)=={'intent','serial','request','owner'} and exact(request['intent'],1)
            and exact(request['serial'],1) and exact(request['owner'],S.OWNER), 'Posting retry owner/intent')
    _, ts = R.request_proof(request['request'], initial['requests'][0]['request'])
    attempts = ledger_value['attempts']
    if not pending:
        require(len(attempts)==1, 'Posting retry result count')
        require(exact({k:v for k,v in attempts[0].items() if k!='responseText'},
                dict(number=1,status=200,outcome='rejected',raw=raw,retryAfterMillis=None,mayHaveApplied=False))
                and exact(json.loads(attempts[0].get('responseText','null')),raw), 'Posting retry original rejection')
    else: require(attempts==[], 'Held retry fabricated result')
    require(exact(ledger_value, dict(case='posting-restricted',requests=requests,
            responses=[dict(intent=1,serial=1,raw=raw,applied=False,deduplicated=True)], attempts=attempts,
            settlements=[] if pending else [dict(intent=1,serial=1,returned=True)],
            releases=[] if pending else [dict(intent=1,serial=1,receiverRetired=False,delivered=True)],
            held=[1] if pending else [],networkCalls=0,scheduling='closed-fixture-hold-until-original-pending-capture')),
            'Posting retry original lifecycle changed')
    resends = C.operation(value).get('resends')
    require(isinstance(resends,list) and len(resends)==1, 'Posting explicit retry operation count')
    generation = integer(resends[0].get('generation'),'Retry generation',1)
    require(exact(resends,[dict(sequence=1,generation=generation,request=request['request'],attempts=attempts,
            raw=None if pending else raw,outcome='sending' if pending else 'rejected',finished=not pending,applied=not pending)]),
            'Posting retry immutable result custody')
    require(exact(value['model']['shareCustody']['delivery'],{} if pending else {ts:'Failed'}),
            'Posting retry delivery state')
    R.native('rejected' if pending else 'suppressed','pending' if pending else 'before',
             value,original,viewport,rectangle)
    unchanged(value,original,retry=True)


def validate(receipt, fixture, captures, viewport, rectangle, validate_png, *, png=True):
    case=receipt.get('sharePostingCase'); identity(receipt,case)
    mode(case,receipt.get('shareCase'),receipt.get('shareDraftCustody'),receipt.get('shareCancelCheck',False),receipt.get('shareResendCase'))
    require(case in CASES, 'Missing requested posting case')
    v=receipt.get('sharePosting')
    require(isinstance(v,dict) and set(v)=={'before','retry','pending','pendingQuiet','newer','release','after','quiet','captures','captureEvidence'},
            'Posting evidence inventory')
    before,pending,after=v['before'],v['pending'],v['after']
    require(C.operation(before).get('resends')==[] and C.operation(before).get('retryBlocked') is False
            and C.operation(before).get('locallyCancelled') is False,'Original recovery state already changed')
    if case=='retry-restricted':
        require(exact(before['server']['confirmationAdd']['share'].get('resend'),dict(case='posting-restricted',
                requests=[],responses=[],attempts=[],settlements=[],releases=[],held=[],networkCalls=0,
                scheduling='closed-fixture-hold-until-original-pending-capture')), 'Retry already dispatched before actual action')
    require(receipt['stages'][-1]['stage']=='destination' and exact(before,F.observation(receipt['stages'][-1])),
            'Posting proof lost original destination')
    for row in receipt['stages']:
        observed=F.observation(row)
        seed_policy(case,observed['server'])
        if case=='retry-restricted' or row['stage'] not in ('share-settled','destination'):
            ledger(case,'empty',observed,before)
        elif data(observed)['requests']:
            ledger(case,'pending',observed,before)
            refresh(case,'pending',observed,before)
        else:
            ledger(case,'empty',observed,before)
            # The effect is reserved synchronously, dispatch runs in its own
            # task; only this pre-dispatch reading state can precede the hold.
            require(exact(C.operation(observed).get('postingRefresh'),[dict(generation=0,
                    request=dict(method='conversations.info',channel=S.A.DEST,_x_reason=REASON),
                    raw=None,outcome='reading',finished=False,applied=False)]), 'Unexpected predispatch posting state')
    observations={'posting-pending':pending,'posting-settled':after}
    if case=='retry-restricted':
        retry=v['retry']; require(isinstance(retry,dict) and set(retry)=={'before','activation','held','quiet','release','after'},'Posting retry proof')
        require(exact(retry['before'],before) and exact(retry['after'],pending),'Posting retry original transition')
        held=retry['held']; ledger(case,'empty',held,before); refresh(case,'empty',held,before)
        retry_proof(held,before,True,viewport,rectangle); R.quiet(retry['quiet'],held)
        action=retry['activation']; row=before['native']['shareResendRow']; button=row['resend']
        R.control(button,'Try again','Retry failed workflow message',viewport,rectangle)
        require(action.get('ok') is True and action.get('kind')=='click' and action.get('provenance')=='synthetic-dom-click'
                and action.get('originalNodes') is True and action.get('localTs')==row['messageId']
                and action.get('rowNodeId')==row['nodeId'] and action.get('nodeId')==button['nodeId']
                and exact(action.get('event'),dict(type='click',isTrusted=False,targetNodeId=button['nodeId'])),
                'Original exposed retry click provenance')
        C.exposed(action,viewport,rectangle)
        require(exact(retry['release'],dict(intent=1,serial=1,receiverRetired=False,delivered=True)), 'Posting retry sender release')
        observations['posting-retry-pending']=held
        for observed in (pending,after): retry_proof(observed,before,False,viewport,rectangle)
    else:
        require(v['retry'] is None and exact(before,pending),'Unrequested retry or changed held destination')
    ledger(case,'pending',pending,before); refresh(case,'pending',pending,before)
    p=pending['model']['sharePosting']
    require(p.get('originalCurrent') is True and exact(p.get('raw'),p.get('originalRaw'))
            and p.get('effectiveFailure') is None
            and exact(p.get('effective'),dict(readOnly=False,threadOnly=False,permitsRootPost=True)), 'Original posting authority facts')
    policy(pending,pending); R.quiet(v['pendingQuiet'],pending)
    newer=None
    if case=='held-newer':
        n=v['newer']; require(isinstance(n,dict) and set(n)=={'before','ingress','after','quiet'} and exact(n['before'],pending),'Newer original response witness')
        newer=n['ingress']; require(exact(newer,dict(kind='synthetic-source-response-ingress',requestDispatched=False,
                raw=response(True),before=p,after=n['after']['model']['sharePosting'])), 'Newer source fabricated network/UI action')
        policy(n['after'],pending,True); ledger(case,'newer',n['after'],before,newer); refresh(case,'newer',n['after'],before)
        unchanged(n['after'],pending); failed_native(n['after'],pending,viewport,rectangle); R.quiet(n['quiet'],n['after'])
        observations['posting-newer']=n['after']
    else: require(v['newer'] is None,'Unrequested newer-source ingress')
    ledger(case,'settled',after,before,newer); refresh(case,'settled',after,before)
    policy(after,pending,None if case=='read-failure' else case=='held-newer',missing=case=='missing-policy')
    for observed in (pending,after):
        unchanged(observed,pending)
        failed_native(observed,pending,viewport,rectangle)
    require(exact(v['release'],dict(serial=1,receiverRetired=False,delivered=True)), 'Posting original sender release')
    R.quiet(v['quiet'],after)
    require(set(v['captures'])==set(v['captureEvidence'])==set(STAGES[case]), 'Posting capture inventory')
    for stage, observed in observations.items():
        cap=v['captureEvidence'][stage]; path=Path(cap.get('path',''))
        require(exact(cap.get('before'),observed) and exact(cap.get('after'),observed),'Posting capture changed original observation')
        require(path.resolve().parent==Path(captures).resolve() and path.name==f'{fixture}-{stage}.png'
                and v['captures'][stage]==str(path),'Posting owned capture path')
        dims=cap.get('pngDimensions')
        require(isinstance(dims,list) and len(dims)==2 and all(type(x)is int and x>0 for x in dims)
                and all(exact(r['geometry']['pngDimensions'],dims) for r in receipt['stages']), 'Posting original physical window dimensions')
        if png:validate_png(path,dims)
    return after
