"""Pure source-generation retirement acceptance regressions."""
import copy
from pathlib import Path
import runpy
import unittest
from workflow_activity_surface_retirement import SUCCESSOR_CHANNEL, SUCCESSOR_THREAD

R = runpy.run_path(str(Path(__file__).with_name('check-workflow-activity-native.py')))
DIRECTORY = Path('/private/activity-captures')


def sample(scenario='execution-thread-retirement', narrow=False, base_sample=None):
    if base_sample is None:
        from test_workflow_activity_surfaces import sample as base_sample
    thread = scenario == 'execution-thread-retirement'
    fixture, base, _ = base_sample('execution-thread' if thread else 'execution-pins', narrow)
    rows = [copy.deepcopy(base['stages'][i]) for i in (0, 1, 3, 1, 3, 4)]
    original_requests = base['stages'][-1]['model']['ledger']['requests']
    requests = [copy.deepcopy(original_requests[i]) for i in (0, 1, 2, 2, 3)]
    for i, request in enumerate(requests, 1):
        request['serial'] = i
    for index, (row, stage) in enumerate(zip(rows, R['STAGES'][scenario])):
        row['stage'] = stage
        count = (0, 2, 2, 3, 3, 5)[index]
        row['model']['ledger'].update(requests=copy.deepcopy(requests[:count]), finished=count - int(stage == 'held'),
                                      cancelled=int(index >= 4), held=3 if stage == 'held' else None)
        if index >= 2:
            row['proof']['source']['documents'] = copy.deepcopy(base['stages'][3]['proof']['source']['documents'])
        if stage == 'held':
            row['model']['entry'].update(active=True, busy=True, visit=7)
            row['model']['metadata'].update(busy=True, dirty=True, title=R['TITLE'], description=R['DESCRIPTION'])
            row['model']['activity'] = dict(active=True, page=None, detail=None, detailRow=None)
            row['geometry'].update(activity=True, detail=False, detailStatus=None)
            row['proof']['exercise'] = dict(heldVisit=7, heldSerial=3)
        if index >= 4:
            source = row['proof']['source']
            if thread:
                source['threadGeneration'] += 1
                source['threadScope'] += '-successor'
            else:
                source['pins']['generation'] += 1
            native = row['proof']['nativeSurface']
            native['actionKey'] += '-successor'
            for editor in native['editors']:
                editor['value'] = SUCCESSOR_CHANNEL if editor['id'] == 'channel-composer' else SUCCESSOR_THREAD
                if editor['id'] == 'thread-composer':
                    editor['visit'] = '2'
                    editor['scope'] += '-successor'
        if stage == 'retired':
            # A retired internal management surface may retain its old read nonce;
            # it is hidden and cannot authorize publication into the fresh source.
            row['model']['entry'].update(active=False, busy=True, visit=7)
            row['model']['metadata']['busy'] = True
            row['model']['activity'] = None
        if stage == 'reopened':
            row['model']['entry']['visit'] = 8
            row['proof']['exercise'] = dict(heldVisit=7, reopenedVisit=8, newVisit=True)
    retired, held = rows[4], rows[3]
    retired['proof']['exercise'] = dict(
        fixtureDrivenSourceRefresh=True, kind='same-root-thread-reopen' if thread else 'same-channel-pins-refresh',
        beforeSource=copy.deepcopy(held['proof']['source']), afterSource=copy.deepcopy(retired['proof']['source']),
        beforeModel=copy.deepcopy(held['model']), afterModel=copy.deepcopy(retired['model']), heldVisit=7,
        release=dict(delivered=False, cancelledBeforeReply=True),
        retainedActivity=[dict(document=8, page=None, detail=None, detailRow=None)], successorWitnessReset=True,
        successorWitness=dict(reset=dict(ok=True, reset=True, previousActionKey=held['proof']['nativeSurface']['actionKey']),
                              nativeSurface=copy.deepcopy(retired['proof']['nativeSurface']),
                              channelDraft=SUCCESSOR_CHANNEL if thread else None,
                              threadDraft=SUCCESSOR_THREAD if thread else None))
    base.update(case=scenario, stages=rows, final=copy.deepcopy(rows[-1]['model']),
                captures={s: str(DIRECTORY / f'{fixture}-{s}.png') for s in R['STAGES'][scenario]})
    return fixture, base, scenario


