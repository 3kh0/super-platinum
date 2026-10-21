"""Synthetic receipt admission tests, not native or visual acceptance."""
import copy
import json
from pathlib import Path
import runpy
import unittest

import profile_group_absent_member as A
from profile_group_mixed_file import file_cache, file_source
import test_profile_group_recovery_native as OLD

H = runpy.run_path(str(Path(__file__).with_name('check-profile-group-recovery-native.py')))
CAPTURES = Path('tmp/pure-mixed-absent-review').resolve()


def rect():
    return dict(left=10, top=10, right=110, bottom=30, width=100, height=20)


def geometry(stage, viewport):
    editor = stage in A.EDITORS
    loading = stage in A.LOADING_STAGES
    waiting = loading or stage == 'member-offline'
    controls = ['Back', 'Save section'] + (
        ['Use description', 'Cancel description edit'] if editor else
        ['Retry description', 'Cancel description edit'] if waiting else ['Edit description'])
    notices = [dict(text=A.LOADING if loading else A.UNAVAILABLE, visible=True, rect=rect())] if waiting else []
    if stage == 'saved':
        notices.append(dict(text='Profile fields saved.', visible=True, rect=rect()))
    tokens = [dict(text=label, visible=True, editable=False, rect=rect(), fragments=[rect()])
              for label in A.LABELS] if editor else []
    for i in range(3, len(tokens)):
        tokens[i].update(fontWeight='700' if i in (3, 5) else '400',
                         fontStyle='normal' if i in (3, 5) else 'italic')
    capture = dict(
        snapshot=dict(blocks=A.original_blocks(A.EDITED if stage in ('edited', 'reopened') else A.DRAFT),
                      composing=False, revision=2 if stage == 'edited' else 1, generation='original-native'),
        tokens=[dict(value=t, label=l) for t, l in zip(
            A.original_blocks()[0]['elements'][0]['elements'][1:], A.LABELS)]) if editor else None
    return dict(ok=True, noOverflow=True, viewport=viewport, dialog=True, dialogRect=rect(),
                controls=[dict(text=c, visible=True, rect=rect(), disabled=c == 'Save section' or
                               (c == 'Retry description' and loading)) for c in controls],
                notices=notices, tokens=tokens, feedbackRequired=waiting, feedbackVisible=waiting,
                richEditable=True if editor else None, captured=capture, globalToast=None,
                pngDimensions=[1942, 1256])


