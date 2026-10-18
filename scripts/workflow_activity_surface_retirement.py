"""Cross-generation custody checks for closed source-retirement receipts."""
import copy

STAGES = {case: ('source', 'opened', 'dirty', 'held', 'retired', 'reopened')
          for case in ('execution-thread-retirement', 'execution-pins-retirement')}
SUCCESSOR_CHANNEL = 'Successor channel draft \u4e2d\U0001f642'
SUCCESSOR_THREAD = 'Successor thread reply \u4e2d\U0001f642'


def transition(rows, row, thread, require):
    held, retired = rows[3], row
    exercise = retired['proof'].get('exercise') or {}
    before, after = held['proof']['source'], retired['proof']['source']
    require(exercise.get('fixtureDrivenSourceRefresh') is True
            and exercise.get('kind') == ('same-root-thread-reopen' if thread else 'same-channel-pins-refresh')
            and exercise.get('beforeSource') == before and exercise.get('afterSource') == after
            and exercise.get('beforeModel') == held['model'] and exercise.get('afterModel') == retired['model'],
            'Missing exact actual source-retirement observations')
    require(exercise.get('heldVisit') == held['model']['entry']['visit'], 'Retirement changed original held visit')
    release = exercise.get('release', {})
    require(release.get('delivered') is False and release.get('cancelledBeforeReply') is True,
            'Retired source response delivered')
    wanted = copy.deepcopy(before)
    if thread:
        require(type(before.get('threadGeneration')) is int and
                after.get('threadGeneration') == before['threadGeneration'] + 1,
                'Same-root reopen did not advance exactly one thread generation')
        require(isinstance(after.get('threadScope'), str) and after['threadScope']
                and after['threadScope'] != before.get('threadScope'), 'Thread successor scope did not change')
        wanted['threadGeneration'] = after['threadGeneration']
        wanted['threadScope'] = after['threadScope']
    else:
        require(type(before.get('pins', {}).get('generation')) is int and
                after.get('pins', {}).get('generation') == before['pins']['generation'] + 1,
                'Same-channel refresh did not advance exactly one Pins generation')
        wanted['pins']['generation'] = after['pins']['generation']
    require(after == wanted, 'Source refresh changed raw bag, owner, draft or unrelated generation')
    documents = after.get('documents', [])
    require(len(documents) == 1 and exercise.get('retainedActivity') ==
            [dict(document=documents[0]['id'], page=None, detail=None, detailRow=None)],
            'Late response published into retained Activity document')
    successor = exercise.get('successorWitness', {})
    reset = successor.get('reset', {})
    prior = held['proof']['nativeSurface']['actionKey']
    require(exercise.get('successorWitnessReset') is True and reset.get('ok') is True
            and reset.get('reset') is True and reset.get('previousActionKey') == prior,
            'Successor witness replaced without exact original key')
    require(successor.get('nativeSurface') == retired['proof']['nativeSurface']
            and retired['proof']['nativeSurface'].get('actionKey') != prior,
            'Retired action key or native witness reused')
    require(successor.get('channelDraft') == (SUCCESSOR_CHANNEL if thread else None)
            and successor.get('threadDraft') == (SUCCESSOR_THREAD if thread else None),
            'Wrong successor native draft seeding')


def held_or_reopened(rows, row, require):
    exercise = row['proof'].get('exercise') or {}
    if row['stage'] == 'held':
        visit = row['model']['entry']['visit']
        require(exercise.get('heldVisit') == visit and exercise.get('heldSerial') == 3
                and visit != rows[1]['model']['entry']['visit'], 'Wrong original held source visit/read')
    elif row['stage'] == 'reopened':
        old, new = rows[3]['model']['entry']['visit'], row['model']['entry']['visit']
        require(exercise.get('heldVisit') == old and exercise.get('reopenedVisit') == new
                and exercise.get('newVisit') is True and new != old,
                'Successor entry reused retired workflow visit')
