"""Pure replacement/removal validators; no app launches or Slack IO."""
import copy
import json
from pathlib import Path
import re
import runpy
import unittest
from workflow_activity_source_transition import ROOT, TS, SUCCESSOR_ROOT, SUCCESSOR_TS, SUCCESSOR_OPEN, REMOVAL_TS
from workflow_activity_surface_retirement import SUCCESSOR_THREAD
from test_workflow_activity_surface_retirement import sample as prior_sample

R = runpy.run_path(str(Path(__file__).with_name('check-workflow-activity-native.py')))
DIRECTORY = Path('/private/activity-captures')


def sample(scenario='execution-thread-replacement', narrow=False):
    thread = scenario == 'execution-thread-replacement'
    fixture, receipt, _ = prior_sample('execution-thread-retirement' if thread else 'execution-pins-retirement', narrow)
    original = receipt['stages'][0]['proof']['source']
    message = copy.deepcopy(original['threadSource'][1] if thread else original['pins']['items'][0]['message'])
    message.update(ts=SUCCESSOR_TS, thread_ts=SUCCESSOR_ROOT,
                   text='A different source retains this workflow draft.', blocks=[dict(type='actions', elements=[dict(
        type='button', action_id='workflow_error_view_logs', accessibility_label=SUCCESSOR_OPEN,
        text=dict(type='plain_text', text=SUCCESSOR_OPEN),
        value=json.dumps(dict(type='wfb', wf_id='WF_REVIEW', execution_id='E_REVIEW')))])])
    new_bag = [dict(ts=SUCCESSOR_ROOT, blocks=[]), message]
    new_pin = dict(key='message:' + SUCCESSOR_TS, kind='message', channel='C2', message=message)
    for i, row in enumerate(receipt['stages']):
        source = row['proof']['source']
        source['documentRevisions'] = [dict(id=d['id'], revision=3 if i >= 2 else 1) for d in source['documents']]
        source['originalPinVersion'] = REMOVAL_TS if not thread and i >= 4 else None
        source['successorThreadSource'] = copy.deepcopy(new_bag) if thread else None
        source['selectedThreadSource'] = copy.deepcopy(new_bag if i >= 4 else source['threadSource']) if thread else None
        if thread and i >= 4:
            source.update(threadRoot=SUCCESSOR_ROOT, threadComposerModel=SUCCESSOR_THREAD + '.')
        if not thread:
            source['pins']['items'] = ([source['pins']['items'][0]] if i < 4 else []) + [copy.deepcopy(new_pin)]
    held, retired = receipt['stages'][3:5]
    ex = retired['proof']['exercise']
    ex.pop('fixtureDrivenSourceRefresh')
    ex.update(fixtureDrivenSourceReplacement=True, kind='thread-root-replacement' if thread else 'originating-pin-removal',
              beforeSource=copy.deepcopy(held['proof']['source']), afterSource=copy.deepcopy(retired['proof']['source']),
              beforeModel=copy.deepcopy(held['model']), afterModel=copy.deepcopy(retired['model']),
              mutation=(dict(kind='begin-thread', previousRoot=ROOT, nextRoot=SUCCESSOR_ROOT) if thread else
                        dict(kind='pin-removed-event', channel='C2', messageTs=TS, eventTs=REMOVAL_TS, applied=True)),
              originalActionAbsent=dict(ok=True, originalActionKey=held['proof']['nativeSurface']['actionKey'],
                                        originalActionCount=0, originalActionConnected=True))
    before_seed = copy.deepcopy(retired['proof']['source'])
    if thread:
        before_seed['threadComposerModel'] = ''
    ex['staleCallback'] = dict(sourceCurrent=False, presentationEnabled=False, unchanged=True,
                              requestsBefore=3, requestsAfter=3, beforeModel=copy.deepcopy(retired['model']),
                              afterModel=copy.deepcopy(retired['model']), beforeSource=before_seed,
                              afterSource=copy.deepcopy(before_seed))
    receipt.update(case=scenario, final=copy.deepcopy(receipt['stages'][-1]['model']))
    return fixture, receipt, scenario


