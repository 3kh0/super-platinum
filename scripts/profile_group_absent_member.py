"""Strict mixed absent-member receipt oracle; never starts a native process."""
import json
from pathlib import Path

from profile_group_mixed_file import file_cache, file_source, file_tokens

CASES = {
    'mixed-absent-member': ('initial', 'file-held', 'member-held', 'group-held',
                            'success', 'edited', 'saved', 'reopened'),
    'mixed-absent-member-cancel': ('initial', 'file-held', 'member-held', 'cancelled',
                                   'late-settled', 'group-held', 'success', 'edited', 'saved', 'reopened'),
    'mixed-absent-member-offline': ('initial', 'file-held', 'member-held', 'member-offline',
                                    'retry-file-held', 'retry-member-held', 'group-held',
                                    'success', 'edited', 'saved', 'reopened'),
}
ACCOUNT = '00000000-0000-4000-8000-000000000081'
TEAM, SELF, MEMBER = 'TINIT0001', 'UINIT0001', 'UMISSING01'
GROUPS = ('SMISSING01', 'SMISSING02')
MEMBER_NAME = '  Cafe\u0301 Harbor  '
LABELS = ['@@ Cafe\u0301 crew  ', '@@ Cafe\u0301 crew  ', '@Review crew',
          '@' + MEMBER_NAME, '@' + MEMBER_NAME, 'List & Plan', 'List & Plan']
DRAFT = 'Keep my unsaved description with '
EDITED = 'Native edited \u4e2d\U0001f98a & description with '
LOADING = 'Loading reference details. Your description is unchanged.'
UNAVAILABLE = ('Reference details are unavailable or changed while opening. Your description is unchanged. '
               'Retry to load document and mention details.')
LOADING_STAGES = {'file-held', 'member-held', 'retry-file-held', 'retry-member-held', 'group-held'}
EDITORS = {'success', 'edited', 'reopened'}
TAIL = {'success', 'edited', 'saved', 'reopened'}


def original_blocks(text=DRAFT):
    elements = [dict(type='text', text=text)]
    elements += [dict(type='usergroup', usergroup_id=g, fixture_occurrence=i,
                      tab_id='opaque-tab', opaque={'keep': True})
                 for i, g in enumerate((GROUPS[0], GROUPS[0], GROUPS[1]), 1)]
    elements += [dict(type='user', user_id=MEMBER, fixture_occurrence=i,
                      tab_id='opaque-member-tab', style={style: True},
                      opaque={'keep': [i, 'original']})
                 for i, style in ((4, 'bold'), (5, 'italic'))]
    elements += file_tokens()
    return [dict(type='rich_text', elements=[dict(type='rich_text_section', elements=elements)])]


def seed():
    user = dict(id=SELF, name='fixture-self', team_id=TEAM, profile={})
    return dict(
        boot=dict(ok=True, team=dict(id=TEAM, name='Group Retry'), self=user, users=[user],
                  channels=[dict(id='CINIT0001', name='warm-harbor', is_channel=True, is_member=True)], prefs={}),
        init=dict(ok=True, self=user, team=dict(id=TEAM, name='Group Retry'), workspaces=[dict(id=TEAM)],
                  onboarding=dict(user_data=dict(onboarding_complete=True)), user_date_created=123))


def member_raw():
    return dict(id=MEMBER, team_id=TEAM, name='username-must-not-win', real_name='Real fallback',
                profile=dict(display_name=MEMBER_NAME, real_name='Profile fallback'), updated=101)


def member_response():
    return dict(ok=True, results=[member_raw()], can_interact={MEMBER: True})


def member_source():
    value = member_raw()
    value['profile'].update(first_name='', last_name='', display_name_normalized='  Cafe Harbor  ',
                            real_name_normalized='Profile fallback')
    value.update(_name_lc='username-must-not-win', _first_name_lc='', _last_name_lc='',
                 _real_name_lc='profile fallback', _display_name_lc=MEMBER_NAME.lower(),
                 _real_name_normalized_lc='profile fallback',
                 _display_name_normalized_lc='  cafe harbor  ', _email_normalized_lc='', is_stranger=False)
    return value


def group_raw(group):
    return dict(id=group, team_id=TEAM, handle=LABELS[0] if group == GROUPS[0] else 'Review crew',
                name='Fixture crew', date_delete=0, is_visible=True, fixture_metadata={'keep': True})