def receipt(case='mixed-absent-member', fixture='profile-group-recovery'):
    viewport = H['expected_viewport'](fixture, 'win32')
    offline, cancel = case.endswith('-offline'), case.endswith('-cancel')
    result = dict(ok=True, fixture=fixture, case=case, viewport=viewport, captureChecksPassed=True,
                  requests=A.group_requests(True), memberRequests=A.member_requests(2 if offline else 1, offline=offline),
                  stages=[], captures={})
    for name in A.CASES[case]:
        fc, fd, fh, mc, md, mh = A.counts(case, name)
        final = name in A.TAIL
        known = final or name == 'group-held'
        saved = name in ('saved', 'reopened')
        raw = A.original_blocks(A.EDITED if saved else A.DRAFT)
        profile = dict(enabled=True, blocks=raw, dirty=not saved, reads=int(saved), visit=1,
                       serial=7 if saved else 2,
                       choiceValue='Morning', countryValue='US',
                       attempts=[[dict(element_id='long', long_text=dict(blocks=json.dumps(raw)))]] if saved else [])
        member_reads = A.member_requests(mc, mh, offline)
        native = dict(A.member_raw(), is_stranger=False) if known else None
        member = dict(account=A.ACCOUNT, epoch=0, profile=copy.deepcopy(profile), sourceObserved=True,
                      sourcePresent=known, native=native, requests=member_reads,
                      pending=member_reads[-1:] if mh else [], driver=dict(started=mc, finished=md, active=int(mh)))
        mixed = dict(seed=A.seed(), member=member, observation=dict(
            source=A.member_source(), native=copy.deepcopy(native), current=True) if known else None)
        key = 0 if name == 'initial' else 1
        if cancel and name in ('cancelled', 'late-settled'):
            key = 2
        if cancel and known:
            key = 3
        if offline and name in ('retry-file-held', 'retry-member-held', 'group-held'):
            key = 2
        if final:
            key = 4 if cancel else 3 if offline else 2
        if name == 'reopened':
            key += 1
        opening = dict(visit=1, serial=7 if saved else 2, key=key,
                       reading=name in A.LOADING_STAGES, mounted=name in A.EDITORS)
        groups = {g: dict(native=dict(id=g, label_source=A.group_raw(g)), source=A.group_raw(g),
                          label=A.LABELS[0] if g == A.GROUPS[0] else A.LABELS[2]) if final else
                  dict(native=None, source=None, label=None) for g in A.GROUPS}
        begun, held = known, name == 'group-held'
        group_reads = A.group_requests(begun, held)
        model = dict(account=A.ACCOUNT, epoch=0, profile=profile, mixedAbsentMember=mixed,
                     mixedFile=dict(source=file_source(fc, fd, fh),
                                    cache=None if name in ('initial', 'file-held') else file_cache(), opening=opening),
                     groups=groups, requests=group_reads, pending=group_reads[-1:] if held else [],
                     driver=dict(started=int(begun), finished=int(begun and not held), active=int(held)))
        proof = {
            'initial': dict(targetTrulyAbsent=True, originalDraft=True),
            'file-held': dict(duplicateOpen=True, fileFirst=True, opening=copy.deepcopy(opening)),
            'member-held': dict(fileRelease=dict(delivered=True, cancelledBeforeReply=False), fileReady=True,
                                originalOpening=copy.deepcopy(opening), coalescedMemberIds=True, noPrematureGroups=True),
            'member-offline': dict(memberRelease=dict(serial=2, receiverRetired=False, delivered=True),
                                   noPlaceholder=True, noAutomaticLoop=True),
            'retry-file-held': dict(explicitRetry=True, duplicateRetry=True, fileFirst=True),
            'retry-member-held': dict(fileRelease=dict(delivered=True, cancelledBeforeReply=False),
                                      fileReady=True, noPrematureGroups=True),
            'cancelled': dict(cancelledOriginalConsumer=True, noPrematureGroups=True),
            'late-settled': dict(memberRelease=dict(serial=2, receiverRetired=False, delivered=True),
                                 memberResponse=A.member_response(), sharedOutcomeSettled=True,
                                 oldConsumerDidNotMount=True, noPrematureGroups=True),
            'group-held': dict(memberCanonical=True, fileReady=True, memberResponse=A.member_response(),
                               member=dict(freshConsumerAdopted=True, noRedundantMemberRead=True) if cancel else
                               dict(actualMemberReadAndAdoption=True, memberRelease=dict(
                                   serial=4 if offline else 2, receiverRetired=False, delivered=True))),
            'success': dict(allOriginalReferencesAdmitted=True, originalDraft=True),
            'edited': dict(actualNativeEdit=True),
            'saved': dict(actualProfileSaveReadback=True, rawDuplicatesPreserved=True),
            'reopened': dict(noRedundantRead=True, originalDraft=True),
        }[name]
        if name != 'initial' and name not in A.TAIL:
            proof['quietObservationMs'] = 500
        geo = geometry(name, viewport)
        if name == 'edited':
            after = copy.deepcopy(geo['captured'])
            before = copy.deepcopy(after)
            before['snapshot'].update(blocks=A.original_blocks(), revision=1)
            proof['nativeEdit'] = dict(ok=True, before=before, after=after)
        result['stages'].append(dict(stage=name, model=model, proof=proof, geometry=geo))
        result['captures'][name] = str(CAPTURES / f'{fixture}-{name}.png')
    return result


def stage(value, name):
    return next(row for row in value['stages'] if row['stage'] == name)


