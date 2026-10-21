"""Oracle regression tests; synthetic inputs are not native acceptance."""
import copy
from datetime import datetime, timedelta
import unittest
from workflow_activity_shared_frozen import SHARED, GRACE, NAMES, validate_observation
from workflow_activity_channel_spec import SOURCE_KEYS, FLAGS


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def sample(spec):
    start = 1_792_500_000
    plan = dict(is_plan_frozen=True, reconnect_grace_period_start_ts=start)
    rows = []
    source = []
    for id in ('CACTKNOWN01', *spec.ids):
        grace = spec.grace and id != 'CGRNOGT01'
        kind = next(kind for _, channel, _, kind in spec.vectors() if channel == id)
        raw = dict(id=id, name='activity-known' if id == 'CACTKNOWN01' else NAMES[id],
                   is_channel=True, is_member=True, is_private=id in spec.private_ids,
                   is_archived=id in spec.archived_ids)
        if spec.grace:
            raw.update(is_frozen=True, unlinked=start)
            if id in ('CACTKNOWN01', 'CGRARCH01'):
                raw['grace_period_end'] = 1
        elif id in ('CACTKNOWN01', 'CSHPRIV01', 'CSHARCH01'):
            raw.update(is_ext_shared=True, is_shared=True, is_org_shared=True,
                       connected_teams=[{'id': 'TWORKFLOW'}, {'id': 'TPARTNER'}])
        else:
            raw['is_frozen'] = True
            if id == 'CFRZACT01':
                raw['unlinked'] = start
        source.append(copy.deepcopy(raw))
        raw.pop('is_shared', None)
        raw['fromAnotherTeam'] = False
        rows.append(dict(id=id, raw=raw, policy=dict(
            private=id in spec.private_ids, archiveIcon=kind in ('Archive', 'ArchiveFilled'),
            archivedSuffix=id == 'CSHARCH01', inGrace=grace,
            capturedNowMs=(start+3600)*1000,
            graceEndMs=int((datetime.fromtimestamp(start)+timedelta(days=5)).timestamp()*1000) if grace else None)))
    return dict(seed=dict(boot=dict(ok=True, team=dict(id='TWORKFLOW'),
                                  channels=source+[{'id': 'DORIGINAL', 'user': 'UHOME0001'}],
                                  users=[{'id': id, 'profile': {}}
                                         for id in ('UWORKFLOW', 'UHOME0001')]),
                         init=dict(ok=True, team=dict(id='TWORKFLOW')),
                         extras=dict(plan_info=plan), startSeconds=start),
                labels=dict(current=True, plan=plan, rows=rows))