def geometry(value, stage, viewport, require, rectangle):
    editor, loading = stage in EDITORS, stage in LOADING_STAGES
    waiting = loading or stage == 'member-offline'
    require(isinstance(value, dict) and value.get('ok') is True and value.get('noOverflow') is True
            and value.get('viewport') == viewport and value.get('dialog') is True,
            'Missing original mixed dialog geometry')
    rectangle(value.get('dialogRect'), viewport)
    controls, notices, tokens = (value.get(k) for k in ('controls', 'notices', 'tokens'))
    require(all(isinstance(v, list) for v in (controls, notices, tokens)), 'Missing mixed DOM inventory')
    for item in controls + notices + tokens:
        require(item.get('visible') is True, 'Hidden mixed control, feedback or token')
        rectangle(item.get('rect'), viewport)
    expected = ['Back', 'Save section'] + (
        ['Use description', 'Cancel description edit'] if editor else
        ['Retry description', 'Cancel description edit'] if waiting else ['Edit description'])
    require(sorted(c.get('text') for c in controls) == sorted(expected), 'Wrong mixed control inventory')
    require(all(c.get('disabled') is (
        c['text'] == 'Save section' or (c['text'] == 'Retry description' and loading)) for c in controls),
        'Wrong mixed control disabled state')
    notice = LOADING if loading else UNAVAILABLE
    feedback = any(n.get('text') == notice for n in notices)
    require(value.get('feedbackRequired') is waiting and value.get('feedbackVisible') is feedback
            and (not waiting or feedback), 'Mixed recovery lacks exact readable guidance')
    require(value.get('richEditable') is (True if editor else None), 'Mixed editor mounted before all sources')
    if editor:
        require([t.get('text') for t in tokens] == LABELS
                and all(t.get('editable') is False for t in tokens), 'Wrong exact mixed labels')
        for token in tokens:
            require(isinstance(token.get('fragments'), list) and token['fragments'], 'Missing token fragments')
            for fragment in token['fragments']:
                rectangle(fragment, viewport)
        require([(t.get('fontWeight'), t.get('fontStyle')) for t in tokens[3:]]
                == [('700', 'normal'), ('400', 'italic')] * 2, 'Mixed member/file painted styles changed')
        capture = value.get('captured')
        require(isinstance(capture, dict), 'Missing actual native mixed capture')
        snapshot = capture.get('snapshot', {})
        require(snapshot.get('blocks') == original_blocks(EDITED if stage in ('edited', 'reopened') else DRAFT)
                and snapshot.get('composing') is False and type(snapshot.get('revision')) is int
                and isinstance(snapshot.get('generation'), str) and snapshot['generation'],
                'Native mixed raw draft or generation changed')
        require(capture.get('tokens') == [dict(value=t, label=l) for t, l in
                zip(original_blocks()[0]['elements'][0]['elements'][1:], LABELS)],
                'Native duplicate token/opaque/style witnesses changed')
    else:
        require(tokens == [] and value.get('captured') is None, 'Retired or incomplete editor retained tokens')
    if stage == 'saved':
        require(any(n.get('text') == 'Profile fields saved.' for n in notices), 'Missing actual save/readback notice')
    require('globalToast' in value, 'Missing independent mixed toast measurement')
    toast = value['globalToast']
    if toast is not None:
        require(isinstance(toast, dict) and type(toast.get('visible')) is bool
                and type(toast.get('positioned')) is bool, 'Invalid mixed toast')
        if toast['visible']:
            require(toast['positioned'] and toast.get('visibility') == 'visible', 'Unsafe mixed toast')
            rectangle(toast.get('rect'), viewport)
            require(toast.get('dismiss', {}).get('visible') is True, 'Mixed toast cannot be dismissed')
            rectangle(toast['dismiss'].get('rect'), viewport)


def member_requests(cycles, held=False, offline=False):
    rows = []
    for cycle in range(cycles):
        for kind in ('discovery', 'member'):
            pending = held and cycle == cycles - 1 and kind == 'member'
            failed = offline and cycle == 0 and kind == 'member' and not pending
            body = (dict(updated_ids={MEMBER: 0}, check_interaction=True, include_profile_only_users=True)
                    if kind == 'member' else
                    dict(include_external_workspaces='true', _x_reason='client-needs-valid-flannel-url'))
            row = dict(serial=len(rows) + 1, kind=kind, method='users/info' if kind == 'member'
                       else 'api.getFlannelHttpUrl', body=body, account=ACCOUNT, team=TEAM, epoch=0,
                       released=not pending)
            if not pending:
                raw = member_response() if kind == 'member' else dict(
                    ok=True, url=f'https://edgeapi.slack.com/cache/{TEAM}', ttl_seconds=300)
                row.update(outcome='offline' if failed else 'response', receiverRetired=False,
                           delivered=True, response=dict(error='offline') if failed else
                           dict(raw=raw, disconnected=None))
            rows.append(row)
    return rows


