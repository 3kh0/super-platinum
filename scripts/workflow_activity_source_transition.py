"""Exact canonical source transitions and stale callback evidence for replacement cases."""
import copy
import json
from workflow_activity_surfaces import ROOT, TS, THREAD_COMPOSER
from workflow_activity_surface_retirement import SUCCESSOR_CHANNEL, SUCCESSOR_THREAD

SUCCESSOR_ROOT = '1781524750.000001'
SUCCESSOR_TS = '1781524850.000001'
SUCCESSOR_OPEN = 'View successor workflow execution'
REMOVAL_TS = '1781524999.000001'


def successor_message(message, require):
    require(isinstance(message, dict) and message.get('ts') == SUCCESSOR_TS
            and message.get('channel') == 'C2' and message.get('thread_ts') == SUCCESSOR_ROOT,
            'Successor message has wrong canonical identity')
    elements = [e for b in message.get('blocks', []) if b.get('type') == 'actions'
                for e in b.get('elements', [])]
    require(len(elements) == 1, 'Successor action must be distinct and unique')
    button = elements[0]
    require(button.get('type') == 'button' and button.get('action_id') == 'workflow_error_view_logs'
            and button.get('accessibility_label') == SUCCESSOR_OPEN
            and button.get('text') == dict(type='plain_text', text=SUCCESSOR_OPEN),
            'Wrong successor action label/type')
    try:
        value = json.loads(button.get('value', ''))
    except (ValueError, TypeError):
        value = None
    require(value == dict(type='wfb', wf_id='WF_REVIEW', execution_id='E_REVIEW'),
            'Successor action targets another workflow/execution')


def source(value, thread, successor, h):
    require = h['require']
    revisions = value.get('documentRevisions')
    docs = value.get('documents', [])
    require(isinstance(revisions, list) and len(revisions) == len(docs)
            and [r.get('id') for r in revisions] == [d.get('id') for d in docs]
            and all(type(r.get('revision')) is int and r['revision'] >= 0 for r in revisions),
            'Missing exact document revision observations')
    require(value.get('originalPinVersion') == (REMOVAL_TS if not thread and successor else None),
            'Original pin removal version not recorded exactly')
    if thread:
        require(value.get('threadRoot') == (SUCCESSOR_ROOT if successor else ROOT),
                'Selected thread root did not match actual stage')
        require(value.get('threadComposerModel') == (SUCCESSOR_THREAD if successor else THREAD_COMPOSER) + '.',
                'Thread autosize model lost or confused with native-only draft')
        old, new = value.get('threadSource'), value.get('successorThreadSource')
        for bag, root in ((old, ROOT), (new, SUCCESSOR_ROOT)):
            require(isinstance(bag, list) and len(bag) == 2 and bag[0].get('ts') == root
                    and bag[0].get('blocks') == [], 'Missing original or pre-existing successor canonical bag')
        h['validate_execution_source']([old[1]])
        require(old[1].get('channel') == 'C2' and old[1].get('thread_ts') == ROOT,
                'Original thread message identity changed')
        successor_message(new[1], require)
        require(value.get('selectedThreadSource') == (new if successor else old),
                'Selected source does not match selected canonical thread')
    else:
        require(value.get('threadRoot') is None and value.get('threadSource') is None
                and value.get('selectedThreadSource') is None and value.get('successorThreadSource') is None,
                'Pins fabricated a thread source')
        pins = value.get('pins', {})
        require(pins.get('scopeCurrent') is True and pins.get('channel') == 'C2'
                and pins.get('loaded') is True and type(pins.get('generation')) is int,
                'Pins source is not current/loaded')
        items = pins.get('items')
        ids = [SUCCESSOR_TS] if successor else [TS, SUCCESSOR_TS]
        require(isinstance(items, list) and len(items) == len(ids), 'Wrong original/surviving pin count')
        for item, ts in zip(items, ids):
            require(item.get('key') == 'message:' + ts and item.get('kind') == 'message'
                    and item.get('channel') == 'C2', 'Wrong pinned message identity/order')
            if ts == TS:
                h['validate_execution_source']([item.get('message', {})])
                require(item['message'].get('channel') == 'C2' and item['message'].get('thread_ts') == ROOT,
                        'Original pin changed channel/root')
            else:
                successor_message(item.get('message', {}), require)