class Admission(unittest.TestCase):
    def validate(self, value):
        H['validate'](value['fixture'], value, CAPTURES, platform='win32', png=False)

    def reject(self, change, case='mixed-absent-member'):
        value = receipt(case)
        change(value)
        with self.assertRaises((RuntimeError, KeyError, ValueError, TypeError)):
            self.validate(value)

    def test_all_three_cases_both_widths(self):
        for case in A.CASES:
            for fixture in H['FIXTURES']:
                with self.subTest(case=case, fixture=fixture):
                    self.validate(receipt(case, fixture))

    def test_known_member_case_stays_exact_zero_io(self):
        old = OLD.receipt('mixed-file-group')
        H['validate'](old['fixture'], old, OLD.CAPTURES, platform='win32', png=False)
        stage(old, 'group-held')['model']['mixedMember']['absentDriver']['started'] = 1
        with self.assertRaises(RuntimeError):
            H['validate'](old['fixture'], old, OLD.CAPTURES, platform='win32', png=False)

    def test_original_boot_cannot_contain_target(self):
        self.reject(lambda r: stage(r, 'initial')['model']['mixedAbsentMember']['seed']['boot']['users'].append(A.member_raw()))

    def test_initial_source_absence_is_observed_not_missing_observation(self):
        self.reject(lambda r: stage(r, 'initial')['model']['mixedAbsentMember']['member'].update(sourceObserved=False))
        self.reject(lambda r: stage(r, 'initial')['model']['mixedAbsentMember']['member'].update(native=A.member_raw()))

    def test_boot_original_and_native_source_cannot_be_rewritten(self):
        self.reject(lambda r: stage(r, 'success')['model']['mixedAbsentMember']['seed']['boot'].update(users=[]))
        self.reject(lambda r: stage(r, 'success')['model']['mixedAbsentMember']['observation']['source'].pop('is_stranger'))

    def test_raw_response_distinct_from_derived_source(self):
        self.reject(lambda r: r['memberRequests'][-1]['response']['raw']['results'][0].update(is_stranger=False))
        self.reject(lambda r: stage(r, 'group-held')['model']['mixedAbsentMember']['observation'].update(source=A.member_raw()))

    def test_derived_names_deburr_only_normalized_fields(self):
        self.reject(lambda r: stage(r, 'success')['model']['mixedAbsentMember']['observation']['source']['profile'].update(
            display_name_normalized=A.MEMBER_NAME))
        self.reject(lambda r: stage(r, 'success')['model']['mixedAbsentMember']['observation']['source'].update(
            _display_name_lc='  cafe harbor  '))

    def test_file_must_settle_before_member_dispatch(self):
        self.reject(lambda r: stage(r, 'file-held')['model']['mixedAbsentMember']['member'].update(requests=A.member_requests(1, True)))
        self.reject(lambda r: stage(r, 'member-held')['model']['mixedFile']['source'].update(completed=0))

    def test_group_cannot_start_before_actual_private_adoption(self):
        self.reject(lambda r: stage(r, 'member-held')['model'].update(requests=A.group_requests(True, True)))
        self.reject(lambda r: stage(r, 'group-held')['model']['mixedAbsentMember'].update(observation=None))
        self.reject(lambda r: stage(r, 'group-held')['model']['mixedAbsentMember']['observation'].update(current=False))

    def test_exact_member_factory_owner_and_flags(self):
        for key, value in [('updated_ids', {A.MEMBER: 101}), ('check_interaction', False),
                           ('include_profile_only_users', False), ('token', 'not-allowed')]:
            self.reject(lambda r: r['memberRequests'][1]['body'].update({key: value}))
        self.reject(lambda r: r['memberRequests'][1].update(account='foreign'))

    def test_duplicate_member_tokens_coalesce_to_one_request(self):
        self.reject(lambda r: r['memberRequests'].append(copy.deepcopy(r['memberRequests'][-1])))
        self.reject(lambda r: stage(r, 'member-held')['proof'].update(coalescedMemberIds=False))

    def test_member_held_does_not_claim_published_source(self):
        self.reject(lambda r: stage(r, 'member-held')['model']['mixedAbsentMember']['member'].update(sourcePresent=True))
        self.reject(lambda r: stage(r, 'member-held')['model']['mixedAbsentMember']['member']['driver'].update(finished=1))

    def test_cancelled_original_stays_closed_until_fresh_open(self):
        case = 'mixed-absent-member-cancel'
        self.reject(lambda r: stage(r, 'late-settled')['model']['mixedFile']['opening'].update(mounted=True), case)
        self.reject(lambda r: stage(r, 'late-settled')['model'].update(requests=A.group_requests(True, True)), case)
        self.reject(lambda r: stage(r, 'late-settled')['proof'].update(oldConsumerDidNotMount=False), case)

    def test_late_shared_delivery_is_not_renderer_adoption(self):
        case = 'mixed-absent-member-cancel'
        value = receipt(case)
        self.assertIsNone(stage(value, 'late-settled')['model']['mixedAbsentMember']['observation'])
        self.validate(value)
        self.reject(lambda r: stage(r, 'late-settled')['proof']['memberRelease'].update(receiverRetired=True, delivered=False), case)
        self.reject(lambda r: stage(r, 'late-settled')['model']['mixedAbsentMember']['member']['driver'].update(finished=0), case)

    def test_allowed_current_shared_publication_still_requires_exact_source(self):
        value = receipt('mixed-absent-member-cancel')
        late, ready = (stage(value, s)['model']['mixedAbsentMember'] for s in ('late-settled', 'group-held'))
        late['observation'] = copy.deepcopy(ready['observation'])
        late['member'].update(sourcePresent=True, native=copy.deepcopy(ready['member']['native']))
        self.validate(value)
        late['observation']['source']['updated'] = 102
        with self.assertRaises(RuntimeError):
            self.validate(value)

    def test_fresh_consumer_reuses_completed_reads(self):
        case = 'mixed-absent-member-cancel'
        self.reject(lambda r: stage(r, 'group-held')['model']['mixedFile']['source'].update(requests=[]), case)
        self.reject(lambda r: stage(r, 'group-held')['proof']['member'].update(freshConsumerAdopted=False), case)
        self.reject(lambda r: stage(r, 'group-held')['model']['mixedFile']['opening'].update(key=2), case)

    def test_offline_does_not_fabricate_placeholder_or_retry(self):
        case = 'mixed-absent-member-offline'
        self.reject(lambda r: stage(r, 'member-offline')['model']['mixedAbsentMember']['member'].update(native=A.member_raw()), case)
        self.reject(lambda r: stage(r, 'member-offline')['model']['mixedAbsentMember']['member'].update(requests=A.member_requests(2)), case)
        self.reject(lambda r: stage(r, 'member-offline')['proof'].update(quietObservationMs=0), case)

    def test_explicit_retry_forces_file_then_actual_new_member_pair(self):
        case = 'mixed-absent-member-offline'
        self.reject(lambda r: stage(r, 'retry-file-held')['model']['mixedFile'].update(source=file_source(1, 1, False)), case)
        self.reject(lambda r: stage(r, 'retry-file-held')['model']['mixedAbsentMember']['member'].update(requests=A.member_requests(2, True)), case)
        self.reject(lambda r: r['memberRequests'][1]['response'].update(raw=A.member_response()), case)

    def test_full_block_occurrence_style_and_opaque_custody(self):
        self.reject(lambda r: stage(r, 'edited')['geometry']['captured']['snapshot']['blocks'][0]['elements'][0]['elements'][4]['opaque'].update(keep=True))
        self.reject(lambda r: stage(r, 'success')['geometry']['captured']['tokens'][4]['value'].pop('style'))
        self.reject(lambda r: stage(r, 'success')['geometry']['tokens'][4].update(fontStyle='normal'))
        self.reject(lambda r: stage(r, 'reopened')['geometry']['tokens'][3].update(text='@Cafe Harbor'))

    def test_native_edit_does_not_mutate_form_before_use(self):
        self.reject(lambda r: stage(r, 'edited')['model']['profile'].update(blocks=A.original_blocks(A.EDITED)))
        self.reject(lambda r: stage(r, 'edited')['proof']['nativeEdit']['after']['snapshot'].update(revision=1))

    def test_once_save_wire_readback_and_canonical_reopen(self):
        self.reject(lambda r: stage(r, 'saved')['model']['profile']['attempts'].append([]))
        self.reject(lambda r: stage(r, 'saved')['model']['profile']['attempts'][0][0]['long_text'].update(
            blocks=json.dumps(A.original_blocks())))
        self.reject(lambda r: stage(r, 'reopened')['model']['profile'].update(reads=2))
        self.reject(lambda r: stage(r, 'reopened')['model']['mixedFile']['source'].update(completed=2))

    def test_guidance_controls_and_clipped_native_tokens(self):
        self.reject(lambda r: stage(r, 'file-held')['geometry']['notices'][0].update(text='Loading mention names.'))
        self.reject(lambda r: stage(r, 'member-held')['geometry']['controls'][-2].update(disabled=False))
        self.reject(lambda r: stage(r, 'success')['geometry']['tokens'][3]['rect'].update(right=5000, width=4990))

    def test_original_opening_and_readiness_paths(self):
        self.reject(lambda r: stage(r, 'member-held')['model']['mixedFile']['opening'].update(key=22))
        self.reject(lambda r: stage(r, 'success')['model']['mixedFile']['opening'].update(visit=2))
        self.reject(lambda r: r['captures'].update(initial='/foreign.png'))
        self.reject(lambda r: r['stages'].pop(1))

    def test_save_readback_advances_serial_only_at_confirmed_boundary(self):
        for name in ('file-held', 'member-held', 'group-held', 'success', 'edited'):
            self.reject(lambda r: stage(r, name)['model']['mixedFile']['opening'].update(serial=7))
        for name in ('saved', 'reopened'):
            self.reject(lambda r: stage(r, name)['model']['mixedFile']['opening'].update(serial=2))
        value = receipt()
        for name in ('saved', 'reopened'):
            row = stage(value, name)['model']
            row['mixedFile']['opening']['serial'] = 2
            row['profile']['serial'] = 2
            row['mixedAbsentMember']['member']['profile']['serial'] = 2
        with self.assertRaisesRegex(RuntimeError, 'new canonical section serial'):
            self.validate(value)

    def test_opening_profile_identity_and_reopened_serial_must_agree(self):
        self.reject(lambda r: stage(r, 'saved')['model']['profile'].update(serial=2))
        value = receipt()
        row = stage(value, 'reopened')['model']
        row['mixedFile']['opening']['serial'] = 8
        row['profile']['serial'] = 8
        row['mixedAbsentMember']['member']['profile']['serial'] = 8
        with self.assertRaisesRegex(RuntimeError, 'new canonical section serial'):
            self.validate(value)

    def test_use_save_keep_key_but_fresh_edit_mints_key(self):
        self.reject(lambda r: stage(r, 'saved')['model']['mixedFile']['opening'].update(key=90))
        self.reject(lambda r: stage(r, 'reopened')['model']['mixedFile']['opening'].update(key=2))
        self.reject(lambda r: stage(r, 'saved')['model']['mixedFile']['opening'].update(visit=2))

    def test_retained_actual_first_run_confirms_canonical_save_serial_transition(self):
        path = Path(__file__).resolve().parents[1] / (
            'tmp/profile-mixed-absent-member-20261020/integration/'
            'mixed-absent-member-normal-a/mixed-absent-member/profile-group-recovery.log')
        if not path.exists():
            self.skipTest('Retained native evidence is local, not a portable test fixture')
        rows = [json.loads(line[len(H['PREFIX']):]) for line in path.read_text(encoding='utf-8').splitlines()
                if line.startswith(H['PREFIX'])]
        self.assertEqual(len(rows), 1)
        value = rows[0]
        self.assertIs(value['ok'], True)  # Rust terminal, not the failed outer wrapper.
        identities = {r['stage']: r['model']['mixedFile']['opening'] for r in value['stages']}
        self.assertEqual([(identities[s]['visit'], identities[s]['serial']) for s in ('edited', 'saved', 'reopened')],
                         [(1, 2), (1, 7), (1, 7)])
        H['validate'](value['fixture'], value, path.parent, platform='win32', png=True)


if __name__ == '__main__':
    unittest.main()