def group_requests(started=False, held=False):
    if not started:
        return []
    rows = []
    for kind in ('discovery', 'directory'):
        pending = held and kind == 'directory'
        row = dict(serial=len(rows) + 1, kind=kind, method='usergroups/info' if kind == 'directory'
                   else 'api.getFlannelHttpUrl', account=ACCOUNT, team=TEAM, epoch=0,
                   body=dict(ids=list(GROUPS)) if kind == 'directory' else
                   dict(include_external_workspaces='true', _x_reason='client-needs-valid-flannel-url'),
                   released=not pending)
        if not pending:
            row.update(outcome='response', receiverRetired=False, delivered=True)
        rows.append(row)
    return rows


def counts(case, stage):
    """Exact scenario dispatch boundaries, independent of captured counters."""
    offline = case == 'mixed-absent-member-offline'
    if stage == 'initial':
        return 0, 0, False, 0, 0, False
    if stage == 'file-held':
        return 1, 0, True, 0, 0, False
    if stage in ('member-held', 'cancelled'):
        return 1, 1, False, 1, 0, True
    if stage in ('member-offline', 'late-settled'):
        return 1, 1, False, 1, 1, False
    if stage == 'retry-file-held':
        return 2, 1, True, 1, 1, False
    if stage == 'retry-member-held':
        return 2, 2, False, 2, 1, True
    total = 2 if offline else 1
    return total, total, False, total, total, False


def profile(value, stage, require):
    saved = stage in ('saved', 'reopened')
    expected = original_blocks(EDITED if saved else DRAFT)
    require(isinstance(value, dict) and value.get('enabled') is True and value.get('blocks') == expected
            and value.get('dirty') is (not saved) and value.get('reads') == int(saved),
            'Original mixed Profile draft or readback changed')
    require(value.get('choiceValue') == 'Morning' and value.get('countryValue') == 'US',
            'Mixed sibling fields changed')
    attempts = value.get('attempts')
    require(isinstance(attempts, list) and len(attempts) == int(saved), 'Mixed Save repeated or missing')
    if saved:
        require(isinstance(attempts[0], list), 'Missing mixed original section wire')
        long = [x for x in attempts[0] if x.get('element_id') == 'long']
        require(len(long) == 1 and isinstance(long[0].get('long_text', {}).get('blocks'), str),
                'Missing mixed long-text request')
        try:
            wire = json.loads(long[0]['long_text']['blocks'])
        except (ValueError, TypeError) as error:
            raise RuntimeError('Invalid mixed saved blocks') from error
        require(wire == expected, 'Mixed Save dropped original token/style/opaque bytes or native edit')


def canonical(mixed, must_be_known, may_be_known, require):
    member, observation = mixed['member'], mixed['observation']
    present = member.get('sourcePresent')
    require(type(present) is bool and member.get('sourceObserved') is True,
            'Missing actual canonical source observation')
    if must_be_known:
        require(present, 'Member private adoption missing before group admission')
    elif not may_be_known:
        require(not present, 'Absent member gained premature canonical metadata')
    if not present:
        require(member.get('native') is None and observation is None, 'Absent member fabricated a native placeholder')
        return
    require(isinstance(observation, dict) and set(observation) == {'source', 'native', 'current'}
            and observation['current'] is True and observation['source'] == member_source(),
            'Mixed derived canonical member source/currentness changed')
    native = member.get('native')
    require(isinstance(native, dict) and observation['native'] == native
            and native.get('id') == MEMBER and native.get('team_id') == TEAM
            and native.get('profile', {}).get('display_name') == MEMBER_NAME
            and native.get('name') == 'username-must-not-win'
            and native.get('is_stranger') is False
            and native.get('isUnknown') is not True and native.get('isNonExistent') is not True,
            'Wrong actual private-adoption native member')