class ReplacementTests(unittest.TestCase):
    def validate(self, f, r, c):
        return R['validate'](f, r, DIRECTORY, c, platform='win32', png=False)

    def reject(self, change, stage='retired', case='execution-thread-replacement'):
        f, r, c = sample(case)
        change(next(row for row in r['stages'] if row['stage'] == stage))
        with self.assertRaises(RuntimeError):
            self.validate(f, r, c)

    def test_both_cases_both_widths(self):
        for case in R['REPLACEMENT_STAGES']:
            for narrow in (False, True):
                with self.subTest(case=case, narrow=narrow):
                    self.validate(*sample(case, narrow))

    def test_source_constants_match_independent_rust(self):
        rust = (Path(__file__).resolve().parents[1] / 'src/desktop/app_home/about/management/entry/fixture.rs').read_text()
        for name, value in [('SUCCESSOR_ROOT', SUCCESSOR_ROOT), ('SUCCESSOR_TS', SUCCESSOR_TS), ('SUCCESSOR_OPEN', SUCCESSOR_OPEN)]:
            actual = re.search(r'const ' + name + r': &str = "([^";]+)";', rust).group(1)
            self.assertEqual(value, actual)
        self.assertNotEqual(ROOT, SUCCESSOR_ROOT)
        self.assertNotEqual(TS, SUCCESSOR_TS)

    def test_original_observation_receipts_exact(self):
        for key in ('beforeSource', 'afterSource', 'beforeModel', 'afterModel'):
            self.reject(lambda r: r['proof']['exercise'].update({key: {}}))

    def test_new_source_must_exist_before_replacement(self):
        self.reject(lambda r: r['proof']['source'].update(successorThreadSource=None), stage='source')
        self.reject(lambda r: r['proof']['source']['pins']['items'].pop(), stage='source', case='execution-pin-removal')

    def test_successor_exact_action_label_and_payload(self):
        for field, value in [('accessibility_label', 'View workflow execution'), ('value', '{}'), ('type', 'link')]:
            self.reject(lambda r: r['proof']['source']['successorThreadSource'][1]['blocks'][0]['elements'][0].update({field: value}), stage='source')

    def test_selected_thread_switch_required(self):
        self.reject(lambda r: r['proof']['source'].update(threadRoot=ROOT))
        self.reject(lambda r: r['proof']['source'].update(selectedThreadSource=r['proof']['source']['threadSource']))

    def test_original_thread_bag_retained_byte_fields(self):
        def change(r):
            r['proof']['source']['threadSource'][1]['opaque'] = 'reseeded'
            r['proof']['exercise']['afterSource'] = copy.deepcopy(r['proof']['source'])
        self.reject(change)

    def test_existing_successor_bag_not_reseeded(self):
        def change(r):
            r['proof']['source']['successorThreadSource'][1]['opaque'] = 'new'
            r['proof']['source']['selectedThreadSource'] = copy.deepcopy(r['proof']['source']['successorThreadSource'])
            r['proof']['exercise']['afterSource'] = copy.deepcopy(r['proof']['source'])
        self.reject(change)

    def test_only_origin_pin_removed(self):
        self.reject(lambda r: r['proof']['source']['pins']['items'].clear(), case='execution-pin-removal')
        def restore(r):
            r['proof']['source']['pins']['items'] = copy.deepcopy(r['proof']['exercise']['beforeSource']['pins']['items'])
        self.reject(restore, case='execution-pin-removal')

    def test_survivor_pin_raw_fields_unchanged(self):
        def change(r):
            r['proof']['source']['pins']['items'][0]['message']['text'] = 'changed'
            r['proof']['exercise']['afterSource'] = copy.deepcopy(r['proof']['source'])
        self.reject(change, case='execution-pin-removal')

    def test_generation_exact_for_both_sources(self):
        for case in R['REPLACEMENT_STAGES']:
            for delta in (0, 2):
                def change(r):
                    source, ex = r['proof']['source'], r['proof']['exercise']
                    if case == 'execution-pin-removal':
                        source['pins']['generation'] = ex['beforeSource']['pins']['generation'] + delta
                    else:
                        source['threadGeneration'] = ex['beforeSource']['threadGeneration'] + delta
                    ex['afterSource'] = copy.deepcopy(source)
                self.reject(change, case=case)

    def test_pin_event_and_watermark_exact(self):
        for field, value in [('messageTs', SUCCESSOR_TS), ('eventTs', 'late'), ('applied', False)]:
            self.reject(lambda r: r['proof']['exercise']['mutation'].update({field: value}), case='execution-pin-removal')
        self.reject(lambda r: r['proof']['source'].update(originalPinVersion=None), case='execution-pin-removal')

    def test_retired_reply_never_delivered(self):
        for key, value in [('delivered', True), ('cancelledBeforeReply', False)]:
            self.reject(lambda r: r['proof']['exercise']['release'].update({key: value}))

    def test_retired_action_dom_absence_exact(self):
        for key, value in [('ok', False), ('originalActionKey', 'other'), ('originalActionCount', 1), ('originalActionConnected', None)]:
            self.reject(lambda r: r['proof']['exercise']['originalActionAbsent'].update({key: value}))

    def test_detached_or_reused_dom_node_both_valid(self):
        for connected in (True, False):
            f, r, c = sample()
            r['stages'][4]['proof']['exercise']['originalActionAbsent']['originalActionConnected'] = connected
            self.validate(f, r, c)

    def test_stale_callback_permission_and_ledger_required(self):
        for key, value in [('sourceCurrent', True), ('presentationEnabled', True), ('unchanged', False),
                           ('requestsBefore', 2), ('requestsAfter', 4)]:
            self.reject(lambda r: r['proof']['exercise']['staleCallback'].update({key: value}))

    def test_stale_callback_cannot_change_model(self):
        self.reject(lambda r: r['proof']['exercise']['staleCallback']['afterModel']['entry'].update(active=True))

    def test_stale_callback_cannot_change_document_revision(self):
        self.reject(lambda r: r['proof']['exercise']['staleCallback']['afterSource']['documentRevisions'][0].update(revision=99))

    def test_equal_but_foreign_stale_callback_not_accepted(self):
        def change(r):
            for key in ('beforeSource', 'afterSource'):
                r['proof']['exercise']['staleCallback'][key]['epoch'] = 90
        self.reject(change)

    def test_model_before_autosize_can_be_restored_draft(self):
        f, r, c = sample()
        for key in ('beforeSource', 'afterSource'):
            r['stages'][4]['proof']['exercise']['staleCallback'][key]['threadComposerModel'] = 'Existing successor draft'
        self.validate(f, r, c)

    def test_exact_model_and_native_successor_drafts(self):
        self.reject(lambda r: r['proof']['source'].update(threadComposerModel=SUCCESSOR_THREAD))
        self.reject(lambda r: r['proof']['nativeSurface']['editors'][1].update(value='old'))
        self.reject(lambda r: r['proof']['nativeSurface']['editors'][1].update(start=0))

    def test_successor_identity_survives_reopen(self):
        self.reject(lambda r: r['proof']['nativeSurface'].update(actionKey='third'), stage='reopened')
        self.reject(lambda r: r['proof']['nativeSurface']['editors'][1].update(scope='third'), stage='reopened')

    def test_late_activity_publication_forbidden(self):
        for key in ('page', 'detail', 'detailRow'):
            self.reject(lambda r: r['proof']['exercise']['retainedActivity'][0].update({key: {}}))

    def test_dirty_document_cannot_change_on_retirement(self):
        self.reject(lambda r: r['proof']['source']['documents'][0].update(title='lost'))
        self.reject(lambda r: r['proof']['source']['documentRevisions'][0].update(revision=999))

    def test_unrelated_source_changes_forbidden(self):
        for key, value in [('epoch', 1), ('channelGeneration', 99), ('composerModel', 'written')]:
            def change(r):
                r['proof']['source'][key] = value
                r['proof']['exercise']['afterSource'] = copy.deepcopy(r['proof']['source'])
            self.reject(change)

    def test_native_clipping_and_control_visibility_rejected(self):
        self.reject(lambda r: r['proof']['nativeSurface']['editors'][1].update(scrollHeight=1000))
        self.reject(lambda r: r['geometry']['controls'][0].update(visible=False))

    def test_pins_cannot_claim_composer(self):
        self.reject(lambda r: r['proof']['nativeSurface'].update(threadComposerCount=1), case='execution-pin-removal')

    def test_request_order_and_final_custody(self):
        self.reject(lambda r: r['model']['ledger'].update(cancelled=0), stage='reopened')
        self.reject(lambda r: r['proof']['exercise'].update(newVisit=False), stage='reopened')


if __name__ == '__main__':
    unittest.main()
