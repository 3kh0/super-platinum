"""Original failed rich Share retry custody; separate from the initial send."""
import copy
import json
from pathlib import Path

import workflow_management_share as S
import workflow_management_first_publish as F
import workflow_management_share_cancel as C
from workflow_management_delete import exact, require, integer

CASES = ('success', 'rejected', 'held', 'suppressed')
STAGES = {case: ('resend-suppressed',) if case == 'suppressed' else
          ('resend-pending', 'resend-settled') for case in CASES}


def request_proof(value, original):
    """gzYT/Xhlw retry fields, not an equality claim against the initial wire."""
    fields = value.get('fields')
    require(isinstance(fields, list) and len(fields) == 15, 'Resend exact field inventory')
    tail = [['user', 'UWORKFLOW'], ['showResend', 'true'], ['showCancel', 'true'],
            ['is_pending', 'true'], ['isEKMError', 'false'],
            ['isDLPUserWarningError', 'false'], ['skip_dlp_user_warning', 'false']]
    require(exact(fields[8:], tail), 'Resend original failed-row metadata/flags changed')
    initial = copy.deepcopy(value)
    initial['fields'] = fields[:8]
    identity = S.request_proof(initial)
    require(identity == S.request_proof(original) and exact(fields[:8], original['fields']),
            'Resend changed original client ID, timestamp, blocks, unfurl or destination')
    return identity


def raw_reply(case, client_id):
    if case == 'rejected':
        return dict(ok=False, error='not_allowed', opaque_receipt='original-share-resend-rejected')
    value = S.raw_reply('success', client_id)
    value['opaque_receipt'] = 'original-share-resend-ack'
    return value


def ledger(case, phase, server, original_server):
    """Immutable original send plus one separately observed user retry."""
    base_case = 'rejected' if case == 'suppressed' else 'lost-response'
    client_id, local_ts = S.ledger(base_case, 'destination', original_server)
    baseline = copy.deepcopy(original_server)
    current = copy.deepcopy(server)
    before = baseline['confirmationAdd']['share'].pop('resend', None)
    value = current['confirmationAdd']['share'].pop('resend', None)
    require(exact(current, baseline), 'Resend mutated original send/read/server-applied identity')
    keys = {'case','requests','responses','attempts','settlements','releases','held','networkCalls','scheduling'}
    require(isinstance(before, dict) and isinstance(value, dict) and set(value) == keys
            and set(before) == keys, 'Separate Resend ledger inventory')
    empty = dict(case=case, requests=[], responses=[], attempts=[], settlements=[], releases=[], held=[],
                 networkCalls=0, scheduling='closed-fixture-hold-until-original-pending-capture')
    require(exact(before, empty), 'Original destination already contained a user retry')
    if phase == 'before' or case == 'suppressed':
        require(exact(value, empty), 'Suppressed/unrequested retry dispatched work')
        return client_id, local_ts
    require(phase in ('pending','settled'), 'Unknown retry evidence phase')
    require(value.get('case') == case and exact(value.get('networkCalls'), 0)
            and value.get('scheduling') == empty['scheduling'], 'Retry escaped closed scheduling')
    requests = value.get('requests')
    require(isinstance(requests, list) and len(requests) == 1, 'User retry duplicated actual dispatch')
    request = requests[0]
    require(set(request) == {'intent','serial','request','owner'}
            and exact(request.get('intent'), 1) and exact(request.get('serial'), 1)
            and exact(request.get('owner'), S.OWNER), 'Retry original account/intent/serial')
    initial_request = baseline['confirmationAdd']['share']['requests'][0]['request']
    require(request_proof(request['request'], initial_request) == (client_id, local_ts),
            'Retry identity changed')
    raw = raw_reply(case, client_id)
    require(exact(value.get('responses'), [dict(intent=1, serial=1, raw=raw,
            applied=case != 'rejected', deduplicated=True)]), 'Retry raw response or dedup fact changed')
    if phase == 'pending':
        require(exact(value.get('held'), [1]) and value.get('attempts') == []
                and value.get('settlements') == [] and value.get('releases') == [],
                'Pending retry fabricated returned receipt or lost original hold')
    else:
        require(value.get('held') == [] and exact(value.get('settlements'),
                [dict(intent=1, serial=1, returned=True)]) and exact(value.get('releases'),
                [dict(intent=1, serial=1, receiverRetired=False, delivered=True)]),
                'Retry original sender settlement/release')
        attempts = value.get('attempts')
        require(isinstance(attempts, list) and len(attempts) == 1, 'Retry returned attempt count')
        attempt = attempts[0]
        expected = dict(number=1, status=200, outcome='rejected' if case == 'rejected' else 'acknowledged',
                        raw=raw, retryAfterMillis=None, mayHaveApplied=case != 'rejected')
        require(exact({k:v for k,v in attempt.items() if k != 'responseText'}, expected),
                'Retry ACK/outcome/status confused with original failure')
        require(exact(json.loads(attempt.get('responseText', 'null')), raw),
                'Retry original response text differs from parsed raw')
    return client_id, local_ts