def proofs(row, case, require):
    stage, proof = row['stage'], row.get('proof', {})
    wanted = {
        'initial': ('targetTrulyAbsent', 'originalDraft'),
        'file-held': ('duplicateOpen', 'fileFirst'),
        'member-held': ('fileReady', 'coalescedMemberIds', 'noPrematureGroups'),
        'member-offline': ('noPlaceholder', 'noAutomaticLoop'),
        'retry-file-held': ('explicitRetry', 'duplicateRetry', 'fileFirst'),
        'retry-member-held': ('fileReady', 'noPrematureGroups'),
        'cancelled': ('cancelledOriginalConsumer', 'noPrematureGroups'),
        'late-settled': ('sharedOutcomeSettled', 'oldConsumerDidNotMount', 'noPrematureGroups'),
        'group-held': ('memberCanonical', 'fileReady'),
        'success': ('allOriginalReferencesAdmitted', 'originalDraft'),
        'edited': ('actualNativeEdit',),
        'saved': ('actualProfileSaveReadback', 'rawDuplicatesPreserved'),
        'reopened': ('noRedundantRead', 'originalDraft'),
    }
    require(all(proof.get(k) is True for k in wanted[stage]), 'Missing executed mixed stage proof')
    if stage not in TAIL and stage != 'initial':
        require(type(proof.get('quietObservationMs')) is int and proof['quietObservationMs'] == 500,
                'Missing bounded quiet original mixed observation')
    if stage in ('member-held', 'retry-member-held'):
        require(proof.get('fileRelease') == dict(delivered=True, cancelledBeforeReply=False),
                'Original file response was not delivered')
    release = None
    serial = 4 if case.endswith('-offline') and stage == 'group-held' else 2
    if stage in ('member-offline', 'late-settled'):
        release = proof.get('memberRelease')
    if stage == 'group-held':
        require(proof.get('memberResponse') == member_response(), 'Group admission lost raw member response')
        nested = proof.get('member', {})
        if case.endswith('-cancel'):
            require(nested.get('freshConsumerAdopted') is True and nested.get('noRedundantMemberRead') is True,
                    'Cancelled consumer reused instead of fresh private adoption')
        else:
            require(nested.get('actualMemberReadAndAdoption') is True, 'Missing actual member adoption')
            release = nested.get('memberRelease')
    if stage == 'late-settled':
        require(proof.get('memberResponse') == member_response(), 'Late original response mutated')
    if release is not None or stage in ('member-offline', 'late-settled') or (
            stage == 'group-held' and not case.endswith('-cancel')):
        require(release == dict(serial=serial, receiverRetired=False, delivered=True),
                'Shared original member receiver delivery was fabricated or lost')
    if stage == 'edited':
        edit = proof.get('nativeEdit', {})
        before, after = edit.get('before', {}), edit.get('after', {})
        a, b = before.get('snapshot', {}), after.get('snapshot', {})
        require(edit.get('ok') is True and a.get('blocks') == original_blocks()
                and b.get('blocks') == original_blocks(EDITED)
                and before.get('tokens') == after.get('tokens') == row['geometry']['captured']['tokens']
                and a.get('generation') == b.get('generation') == row['geometry']['captured']['snapshot']['generation']
                and type(a.get('revision')) is int and type(b.get('revision')) is int
                and b['revision'] > a['revision'] and a.get('composing') is False and b.get('composing') is False,
                'Actual native edit did not preserve original token witnesses and advance original revision')


