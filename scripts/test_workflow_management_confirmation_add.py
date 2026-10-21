import copy
import unittest
import workflow_management_confirmation_add as A


def sample(case,stage):
    selected=stage not in ('checked','done') or case=='first-add-unavailable'
    submitted=stage in ('added','duplicate','rejected','add-held','successor','late-settled')
    listing_ids=list(A.CHANNELS)
    methods=['conversations.info']*len(listing_ids)+(['conversations.info'] if selected else [])+(['conversations.info','bookmarks.list'] if submitted else [])+(['bookmarks.add'] if submitted and case!='first-add-duplicate' else [])
    requests=[];responses=[]
    for i,method in enumerate(methods,1):
        mutation=method=='bookmarks.add'
        initial=i<=len(listing_ids)
        channel=listing_ids[i-1] if initial else A.DEST
        requests.append(dict(serial=i,phase='mutation' if mutation else 'read',method=method,fields=list(map(list,A.fields(method,channel).items())),retrySafe=not mutation,applied=mutation and case!='first-add-rejected',owner=copy.deepcopy(A.OWNER)))
        if method=='conversations.info':result={'error':'offline'} if case=='first-add-unavailable' and not initial else {'raw':dict(ok=True,channel=copy.deepcopy(A.CHANNELS[channel]),opaque_receipt='original-add-channel')}
        elif method=='bookmarks.list':result={'raw':dict(ok=True,bookmarks=[A.bookmark()] if case=='first-add-duplicate' else [],opaque_receipt='original-shortcut-list')}
        else:result={'raw':A.rejection() if case=='first-add-rejected' else dict(ok=True,bookmark=A.bookmark(),opaque_receipt='original-shortcut-added')}
        responses.append(dict(serial=i,result=result))
    held=stage in ('add-held','successor');n=len(methods)
    return dict(confirmationAdd=dict(requests=requests,responses=responses,settlements=[dict(serial=i,returned=True) for i in range(1,n+1) if not held or i!=n],held=[n] if held else [],releases=[dict(serial=n,receiverRetired=False,delivered=True)] if stage=='late-settled' else [],bookmarks=[A.bookmark()] if submitted and case not in ('first-add-duplicate','first-add-rejected') else [],networkCalls=0))

class AddLedger(unittest.TestCase):
    def valid(self,case='first-add-success',stage='added'):
        server=sample(case,stage);A.add_ledger(case,stage,server);return server
    def reject(self,mutate,case='first-add-success',stage='added'):
        server=self.valid(case,stage);mutate(server['confirmationAdd'])
        with self.assertRaises(RuntimeError):A.add_ledger(case,stage,server)
    def test_all_case_ledgers(self):
        for case,stages in [('first-add-no-selection',['done']),('first-add-success',['selected','added']),('first-add-duplicate',['duplicate']),('first-add-rejected',['rejected']),('first-add-unavailable',['unavailable','done']),('first-add-retirement',['add-held','successor','late-settled'])]:
            for stage in stages:
                with self.subTest(case=case,stage=stage):self.valid(case,stage)
    def test_replay(self):self.reject(lambda v:v['requests'].append(copy.deepcopy(v['requests'][-1])))
    def test_wrong_reason(self):self.reject(lambda v:v['requests'][-1]['fields'].append(['_x_reason','wrong']))
    def test_link_bookmark_not_shortcut(self):self.reject(lambda v:v['responses'][-1]['result']['raw']['bookmark'].update(type='link'))
    def test_wrong_shortcut_id(self):self.reject(lambda v:v['responses'][-1]['result']['raw']['bookmark'].update(shortcut_id='TR_OTHER'))
    def test_wrong_channel(self):self.reject(lambda v:v['requests'][0]['fields'].__setitem__(0,['channel',A.PRIVATE]))
    def test_wrong_owner(self):self.reject(lambda v:v['requests'][-1]['owner'].update(epoch=99))
    def test_mutation_retry_safe(self):self.reject(lambda v:v['requests'][-1].update(retrySafe=True))
    def test_read_retry_bit(self):self.reject(lambda v:v['requests'][0].update(retrySafe=False))
    def test_rejected_applied(self):self.reject(lambda v:v['requests'][-1].update(applied=True),'first-add-rejected','rejected')
    def test_offline_fake_raw_success(self):self.reject(lambda v:v['responses'][-1].update(result={'raw':{'ok':True}}),'first-add-unavailable','unavailable')
    def test_duplicate_missing_original(self):self.reject(lambda v:v['responses'][-1]['result']['raw'].update(bookmarks=[]),'first-add-duplicate','duplicate')
    def test_held_synthetic_settlement(self):self.reject(lambda v:v['settlements'].append({'serial':9,'returned':True}),'first-add-retirement','add-held')
    def test_late_cancelled_delivery(self):self.reject(lambda v:v['releases'][0].update(delivered=False),'first-add-retirement','late-settled')
    def test_late_rollback(self):self.reject(lambda v:v.update(bookmarks=[]),'first-add-retirement','late-settled')
    def test_unknown_response_fields_not_dropped(self):self.reject(lambda v:v['responses'][-1]['result']['raw'].pop('opaque_receipt'))
    def test_all_http_rows_have_no_derived_foreign_grant(self):
        self.assertTrue(all('fromAnotherTeam' not in row for row in A.channels()))
        self.assertEqual(len(A.channels()),5)
    def test_null_run_draft_fails_explicitly(self):
        with self.assertRaisesRegex(RuntimeError,'Original embedded run settings unavailable'):
            A.embedded_settings('finish',{'confirmation':{'runDraft':None}})
    def test_original_run_draft_required(self):
        A.embedded_settings('details',{'confirmation':{'runDraft':{'mode':'everyone'}}})
    def test_listing_missing_row(self):
        self.reject(lambda v:v['responses'][2]['result']['raw']['channel'].update(id=A.DEST))
    def test_listing_offline_cannot_replace_selected_failure(self):
        self.reject(lambda v:v['responses'][0].update(result={'error':'offline'}),'first-add-unavailable','unavailable')
    def test_no_selection_still_acquires_five_original_rows(self):
        v=self.valid('first-add-no-selection','done')['confirmationAdd']
        self.assertEqual(len(v['requests']),5)
        self.assertTrue(all(r['phase']=='read' for r in v['requests']))
    def test_explicit_auth_configuration_required(self):
        seed={'boot':{'channels':[{'id':A.ORIGINAL}]+A.channels()},'canonicalChannels':[dict(v,fromAnotherTeam=False) for v in A.channels()],'unifiedUserClientEnabled':{'TWORKFLOW':False}}
        A.seed_proof({'seed':seed})
        for value in ({},{'TWORKFLOW':True},None):
            with self.subTest(value=value),self.assertRaisesRegex(RuntimeError,'explicit non-unified'):
                A.seed_proof({'seed':dict(seed,unifiedUserClientEnabled=value)})
    def test_scope_stages(self):
        self.assertEqual([len(v) for v in A.STAGES.values()],[8,9,9,9,11,9])

if __name__=='__main__':unittest.main()