def product(case, phase, value, before):
    op = C.operation(value); old = C.operation(before)
    require(old.get('resends') == [] and old.get('locallyCancelled') is False
            and old.get('retryBlocked') is False, 'Original retry admission state')
    require(op.get('locallyCancelled') is False and op.get('retryBlocked') is False,
            'Retry changed cancellation/cooldown state')
    if phase == 'before' or case == 'suppressed':
        require(exact(value['firstPublish'], before['firstPublish']), 'Suppressed retry changed operation')
        return
    resends = op.get('resends')
    require(isinstance(resends, list) and len(resends) == 1, 'Original user retry operation count')
    retry = resends[0]
    require(set(retry) == {'sequence','generation','request','attempts','raw','outcome','finished','applied'},
            'Retry operation field inventory')
    require(exact(retry.get('sequence'), 1), 'User retry sequence changed')
    integer(retry.get('generation'), 'Original retry generation', 1)
    actual = value['server']['confirmationAdd']['share']['resend']
    require(exact(retry.get('request'), actual['requests'][0]['request'])
            and exact(retry.get('attempts'), actual['attempts']), 'Retry product/wire custody differs')
    pending = phase == 'pending'
    expected = 'sending' if pending else 'rejected' if case == 'rejected' else 'acknowledged'
    require(retry.get('outcome') == expected and retry.get('finished') is (not pending)
            and retry.get('applied') is (not pending), 'Retry local settlement/host application changed')
    require(exact(retry.get('raw'), None if pending else actual['responses'][0]['raw']),
            'Retry product replaced/lost original raw response')
    expected_product = copy.deepcopy(before['firstPublish'])
    expected_product['attempts'][-1]['confirmationShare']['operation']['resends'] = resends
    require(exact(value['firstPublish'], expected_product), 'Retry changed original operation or unrelated product state')


def model(case, phase, value, before):
    expected = copy.deepcopy(before['model'])
    c = expected['shareCustody']
    old = c['messages'][0]; ts = old['ts']
    if case != 'suppressed' and phase != 'before':
        c['delivery'] = {} if phase == 'pending' or case != 'rejected' else {ts:'Failed'}
        if phase == 'settled' and case != 'rejected':
            c['messages'][0].update(raw_reply(case, old['client_msg_id'])['message'])
            c['pending'] = []
    require(exact(value['model'], expected), 'Retry changed original rich row/drafts or failed exact reconciliation')


def control(value, text, label, viewport, rectangle):
    require(isinstance(value, dict) and value.get('connected') is True and value.get('displayed') is True
            and value.get('disabled') is False and value.get('text') == text and value.get('label') == label,
            'Original enabled failed-row control missing')
    integer(value.get('nodeId'), 'Original recovery control', 1)
    C.exposed(value, viewport, rectangle)


def native(case, phase, value, before, viewport, rectangle):
    current = value['native']; initial = before['native']
    require(current.get('ok') is True and current.get('shareDrafts', {}).get('windowOriginal') is True,
            'Original retry native window missing')
    # Row reconciliation may change only the actual row/control proofs. The
    # timeline host, draft node/selection and other original controls stay exact.
    expected = copy.deepcopy(initial)
    for key in ('shareResendRow','shareResendOriginal'):
        expected[key] = current.get(key)
    expected['shareTimeline']['rows'] = current.get('shareTimeline', {}).get('rows')
    require(exact(current, expected), 'Retry changed unrelated native host/draft controls')
    row = current.get('shareResendRow'); original = current.get('shareResendOriginal')
    require(isinstance(row, dict) and isinstance(original, dict), 'Original retry row witness absent')
    integer(row.get('nodeId'), 'Actual retry row node', 1)
    require(row.get('connected') is True, 'Retry row detached from destination')
    ts = value['model']['shareCustody']['messages'][0]['ts']
    require(row.get('messageId') == ts, 'Retry native row identity differs from actual model')
    painted = current['shareTimeline']['rows']
    require(len(painted) == 1 and all(exact(row.get(k), painted[0].get(k))
            for k in ('nodeId','messageId','text','links')), 'Retry rich DOM row/independent timeline differ')
    require(S.MESSAGE.split('\n')[0] in row.get('text','') and 'Please take a look.' in row.get('text','')
            and any(x.get('href') == S.URL for x in row.get('links',[])), 'Retry lost rich text/actual link')
    pending = phase == 'pending'
    success = phase == 'settled' and case not in ('rejected','suppressed')
    status = None if success else 'Sending…' if pending else 'Not sent' if case == 'suppressed' or phase == 'settled' else 'Delivery unconfirmed'
    require(row.get('status') == status, 'Retry native delivery label changed')
    if pending or success:
        require(row.get('resend') is None and row.get('cancel') is None, 'Sending/ACK row exposes failed recovery actions')
    else:
        control(row.get('cancel'), 'Cancel', 'Cancel failed workflow message', viewport, rectangle)
        if case == 'suppressed':
            require(row.get('resend') is None, 'Restricted rejection exposes Try again')
        else:
            control(row.get('resend'), 'Try again', 'Retry failed workflow message', viewport, rectangle)
    old = initial['shareResendOriginal']
    require(all(exact(original.get(k), old.get(k)) for k in ('rowNodeId','resendNodeId','cancelNodeId')),
            'Original failed row bindings silently replaced')
    for name, node in [('row',row),('resend',row.get('resend')),('cancel',row.get('cancel'))]:
        expected_connected = node is not None and node.get('nodeId') == original.get(name+'NodeId')
        require(original.get(name+'Connected') is expected_connected, 'Original recovery node connectivity mismatch')