def validate_absent(receipt, captures, viewport, require, rectangle, validate_png, png):
    case = receipt['case']
    offline = case.endswith('-offline')
    require(receipt.get('requests') == group_requests(True), 'Wrong final mixed group factory/response ledger')
    require(receipt.get('memberRequests') == member_requests(2 if offline else 1, offline=offline),
            'Wrong final original member request/raw response ledger')
    openings = {}
    for row in receipt['stages']:
        stage, model = row['stage'], row.get('model')
        require(isinstance(model, dict) and model.get('account') == ACCOUNT and model.get('epoch') == 0,
                'Mixed stage escaped original account')
        profile(model.get('profile'), stage, require)
        fcount, fdone, fheld, mcount, mdone, mheld = counts(case, stage)
        mixed = model.get('mixedAbsentMember')
        require(isinstance(mixed, dict) and set(mixed) == {'seed', 'member', 'observation'}
                and mixed['seed'] == seed(), 'Original boot/init absence evidence changed')
        member = mixed['member']
        require(isinstance(member, dict) and member.get('account') == ACCOUNT and member.get('epoch') == 0
                and member.get('profile') == model['profile'], 'Member observation escaped original Profile draft')
        expected_member = member_requests(mcount, mheld, offline)
        require(member.get('requests') == expected_member, 'Mixed member acquisition ordering/receipt changed')
        require(member.get('pending') == (expected_member[-1:] if mheld else [])
                and member.get('driver') == dict(started=mcount, finished=mdone, active=int(mheld)),
                'Member dispatch completion confused with private adoption')
        canonical(mixed, stage in TAIL or stage == 'group-held', stage == 'late-settled', require)
        files = model.get('mixedFile')
        require(isinstance(files, dict) and set(files) == {'source', 'cache', 'opening'}
                and files['source'] == file_source(fcount, fdone, fheld),
                'Mixed files.info dispatch/forced Retry changed')
        require(files['cache'] == (None if stage in ('initial', 'file-held') else file_cache()),
                'Mixed file source/currentness changed')
        opening = files['opening']
        require(isinstance(opening, dict) and set(opening) == {'visit', 'serial', 'key', 'reading', 'mounted'}
                and all(type(opening[k]) is int and opening[k] >= 0 for k in ('visit', 'serial', 'key'))
                and opening['reading'] is (stage in LOADING_STAGES) and opening['mounted'] is (stage in EDITORS),
                'Wrong original mixed editor opening state')
        require((model['profile'].get('visit'), model['profile'].get('serial'))
                == (opening['visit'], opening['serial']),
                'Mixed opening and actual Profile section identity disagree')
        openings[stage] = opening
        begun, held = stage in TAIL or stage == 'group-held', stage == 'group-held'
        expected_group = group_requests(begun, held)
        require(model.get('requests') == expected_group and model.get('pending') == (expected_group[-1:] if held else [])
                and model.get('driver') == dict(started=int(begun), finished=int(begun and not held), active=int(held)),
                'Group acquisition preceded original member adoption or repeated')
        groups = model.get('groups')
        require(isinstance(groups, dict) and set(groups) == set(GROUPS), 'Mixed group inventory changed')
        for group in GROUPS:
            item = groups[group]
            if stage in TAIL:
                require(item.get('source') == group_raw(group)
                        and item.get('label') == (LABELS[0] if group == GROUPS[0] else LABELS[2])
                        and item.get('native', {}).get('label_source') == group_raw(group),
                        'Mixed group raw metadata or native source changed')
            else:
                require(item == dict(native=None, source=None, label=None), 'Group gained premature source authority')
        proofs(row, case, require)
        geometry(row.get('geometry'), stage, viewport, require, rectangle)
        path = Path(receipt['captures'][stage])
        require(path.resolve() == (captures / f"{receipt['fixture']}-{stage}.png").resolve(),
                'Mixed capture escaped expected original path')
        dimensions = row['geometry'].get('pngDimensions')
        require(isinstance(dimensions, list) and len(dimensions) == 2
                and all(type(v) is int and v > 0 for v in dimensions), 'Missing original PNG dimensions')
        if png:
            validate_png(path, dimensions)
    first = openings['file-held']
    require(first['key'] > openings['initial']['key'] and openings['member-held'] == first,
            'File/member handoff replaced original opening')
    by_stage = {r['stage']: r for r in receipt['stages']}
    require(by_stage['file-held']['proof'].get('opening') == first
            and by_stage['member-held']['proof'].get('originalOpening') == first,
            'Original opening witness was substituted')
    for stage, value in openings.items():
        require(value['visit'] == first['visit'], 'Mixed recovery replaced original Profile visit')
        if stage not in ('saved', 'reopened'):
            require(value['serial'] == first['serial'],
                    'Mixed recovery replaced original Profile section before Save')
    # network::finish_check advances d.serial only after the original Save's
    # matching canonical readback. The rich opening key survives Use/Save;
    # a subsequent Edit calls opening::prepare and mints a new opening key.
    saved, reopened = openings['saved'], openings['reopened']
    require(saved['serial'] > first['serial'] and reopened['serial'] == saved['serial'],
            'Mixed confirmed Save/reopen did not retain the new canonical section serial')
    require(saved['key'] == openings['edited']['key'] and reopened['key'] > saved['key'],
            'Mixed Use/Save changed admitted key or reopening failed to create a fresh editor')
    if offline:
        require(openings['retry-file-held']['key'] > openings['member-offline']['key']
                and openings['retry-member-held'] == openings['retry-file-held'],
                'Explicit Retry failed original file/member ordering or reused retired opening')
    if case.endswith('-cancel'):
        require(openings['cancelled'] == openings['late-settled']
                and openings['cancelled']['key'] > first['key']
                and openings['group-held']['key'] > openings['late-settled']['key'],
                'Late member completion reopened cancelled consumer or fresh opening missing')
    prior = openings['group-held']
    require(openings['success']['key'] > prior['key'] and openings['edited'] == openings['success'],
            'Successful private adoption did not re-track before editing or editor identity changed')