def transition(rows, row, thread, require):
    held = rows[3]
    ex = row['proof'].get('exercise') or {}
    before, after = held['proof']['source'], row['proof']['source']
    require(ex.get('fixtureDrivenSourceReplacement') is True
            and ex.get('kind') == ('thread-root-replacement' if thread else 'originating-pin-removal')
            and ex.get('beforeSource') == before and ex.get('afterSource') == after
            and ex.get('beforeModel') == held['model'] and ex.get('afterModel') == row['model']
            and ex.get('heldVisit') == held['model']['entry']['visit'],
            'Missing actual original source/read observations')
    require(ex.get('release') == dict(delivered=False, cancelledBeforeReply=True),
            'Retired original reply delivered or not cancelled')
    wanted = copy.deepcopy(before)
    if thread:
        require(ex.get('mutation') == dict(kind='begin-thread', previousRoot=ROOT, nextRoot=SUCCESSOR_ROOT),
                'Missing actual root replacement mutation')
        require(after.get('threadGeneration') == before['threadGeneration'] + 1
                and isinstance(after.get('threadScope'), str) and after['threadScope']
                and after['threadScope'] != before['threadScope'], 'Wrong thread generation/scope advance')
        wanted.update(threadRoot=SUCCESSOR_ROOT, selectedThreadSource=before['successorThreadSource'],
                      threadGeneration=after['threadGeneration'], threadScope=after['threadScope'],
                      threadComposerModel=SUCCESSOR_THREAD + '.')
    else:
        require(ex.get('mutation') == dict(kind='pin-removed-event', channel='C2', messageTs=TS,
                                           eventTs=REMOVAL_TS, applied=True), 'Missing actual pin removal event')
        require(after['pins']['generation'] == before['pins']['generation'] + 1,
                'Pin removal did not advance exactly one generation')
        wanted['pins']['generation'] += 1
        wanted['pins']['items'] = [item for item in before['pins']['items'] if item['key'] != 'message:' + TS]
        wanted['originalPinVersion'] = REMOVAL_TS
    require(after == wanted, 'Replacement changed retained bag, survivor pin, owner, document or unrelated source')
    documents = after['documents']
    require(len(documents) == 1 and ex.get('retainedActivity') ==
            [dict(document=documents[0]['id'], page=None, detail=None, detailRow=None)],
            'Late reply published into retained Activity')
    prior = held['proof']['nativeSurface']['actionKey']
    absent = ex.get('originalActionAbsent', {})
    require(absent.get('ok') is True and absent.get('originalActionKey') == prior
            and absent.get('originalActionCount') == 0 and type(absent.get('originalActionConnected')) is bool,
            'Retired action still exists or lacks original-key DOM proof')
    stale = ex.get('staleCallback', {})
    require(stale.get('sourceCurrent') is False and stale.get('presentationEnabled') is False
            and stale.get('unchanged') is True and stale.get('requestsBefore') == stale.get('requestsAfter') == 3
            and stale.get('beforeModel') == stale.get('afterModel') == row['model'],
            'Stale callback authorized work or mutated original model/ledger')
    stale_source = stale.get('beforeSource')
    require(isinstance(stale_source, dict) and stale_source == stale.get('afterSource'),
            'Stale callback mutated source/drafts/revisions')
    expected_stale = copy.deepcopy(after)
    if thread:
        # Actual normal input/autosize follows the callback; only this model field may differ.
        require(isinstance(stale_source.get('threadComposerModel'), str), 'Missing pre-seed thread model')
        expected_stale['threadComposerModel'] = stale_source['threadComposerModel']
    require(stale_source == expected_stale, 'Stale callback was tested against a foreign source/revision')
    successor = ex.get('successorWitness', {})
    reset = successor.get('reset', {})
    require(ex.get('successorWitnessReset') is True and reset.get('ok') is True and reset.get('reset') is True
            and reset.get('previousActionKey') == prior
            and successor.get('nativeSurface') == row['proof']['nativeSurface']
            and row['proof']['nativeSurface'].get('actionKey') != prior,
            'Missing fresh successor witness or retired action reused')
    require(successor.get('channelDraft') == (SUCCESSOR_CHANNEL if thread else None)
            and successor.get('threadDraft') == (SUCCESSOR_THREAD if thread else None),
            'Successor native draft differs or Pins claims a composer')