def quiet(value, observation):
    require(isinstance(value, dict) and exact(value.get('observedMs'), 500)
            and exact(value.get('before'), observation) and exact(value.get('after'), observation),
            'Retry replayed work or failed quiet observation')


def validate(receipt, fixture, captures, viewport, rectangle, validate_png, *, png=True):
    case = receipt.get('shareResendCase')
    require(case in CASES and receipt.get('shareResendCheck') is True
            and receipt.get('shareDraftCustody') is True and not receipt.get('shareCancelCheck',False)
            and receipt.get('shareCase') == ('rejected' if case == 'suppressed' else 'lost-response'),
            'Explicit same-identity retry mode missing')
    value = receipt.get('shareResend')
    require(isinstance(value, dict) and set(value) == {'before','activation','held','blocked','release',
            'after','quiet','captureEvidence','captures'}, 'Retry receipt field inventory')
    rows = receipt['stages']; before = value['before']; after = value['after']
    require(rows[-1]['stage'] == 'destination' and exact(before, F.observation(rows[-1])),
            'Retry did not begin at original failed destination')
    original_server = before['server']
    for row in rows:
        require(exact(row['proof']['server']['confirmationAdd']['share'].get('resend'),
                      original_server['confirmationAdd']['share']['resend']), 'Retry work appeared before user action')
    for phase, observation in [('before',before),('settled',after)]:
        ledger(case,phase,observation['server'],original_server)
        product(case,phase,observation,before)
        model(case,phase,observation,before)
        native(case,phase,observation,before,viewport,rectangle)
    if case == 'suppressed':
        require(all(value[k] is None for k in ('activation','held','blocked','release'))
                and exact(after,before), 'Suppressed retry invoked an action')
        observations = {'resend-suppressed':after}
    else:
        held = value['held']
        ledger(case,'pending',held['server'],original_server)
        product(case,'pending',held,before); model(case,'pending',held,before)
        native(case,'pending',held,before,viewport,rectangle)
        action = value['activation']; row = before['native']['shareResendRow']
        button = row['resend']
        require(isinstance(action,dict) and action.get('ok') is True and action.get('kind') == 'click'
                and action.get('provenance') == 'synthetic-dom-click' and action.get('originalNodes') is True
                and action.get('localTs') == row['messageId'] and action.get('rowNodeId') == row['nodeId']
                and action.get('nodeId') == button['nodeId'] and exact(action.get('event'),
                dict(type='click',isTrusted=False,targetNodeId=button['nodeId'])), 'Original Try again click provenance')
        C.exposed(action,viewport,rectangle)
        blocked = value['blocked']
        require(isinstance(blocked,dict) and set(blocked) == {'kind','resendAbsent','cancelAbsent','actionDispatched','quiet'}
                and blocked['kind'] == 'passive-original-control-absence' and blocked['resendAbsent'] is True
                and blocked['cancelAbsent'] is True and blocked['actionDispatched'] is False,
                'Inflight retry did not prove recovery control absence')
        quiet(blocked['quiet'],held)
        require(exact(value['release'],dict(intent=1,serial=1,receiverRetired=False,delivered=True)),
                'Original retry response release mismatch')
        a=held['server']['confirmationAdd']['share']['resend']; b=after['server']['confirmationAdd']['share']['resend']
        require(exact(a['requests'],b['requests']) and exact(a['responses'],b['responses']),
                'Pending original retry request/response replaced on release')
        x=C.operation(held)['resends'][0]; y=C.operation(after)['resends'][0]
        require(x['generation'] == y['generation'] and exact(x['request'],y['request']), 'Retry work owner changed at settlement')
        observations = {'resend-pending':held,'resend-settled':after}
    quiet(value['quiet'],after)
    require(set(value['captureEvidence']) == set(STAGES[case]) and set(value['captures']) == set(STAGES[case]),
            'Retry capture inventory/order')
    for stage, observation in observations.items():
        capture = value['captureEvidence'][stage]; path = Path(capture.get('path',''))
        require(exact(capture.get('before'),observation) and exact(capture.get('after'),observation),
                'Retry screenshot not bound to original observation')
        require(path.resolve().parent == Path(captures).resolve() and path.name == f'{fixture}-{stage}.png'
                and value['captures'][stage] == str(path), 'Retry capture owned path')
        dimensions=capture.get('pngDimensions')
        require(isinstance(dimensions,list) and len(dimensions)==2 and all(type(n) is int and n>0 for n in dimensions)
                and all(exact(r['geometry']['pngDimensions'],dimensions) for r in rows), 'Retry original physical window dimensions')
        if png: validate_png(path,dimensions)
    return after