class SharedFrozenTests(unittest.TestCase):
    def test_current_observations(self):
        for spec in (SHARED, GRACE):
            validate_observation(sample(spec), spec, require)

    def test_explicit_non_current_before_detail(self):
        value = sample(GRACE)
        value['labels'] = {'current': False}
        validate_observation(value, GRACE, require)

    def test_absent_observation_before_workflow_document(self):
        value = sample(GRACE)
        value['labels'] = None
        validate_observation(value, GRACE, require)

    def reject(self, change, spec=GRACE):
        value = sample(spec)
        change(value)
        with self.assertRaises(RuntimeError):
            validate_observation(value, spec, require)

    def test_canonical_foreign_is_not_shared(self):
        self.reject(lambda v: v['labels']['rows'][0]['raw'].update(fromAnotherTeam=True))

    def test_no_synthetic_current_plan(self):
        self.reject(lambda v: v['labels'].update(plan=None))

    def test_no_plan_default_without_actual_extras(self):
        self.reject(lambda v: v['seed']['extras'].clear())

    def test_wrong_plan_start(self):
        self.reject(lambda v: v['labels'].update(plan={'reconnect_grace_period_start_ts': 0}))

    def test_missing_row(self):
        self.reject(lambda v: v['labels']['rows'].pop())

    def test_duplicate_row(self):
        self.reject(lambda v: v['labels']['rows'].append(copy.deepcopy(v['labels']['rows'][0])))

    def test_unobserved_clock(self):
        self.reject(lambda v: v['labels']['rows'][0]['policy'].update(capturedNowMs=None))

    def test_stale_clock(self):
        self.reject(lambda v: v['labels']['rows'][0]['policy'].update(capturedNowMs=0))

    def test_nonlocal_deadline(self):
        self.reject(lambda v: v['labels']['rows'][0]['policy'].update(graceEndMs=0))

    def test_grace_suppresses_archive_suffix(self):
        self.reject(lambda v: v['labels']['rows'][2]['policy'].update(archivedSuffix=True))

    def test_frozen_active_never_gets_archived_suffix(self):
        self.reject(lambda v: v['labels']['rows'][3]['policy'].update(archivedSuffix=True), SHARED)

    def test_shared_flags_do_not_create_special_icon(self):
        self.reject(lambda v: v['labels']['rows'][0]['policy'].update(archiveIcon=True), SHARED)

    def test_canonical_projection_drops_wire_only_sharing_flag(self):
        self.reject(lambda v: v['labels']['rows'][0]['raw'].update(is_shared=True), SHARED)

    def test_private_grace_remains_private(self):
        self.reject(lambda v: v['labels']['rows'][1]['policy'].update(private=False))

    def test_public_gate_is_required(self):
        self.reject(lambda v: v['labels']['rows'][4]['policy'].update(inGrace=True))

    def test_raw_source_cannot_rewrite_grace_gate(self):
        self.reject(lambda v: v['seed']['boot']['channels'][0].update(grace_period_end=0))

    def test_canonical_plan_input_cannot_drift(self):
        self.reject(lambda v: v['labels']['rows'][0]['raw'].update(unlinked=0))

    def test_no_deadline_for_structural_false(self):
        self.reject(lambda v: v['labels']['rows'][4]['policy'].update(graceEndMs=1))

    def test_occurrences_keep_duplicates(self):
        for spec in (SHARED, GRACE):
            vectors = spec.vectors()
            self.assertEqual(len(vectors), 7)
            self.assertEqual(vectors[0][1], vectors[1][1])
            self.assertEqual(len({v[0] for v in vectors}), 7)
            self.assertEqual(len(spec.inert()), 7)

    def test_frozen_active_composer_is_independent(self):
        self.assertIn('frozen-active', SHARED.destinations)
        self.assertNotIn('frozen-active', SHARED.archived_destinations)
        self.assertIn('archived-grace', GRACE.archived_destinations)
        self.assertEqual(next(v[3] for v in GRACE.vectors() if v[1] == 'CGRARCH01'), 'Tag')

    def test_sidebar_only_archived_hidden(self):
        self.assertIn('CFRZACT01', SHARED.sidebar_ids('DORIGINAL', 'detail', False))
        self.assertNotIn('CSHARCH01', SHARED.sidebar_ids('DORIGINAL', 'detail', False))
        self.assertIn('CSHARCH01', SHARED.sidebar_ids('DORIGINAL', 'shared-archived', False))


class FullReceiptTests(unittest.TestCase):
    def reject(self, spec, stage, change):
        from test_workflow_activity_native import R, DIRECTORY, channel_sample
        fixture, receipt = channel_sample(spec=spec)
        change(next(r for r in receipt['stages'] if r['stage'] == stage))
        with self.assertRaises(RuntimeError):
            R['validate'](fixture, receipt, DIRECTORY, spec.case, platform='win32', png=False)

    def test_missing_current_observation(self):
        self.reject(GRACE, 'detail', lambda r: r['proof']['channel']['labels'].update(current=False))

    def test_null_detail_observation_is_rejected(self):
        self.reject(GRACE, 'detail', lambda r: r['proof']['channel'].update(labels=None))

    def test_grace_icon_is_not_archive(self):
        self.reject(GRACE, 'detail', lambda r: r['proof']['channelLabels']['rows'][4].update(iconKind='Archive'))

    def test_frozen_icon_does_not_change_composer(self):
        self.reject(SHARED, 'frozen-active', lambda r: r['proof']['conversationOwner'].update(composer=None))

    def test_grace_does_not_unarchive_composer(self):
        self.reject(GRACE, 'archived-grace', lambda r: r['geometry'].update(noComposer=False))

    def test_additional_click_uses_original_occurrence(self):
        self.reject(SHARED, 'frozen-active', lambda r: r['proof']['exercise'].update(button='workflow-activity-channel-step-1-1'))

    def test_return_keeps_all_seven_occurrences(self):
        self.reject(GRACE, 'final-return', lambda r: r['proof']['exercise']['reentryLabels']['rows'].pop())

    def test_source_private_flag_is_not_derived_from_icon(self):
        self.reject(SHARED, 'detail', lambda r: next(v for v in r['proof']['channel']['sourceRows'] if v['id'] == 'CFRZACT01').update(isPrivate=True))

    def test_no_plan_fabrication_in_failure_state(self):
        self.reject(GRACE, 'offline', lambda r: r['proof']['channel']['labels'].update(plan=None))


if __name__ == '__main__':
    unittest.main()
