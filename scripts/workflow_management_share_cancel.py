"""Local failed Share removal; never a server deletion or delivery reversal."""
import copy
from pathlib import Path

import workflow_management_share as S
import workflow_management_first_publish as F
from workflow_management_delete import exact, integer, require

CASES = ('rejected', 'lost-response')
STAGE = 'share-cancelled'


def operation(observation):
    attempts = observation.get('firstPublish', {}).get('attempts')
    require(isinstance(attempts, list) and attempts, 'Original Share operation missing')
    value = attempts[-1].get('confirmationShare', {}).get('operation')
    require(isinstance(value, dict), 'Original Share operation absent')
    return value


def exposed(value, viewport, rectangle):
    require(isinstance(value, dict), 'Original Cancel geometry missing')
    rectangle(value.get('rect'), viewport)
    point = value.get('hitPoint'); require(isinstance(point, dict), 'Original Cancel was not exposed')
    r = value['rect']; x, y = point.get('x'), point.get('y')
    require(type(x) in (int, float) and type(y) in (int, float)
            and max(0,r['left']) <= x < min(viewport[0],r['right'])
            and max(0,r['top']) <= y < min(viewport[1],r['bottom']),
            'Cancel hitpoint outside original exposed control')


def validate(receipt, fixture, captures, viewport, rectangle, validate_png, *, png=True):
    case = receipt.get('shareCase')
    require(receipt.get('shareCancelCheck') is True and receipt.get('shareDraftCustody') is True
            and case in CASES, 'Cancel requires explicitly requested failed Share with draft custody')
    rows = receipt.get('stages')
    require(isinstance(rows, list) and rows and rows[-1].get('stage') == 'destination',
            'Cancel must follow actual failed Share destination')
    value = receipt.get('shareCancel')
    require(isinstance(value, dict) and set(value) == {'before','activation','after','quiet','capture','captures'},
            'Local Cancel evidence inventory')
    before, after = value.get('before'), value.get('after')
    require(exact(before, F.observation(rows[-1])), 'Cancel did not retain original destination observation')
    require(isinstance(after, dict) and set(after) == set(before), 'Cancel observation fields changed')
    require(exact(after.get('server'), before['server']),
            'Local Cancel dispatched work or changed original server application/receipt')
    client_id, local_ts = S.ledger(case, 'destination', before['server'])
    S.message_proof(case, 'destination', rows[-1], client_id, local_ts)
    old = operation(before)
    require(old.get('outcome') == ('rejected' if case == 'rejected' else 'unknown')
            and old.get('finished') is True and old.get('queued') is True
            and old.get('locallyCancelled') is False, 'Sending/acknowledged/already-cancelled row is not cancellable')
    S.operation_proof(case, 'destination', before['firstPublish']['attempts'][-1]['confirmationShare'], before['server'])
    expected_product = copy.deepcopy(before['firstPublish'])
    expected_product['attempts'][-1]['confirmationShare']['operation']['locallyCancelled'] = True
    require(exact(after.get('firstPublish'), expected_product),
            'Cancel changed immutable Share request/outcome/ACK or failed to retain local cancellation')

    expected_model = copy.deepcopy(before['model'])
    expected_model['shareCustody'].update(messages=[], pending=[], delivery={})
    require(exact(after.get('model'), expected_model),
            'Cancel removed unrelated state or failed to remove original local row/pending/status')
    native = before['native']; current = after.get('native')
    require(isinstance(current, dict) and native.get('ok') is True and current.get('ok') is True,
            'Cancel original native observation failed')
    witness = native.get('shareCancelRow')
    require(isinstance(witness, dict) and witness.get('originalNodes') is True,
            'Original failed row and Cancel binding absent')
    row = witness.get('row', {}); button = witness.get('cancel', {})
    row_id = integer(row.get('nodeId'), 'Original failed row node', 1)
    button_id = integer(button.get('nodeId'), 'Original Cancel node', 1)
    require(row_id != button_id and row.get('messageId') == local_ts and row.get('connected') is True,
            'Cancel attached to another original row')
    status = 'Not sent' if case == 'rejected' else 'Delivery unconfirmed'
    require(row.get('status') == status, 'Original failure status confused with pending/ACK')
    timeline = native.get('shareTimeline', {})
    native_rows = timeline.get('rows')
    require(isinstance(native_rows, list) and len(native_rows) == 1 and timeline.get('connected') is True,
            'Original failed destination row inventory')
    painted = native_rows[0]
    require(painted.get('messageId') == local_ts and painted.get('nodeId') == row_id
            and exact(row.get('text'), painted.get('text')) and exact(row.get('links'), painted.get('links')),
            'Cancel witness differs from actual rich destination row')
    require(S.MESSAGE.split('\n')[0] in row['text'] and 'Please take a look.' in row['text']
            and any(link.get('href') == S.URL for link in row['links']), 'Original failed rich content/link absent')
    require(button.get('text') == 'Cancel' and button.get('label') == 'Cancel failed workflow message'
            and button.get('connected') is True and button.get('displayed') is True
            and button.get('disabled') is False, 'Original enabled local Cancel control missing')
    exposed(button, viewport, rectangle)
    original = dict(rowNodeId=row_id, cancelNodeId=button_id, rowConnected=True, cancelConnected=True)
    require(exact(native.get('shareCancelOriginal'), original), 'Original failed node witness changed')

    action = value.get('activation')
    require(isinstance(action, dict) and action.get('ok') is True and action.get('kind') == 'click'
            and action.get('provenance') == 'synthetic-dom-click' and action.get('originalNodes') is True
            and action.get('localTs') == local_ts and action.get('rowNodeId') == row_id
            and action.get('nodeId') == button_id, 'Cancel did not use the original failed row control')
    require(exact(action.get('event'), dict(type='click', isTrusted=False, targetNodeId=button_id)),
            'Original Cancel click event missing or fabricated as trusted')
    exposed(action, viewport, rectangle)
    expected_native = copy.deepcopy(native)
    expected_native['shareTimeline']['rows'] = []
    expected_native['shareCancelRow'] = None
    expected_native['shareCancelOriginal'].update(rowConnected=False, cancelConnected=False)
    require(exact(current, expected_native), 'Cancel native removal changed unrelated controls/drafts or retained the row')

    quiet = value.get('quiet', {})
    require(quiet.get('observedMs') == 500 and exact(quiet.get('before'), after)
            and exact(quiet.get('after'), after), 'Local Cancel was not quiet or replayed work')
    capture = value.get('capture', {})
    require(exact(capture.get('before'), after) and exact(capture.get('after'), after),
            'Cancel screenshot changed its original observation')
    path = Path(capture.get('path', ''))
    require(path.resolve().parent == Path(captures).resolve() and path.name == f'{fixture}-{STAGE}.png'
            and exact(value.get('captures'), {STAGE: str(path)}), 'Cancel PNG ownership/inventory')
    dimensions = capture.get('pngDimensions')
    require(isinstance(dimensions, list) and len(dimensions) == 2
            and all(type(n) is int and n > 0 for n in dimensions)
            and all(exact(r.get('geometry', {}).get('pngDimensions'), dimensions) for r in rows),
            'Cancel PNG differs from original physical window')
    require(current.get('shareDrafts', {}).get('windowOriginal') is True,
            'Cancel screenshot belongs to another window')
    if png:
        validate_png(path, dimensions)
    return after