class RetirementTests(unittest.TestCase):
    def validate(self, f, r, c):
        return R['validate'](f, r, DIRECTORY, c, platform='win32', png=False)

    def reject(self, change, stage='retired', scenario='execution-thread-retirement'):
        f, r, c = sample(scenario)
        change(next(row for row in r['stages'] if row['stage'] == stage))
        with self.assertRaises(RuntimeError):
            self.validate(f, r, c)

    def test_both_retirements_both_widths(self):
        for case in ('execution-thread-retirement', 'execution-pins-retirement'):
            for narrow in (False, True):
                self.validate(*sample(case, narrow))

    def test_successor_unicode_matches_independent_rust_literals(self):
        import re
        text = (Path(__file__).resolve().parents[1] / 'src/desktop/app_home/about/management/entry/fixture/retirement.rs').read_text()
        for key, value in [('SUCCESSOR_CHANNEL', SUCCESSOR_CHANNEL), ('SUCCESSOR_THREAD', SUCCESSOR_THREAD)]:
            raw = re.search(r'const ' + key + r': &str\s*=\s*"([^";]+)";', text).group(1)
            actual = re.sub(r'\\u\{([0-9a-fA-F]+)\}', lambda m: chr(int(m.group(1), 16)), raw)
            self.assertEqual(value, actual)
            self.assertEqual([ord(c) for c in value[-2:]], [0x4e2d, 0x1f642])

    def test_held_request_exact(self):
        for key, value in [('heldSerial', 2), ('heldVisit', 6)]:
            self.reject(lambda row: row['proof']['exercise'].update({key: value}), stage='held')

    def test_held_page_and_detail_remain_unobserved(self):
        for key in ('page', 'detail', 'detailRow'):
            self.reject(lambda row: row['model']['activity'].update({key: {}}), stage='held')

    def test_original_observations_exact(self):
        for key in ('beforeSource', 'afterSource', 'beforeModel', 'afterModel'):
            self.reject(lambda row: row['proof']['exercise'].update({key: {}}))

    def test_real_generation_operation_required(self):
        self.reject(lambda row: row['proof']['exercise'].update(fixtureDrivenSourceRefresh=False))
        self.reject(lambda row: row['proof']['exercise'].update(kind='remove-item'))

    def test_one_generation_exact_even_with_matching_claim(self):
        for delta in (0, 2):
            def change(row):
                ex = row['proof']['exercise']
                row['proof']['source']['threadGeneration'] = ex['beforeSource']['threadGeneration'] + delta
                ex['afterSource'] = copy.deepcopy(row['proof']['source'])
            self.reject(change)

    def test_pins_exact_generation_and_unchanged_item(self):
        def change(row):
            row['proof']['source']['pins']['generation'] += 1
            row['proof']['exercise']['afterSource'] = copy.deepcopy(row['proof']['source'])
        self.reject(change, scenario='execution-pins-retirement')
        self.reject(lambda row: row['proof']['source']['pins']['items'].clear(), scenario='execution-pins-retirement')

    def test_raw_bag_cannot_be_reseeded_under_new_generation(self):
        self.reject(lambda row: row['proof']['source']['threadSource'][1].update(text='replaced'))

    def test_unrelated_owner_or_channel_generation_cannot_change(self):
        for key, value in [('epoch', 1), ('channelGeneration', 4), ('composerModel', 'published')]:
            def change(row):
                row['proof']['source'][key] = value
                row['proof']['exercise']['afterSource'] = copy.deepcopy(row['proof']['source'])
            self.reject(change)

    def test_late_response_must_not_deliver(self):
        self.reject(lambda row: row['proof']['exercise']['release'].update(delivered=True))
        self.reject(lambda row: row['proof']['exercise']['release'].update(cancelledBeforeReply=False))

    def test_actual_cancelled_ledger_required(self):
        self.reject(lambda row: row['model']['ledger'].update(cancelled=0))

    def test_hidden_document_cannot_receive_old_page_detail_or_row(self):
        for key in ('page', 'detail', 'detailRow'):
            self.reject(lambda row: row['proof']['exercise']['retainedActivity'][0].update({key: {}}))

    def test_hidden_retained_document_identity_required(self):
        self.reject(lambda row: row['proof']['exercise']['retainedActivity'][0].update(document=99))
        self.reject(lambda row: row['proof']['exercise'].update(retainedActivity=[]))

    def test_retired_surface_stays_hidden(self):
        self.reject(lambda row: row['model']['entry'].update(active=True))
        self.reject(lambda row: row['model'].update(activity={}))

    def test_stale_internal_read_can_clear_without_claiming_new_custody(self):
        f, r, c = sample()
        retired = r['stages'][4]
        retired['model']['entry']['busy'] = retired['model']['metadata']['busy'] = False
        retired['proof']['exercise']['afterModel'] = copy.deepcopy(retired['model'])
        self.validate(f, r, c)

    def test_reset_requires_original_key_and_new_key(self):
        self.reject(lambda row: row['proof']['exercise'].update(successorWitnessReset=False))
        self.reject(lambda row: row['proof']['exercise']['successorWitness']['reset'].update(previousActionKey='foreign'))
        self.reject(lambda row: row['proof']['exercise']['successorWitness']['reset'].update(reset=False))

    def test_successor_witness_must_be_observed_not_claimed(self):
        self.reject(lambda row: row['proof']['exercise']['successorWitness']['nativeSurface'].update(sameBody=False))

    def test_distinct_successor_buffers_required(self):
        self.reject(lambda row: row['proof']['nativeSurface']['editors'][1].update(value='old'))
        self.reject(lambda row: row['proof']['exercise']['successorWitness'].update(threadDraft='old'))

    def test_successor_pins_cannot_claim_composer(self):
        self.reject(lambda row: row['proof']['exercise']['successorWitness'].update(channelDraft=SUCCESSOR_CHANNEL),
                    scenario='execution-pins-retirement')

    def test_successor_identity_retained_through_reopen(self):
        self.reject(lambda row: row['proof']['nativeSurface'].update(actionKey='third-key'), stage='reopened')
        self.reject(lambda row: row['proof']['nativeSurface']['editors'][1].update(scope='third'), stage='reopened')

    def test_fresh_reopened_visit_required(self):
        for key, value in [('heldVisit', 6), ('reopenedVisit', 7), ('newVisit', False)]:
            self.reject(lambda row: row['proof']['exercise'].update({key: value}), stage='reopened')

    def test_dirty_document_retained(self):
        self.reject(lambda row: row['proof']['source']['documents'][0].update(title='lost'))
        self.reject(lambda row: row['model']['metadata'].update(description='lost'), stage='reopened')

    def test_reopened_count_and_finished_cancellation_exact(self):
        self.reject(lambda row: row['model']['ledger'].update(finished=4), stage='reopened')
        self.reject(lambda row: row['model']['ledger'].update(cancelled=0), stage='reopened')


if __name__ == '__main__':
    unittest.main()
