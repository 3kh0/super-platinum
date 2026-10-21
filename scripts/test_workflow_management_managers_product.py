"""Receipt forgery regressions; synthetic inputs do not claim native acceptance."""
import copy
import unittest
from workflow_management_delete import TITLE, DESCRIPTION
import workflow_management_managers_ledger as L
import workflow_management_managers_product as P
from workflow_management_managers import boot_member
from workflow_management_managers_exercises import managed_entry
from test_workflow_management_managers_ledger import sample


def acknowledged():
    server = sample()
    added = L.response_for('managers-success', L.ADD)['raw']
    removed = L.response_for('managers-success', L.REMOVE)['raw']
    attempt = dict(id=10, visit=7, document=8, revision=9, title=TITLE, description=DESCRIPTION,
                   workflow='WF_REVIEW', originalCopy=copy.deepcopy(L.COPY), finished=True, complete=True,
                   resolved=True, noSend=False, current=False, departure=dict(armed=False, departed=False, revoked=False),
                   managerGroup=dict(selected=['UWORKFLOW', L.NEW], selfRemoval=False, complete=True, slots=[
                       dict(index=0, method=L.ADD, arguments={'workflow_id': 'WF_REVIEW', 'collaborator_ids': [L.NEW]},
                            prerequisite=None, outcome='acknowledged', raw=added, failure=None),
                       dict(index=1, method=L.REMOVE, arguments={'workflow_id': 'WF_REVIEW', 'collaborator_ids': [L.OLD]},
                            prerequisite=0, outcome='acknowledged', raw=removed, failure=None)]),
                   copyChanged=False, desired='collaborators', primarySucceeded=False,
                   slots=[dict(index=0, method=L.COPY_METHOD, arguments={'workflow_id': 'WF_REVIEW', 'permission_type': 'collaborators'},
                               prerequisite=None, outcome='pending', raw=None, failure=None)], runGroup=None,
                   readback={k: copy.deepcopy(v) for k, v in server['current'].items() if k != 'restriction'})
    attempt['readback']['matches'] = True
    return attempt, server


class ProductTests(unittest.TestCase):
    def test_triggerless_entry_requires_actual_row_and_original_read_prefix(self):
        managed = dict(model=dict(route='managed', busy=False, visit=7, writes=0, reads=6),
                       permissions=dict(document=None), native=dict(fields=None),
                       server={key: list(range(6)) for key in ('requests', 'responses', 'settlements')})
        current = dict(model=dict(visit=7, writes=0, reads=15),
                       server={key: list(range(15)) for key in ('requests', 'responses', 'settlements')})
        exercise = dict(managementEntry=dict(managed=managed, clicked=dict(
            id='workflow-management-open-WF_REVIEW', text='Review workflow', ok=True, connected=True, hit=True)))
        managed_entry(exercise, current)
        bad = copy.deepcopy(exercise)
        bad['managementEntry']['clicked']['hit'] = False
        with self.assertRaises(RuntimeError):
            managed_entry(bad, current)
        bad = copy.deepcopy(current)
        bad['server']['requests'][0] = 'replacement'
        with self.assertRaises(RuntimeError):
            managed_entry(exercise, bad)

    def test_boot_member_keeps_wire_flags_and_derives_only_canonical_name_fields(self):
        raw = dict(id='UWORKFLOW', name='You', team_id='TWORKFLOW', is_bot=False, profile={'display_name': 'You'})
        derived = boot_member(raw)
        self.assertEqual(raw['profile'], {'display_name': 'You'})
        self.assertEqual(derived['profile'], dict(display_name='You', display_name_normalized='You',
                                                 real_name_normalized='', first_name='', last_name=''))
        self.assertIs(derived['is_self'], True)
        self.assertIs(derived['is_bot'], False)
        self.assertEqual(derived['_display_name_lc'], 'you')

    def check(self, attempt, server):
        P.attempt_proof(attempt, L.stage_state('managers-success', 'checked'), server,
                        L.selected('managers-success'), L.desired_run('managers-success'))

    def test_raw_intermediate_and_final_responses_remain_distinct(self):
        a, s = acknowledged()
        self.check(a, s)
        a['managerGroup']['slots'][0]['raw'] = copy.deepcopy(a['managerGroup']['slots'][1]['raw'])
        with self.assertRaises(RuntimeError):
            self.check(a, s)

    def test_ack_does_not_replace_readback_or_unlock_pending_draft(self):
        a, s = acknowledged()
        a['readback'] = None
        with self.assertRaises(RuntimeError):
            self.check(a, s)

    def test_no_fabricated_self_departure_authority(self):
        a, s = acknowledged()
        a['departure']['armed'] = True
        with self.assertRaises(RuntimeError):
            self.check(a, s)

    def test_remove_dependency_cannot_be_erased(self):
        a, s = acknowledged()
        a['managerGroup']['slots'][1]['prerequisite'] = None
        with self.assertRaises(RuntimeError):
            self.check(a, s)

    def test_unknown_dispatch_is_not_ack_and_keeps_raw_absent(self):
        server = sample('managers-add-unknown', 'unconfirmed')
        self.assertEqual(P.operation(server, L.ADD), ('unknown', None, 'delivery unconfirmed'))
        self.assertEqual(P.operation(server, L.REMOVE), ('pending', None, None))

    def test_retired_held_response_does_not_authorize_remove(self):
        held = sample('managers-retirement', 'successor')
        self.assertEqual(P.operation(held, L.ADD), ('in-flight', None, None))
        late = sample('managers-retirement', 'late-settled')
        self.assertEqual(P.operation(late, L.ADD)[0], 'acknowledged')
        self.assertEqual(P.operation(late, L.REMOVE), ('pending', None, None))

    def test_opaque_original_metadata_cannot_be_replaced_on_finish(self):
        source = L.original('managers-success')
        retained = dict(id=8, revision=10, title=TITLE, description=DESCRIPTION, pending=10,
                        editor=copy.deepcopy(source['workflow']), trigger=copy.deepcopy(source['trigger']))
        P.retained_proof(retained, source, source, 8, 10, False, TITLE, DESCRIPTION)
        retained['editor']['title'] = TITLE
        with self.assertRaises(RuntimeError):
            P.retained_proof(retained, source, source, 8, 10, False, TITLE, DESCRIPTION)


if __name__ == '__main__':
    unittest.main()
