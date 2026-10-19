"""Adversarial receipts for local journal failures; no app or live network."""
import copy
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch

V = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_storage_recovery.py')))
BASE = runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_newer_draft.py')))
DIR = Path('/private/storage-recovery')


def sample(narrow=False):
    fixture, base = BASE['sample'](narrow)
    first = base['stages'][0]
    seed = copy.deepcopy(first['proof']['seed'])
    scope = seed['scope']
    a = copy.deepcopy(first['proof']['originalA'])
    a[0]['elements'][1]['elements'][0]['text'] = V['A_TEXT'].split('\n')[1]
    b = copy.deepcopy(a)
    b[0]['elements'].extend(dict(type='rich_text_section',elements=[dict(type='text',text=text)]) for text in V['B_TEXT'][1:].split('\n'))
    cap_a,cap_b,cap_c = [f'11111111-2222-4333-8444-{i:012}' for i in (1,2,3)]
    account=first['model']['account']
    post=copy.deepcopy(first['model']['composeLedger']['posts'][0])
    post['args']['blocks']=json.dumps(b,ensure_ascii=False)
    record=copy.deepcopy(first['proof']['journal']['attempt']['record'])
    record['blocks']=copy.deepcopy(b)
    attempt=dict(record=record,capture=cap_b,raw=None)
    ia=dict(scope=copy.deepcopy(scope),revision=4,capture=cap_a,blocks=copy.deepcopy(a),attempt=None)
    ib=dict(scope=copy.deepcopy(scope),revision=5,capture=cap_b,blocks=copy.deepcopy(b),attempt=None)
    ip=dict(scope=copy.deepcopy(scope),revision=6,capture=cap_b,blocks=copy.deepcopy(b),attempt=copy.deepcopy(attempt))
    cleared=dict(scope=copy.deepcopy(scope),revision=7,capture=cap_c,blocks=[],attempt=None)
    hold={}
    release={}
    for sequence,inv in ((1,ia),(3,ia),(5,ip)):
        hold[sequence]=dict(base['readiness'],sequence=sequence,action='hold',ok=True,account=account,
                            inventories=[copy.deepcopy(inv)],contentSha256=str(sequence)*64)
        release[sequence]=dict(copy.deepcopy(hold[sequence]),sequence=sequence+1,action='release',unchanged=True)
    rows=[]
    for i,name in enumerate(V['STAGES']):
        model=copy.deepcopy(first['model'])
        model['composeLedger'].update(posts=[],prepared=[],messages=[],policies=[],preflights=[],held=False,heldPost=False,published=False)
        if i>=6:
            model['composeLedger'].update(posts=[copy.deepcopy(post)],prepared=[copy.deepcopy(post)],
                messages=[dict(type='message',channel='CCOMMENT01',thread_ts=scope['root'],user=scope['user'],blocks=copy.deepcopy(b),
                    client_msg_id=post['args']['client_msg_id'],ts='1781525900.000001')],
                policies=copy.deepcopy(first['model']['composeLedger']['policies']),preflights=copy.deepcopy(first['model']['composeLedger']['preflights']))
        model['comments']['generation']=11 if i<5 else 12
        generation=20 if i<5 else 30 if i==5 or i==6 else 40
        journal=copy.deepcopy((None,ia,None,None,ib,ib,None,cleared)[i])
        inv=ia if i<4 else ib if i<6 else ip if i==6 else cleared
        blocks=[] if i in (0,7) else a if i==1 else b
        capture=cap_c if i in (0,7) else cap_a if i==1 else cap_b
        error=V['ERRORS'].get(i,V['ERRORS'][2] if i==3 else None)
        draft=dict(id='loaded' if i<5 else 'reopened',scope=copy.deepcopy(scope),blocks=copy.deepcopy(blocks),capture=capture,
            sequence=i+1,generation=generation,busy=False,saving=False,loading=False,captureFailed=False,native=i!=0,
            savedRevision=None if i==0 else inv['revision'],attempt=copy.deepcopy(attempt) if i==6 else None,
            terminalPending=i==6,error=error,status='Comment sent.' if i==7 else None)
        model['draftStore']=dict(active=f"{draft['id']}:{model['comments']['generation']}:{generation}",drafts=[draft])
        proof=dict(seed=copy.deepcopy(seed),originalA=copy.deepcopy(a),newerB=None if i<2 else copy.deepcopy(b),
            runtimeForgotten=i in (0,1,5),journal=journal)
        pair=1 if i<2 else 3 if i<6 else 5
        proof['storage']=dict(hold=copy.deepcopy(hold[pair]),release=None if i in V['FAILED'] else copy.deepcopy(release[pair]))
        if i>=6:proof['postRelease']=dict(delivered=True,cancelledBeforeReply=False)
        geometry=copy.deepcopy(first['geometry'])
        geometry['notices']=[error] if error else ['Comment sent.'] if i==7 else []
        geometry['editor']=None if i==0 else dict(geometry['editor'],blocks=copy.deepcopy(blocks),text=V['N']['visible_text'](blocks) if blocks else '\n',
                                                generation=str(generation),bold=i!=7,editable=True)
        geometry['controls'][2]['disabled']=i in V['FAILED'] or i==7
        if i in V['FAILED']:
            geometry['controls'].append(dict(geometry['controls'][0],id='slack-list-comment-retry',disabled=False))
        rows.append(dict(stage=name,proof=proof,model=model,geometry=geometry,capture=str(DIR/f'{fixture}-{name}.png')))
    return fixture,dict(ok=True,case='storage-recovery',fixture=fixture,readiness=base['readiness'],initialRecords=base['initialRecords'],
                        noLiveNetwork=True,stages=rows,final=copy.deepcopy(rows[-1]['model']))


class RecoveryTests(unittest.TestCase):
    def check(self,f,r):return V['validate'](f,r,DIR,png=False)
    def reject(self,change,stage=2):
        f,r=sample();change(r['stages'][stage])
        with self.assertRaises((RuntimeError,KeyError,ValueError,TypeError)):self.check(f,r)

    def test_normal_and_narrow(self):
        for narrow in (False,True):self.check(*sample(narrow))

    def test_unicode_codepoints_not_self_referencing_oracle(self):
        self.assertEqual([ord(c) for c in V['A_TEXT'] if ord(c)>127],[0x4E2D,0x1F642])
        self.assertEqual([ord(c) for c in V['B_TEXT'] if ord(c)>127],[0x65E5,0x1F98A])

    def test_stage_order_and_case(self):
        for change in (lambda r:r.update(case='compose'),lambda r:r['stages'].pop(),lambda r:r['stages'].reverse()):
            f,r=sample();change(r)
            with self.assertRaises(RuntimeError):self.check(f,r)

    def test_account_session_network_source_custody(self):
        for key,value in (('account','foreign'),('epoch',1),('noSession',False),('noTransport',False),('writes',1),('networkCalls',1),('document',{}),('records',{}),('unrelated',{})):
            self.reject(lambda row:row['model'].update({key:value}))
        self.reject(lambda row:row['model']['comments'].update(root='other'))

    def test_ack_original_identity_sequence_fields(self):
        for key,value in (('pid',9),('version',True),('sequence',2),('nonce','foreign'),('action','release'),('ok',False),('account','foreign'),('inventories',[]),('contentSha256','x')):
            self.reject(lambda row:row['proof']['storage']['hold'].update({key:value}))
        self.reject(lambda row:row['proof']['storage']['hold'].update(path='/real/database'))

    def test_lock_cannot_claim_disk_read_while_locked(self):
        for i in (0,2,3,6):
            self.reject(lambda row:row['proof'].update(journal=copy.deepcopy(row['proof']['storage']['hold']['inventories'][0])),i)
            self.reject(lambda row:row['proof']['storage'].update(release=copy.deepcopy(row['proof']['storage']['hold'])),i)

    def test_release_must_prove_unchanged_data(self):
        for i in (1,4,5,7):
            self.reject(lambda row:row['proof']['storage']['release'].update(unchanged=False),i)
            self.reject(lambda row:row['proof']['storage']['release'].update(contentSha256='f'*64),i)
            self.reject(lambda row:row['proof']['storage']['release']['inventories'][0].update(blocks=[]),i)

    def test_failed_load_never_mounts_untracked_editor(self):
        f,r=sample();r['stages'][0]['geometry']['editor']=copy.deepcopy(r['stages'][1]['geometry']['editor'])
        with self.assertRaises(RuntimeError):self.check(f,r)
        self.reject(lambda row:row['model']['draftStore']['drafts'][0].update(native=True),0)
        self.reject(lambda row:row['model']['draftStore']['drafts'][0].update(savedRevision=4),0)

    def test_load_retry_exact_disk_inventory(self):
        self.reject(lambda row:row['proof']['journal'].update(revision=99),1)
        self.reject(lambda row:row['model']['draftStore']['drafts'][0].update(id='foreign'),1)

    def test_save_failure_cannot_erase_b_or_falsify_saved_a(self):
        self.reject(lambda row:row['geometry']['editor'].update(blocks=row['proof']['originalA']))
        self.reject(lambda row:row['model']['draftStore']['drafts'][0].update(blocks=row['proof']['originalA']))
        self.reject(lambda row:row['proof']['storage']['hold']['inventories'][0].update(blocks=row['proof']['newerB']))
        self.reject(lambda row:row['model']['draftStore']['drafts'][0].update(savedRevision=99))

    def test_back_refusal_preserves_original_mount_and_scope(self):
        for key,value in (('generation',999),('id','other'),('capture','11111111-2222-4333-8444-999999999999')):
            self.reject(lambda row:row['model']['draftStore']['drafts'][0].update({key:value}),3)
        self.reject(lambda row:row['model']['comments'].update(generation=99),3)

    def test_explicit_save_retry_and_real_journal_reopen(self):
        self.reject(lambda row:row['proof']['journal'].update(blocks=[]),4)
        self.reject(lambda row:row['proof'].update(runtimeForgotten=False),5)
        self.reject(lambda row:row['model']['draftStore']['drafts'][0].update(id='loaded'),5)
        self.reject(lambda row:row['model']['comments'].update(generation=11),5)

    def test_receipt_failure_requires_durable_original_attempt_and_terminal(self):
        self.reject(lambda row:row['model']['draftStore']['drafts'][0].update(terminalPending=False),6)
        self.reject(lambda row:row['proof']['storage']['hold']['inventories'][0].update(attempt=None),6)
        self.reject(lambda row:row['model']['draftStore']['drafts'][0].update(attempt=None),6)
        self.reject(lambda row:row['proof']['postRelease'].update(delivered=False),6)

    def test_receipt_retry_cannot_repost_or_replace_receipt(self):
        self.reject(lambda row:row['model']['composeLedger']['posts'].append(copy.deepcopy(row['model']['composeLedger']['posts'][0])),7)
        self.reject(lambda row:row['model']['composeLedger']['prepared'][0]['args'].update(client_msg_id='00000000-0000-4000-8000-000000000099'),7)
        self.reject(lambda row:row['model']['composeLedger']['messages'][0].update(user='other'),7)
        self.reject(lambda row:row['model']['draftStore']['drafts'][0].update(terminalPending=True),7)

    def test_no_post_during_load_or_autosave_recovery(self):
        f,r=sample();post=r['stages'][6]['model']['composeLedger']['posts'][0]
        for i in range(6):self.reject(lambda row:row['model']['composeLedger']['posts'].append(copy.deepcopy(post)),i)

    def test_attempt_exact_post_and_scope_even_if_consistent_inventory(self):
        f,r=sample()
        for i in (6,7):
            r['stages'][i]['proof']['storage']['hold']['inventories'][0]['attempt']['record']['target']['root']='other'
        r['stages'][6]['model']['draftStore']['drafts'][0]['attempt']=copy.deepcopy(r['stages'][6]['proof']['storage']['hold']['inventories'][0]['attempt'])
        with self.assertRaisesRegex(RuntimeError,'POST/source'):self.check(f,r)

    def test_opaque_and_style_prefix_cannot_change(self):
        for mode in ('opaque','style','paragraph'):
            f,r=sample()
            for row in r['stages'][2:]:
                sections=row['proof']['newerB'][0]['elements']
                if mode=='opaque':sections[0]['elements'][1]['opaque']['retain'].reverse()
                elif mode=='style':sections[0]['elements'][4]['style']['bold']=False
                else:sections[2]['elements']+=sections.pop(3)['elements']
            with self.assertRaises(RuntimeError):self.check(f,r)

    def test_native_text_cannot_match_only_flattened_leaves(self):
        self.reject(lambda row:row['geometry']['editor'].update(text=''.join(c for c,_ in V['N']['text_units'](row['proof']['newerB']))))

    def test_geometry_controls_and_errors(self):
        for i in range(8):
            self.reject(lambda row:row['geometry']['controls'][2].update(disabled=not row['geometry']['controls'][2]['disabled']),i)
        for i in (0,2,3,6):
            self.reject(lambda row:row['geometry'].update(notices=[]),i)
            self.reject(lambda row:row['geometry']['controls'][-1].update(disabled=True),i)
        self.reject(lambda row:row['geometry']['controls'][0].update(hit=False))
        self.reject(lambda row:row['geometry']['editor']['rect'].update(right=99999))
        self.reject(lambda row:row['geometry'].update(viewport=[float('nan'),600]))

    def test_all_storage_work_settled(self):
        for key in ('busy','saving','loading','captureFailed'):
            self.reject(lambda row:row['model']['draftStore']['drafts'][0].update({key:True}))

    def test_exact_png_paths_and_dimensions(self):
        f,r=sample();seen=[]
        with patch.dict(V['H'],{'png_signature':lambda p,d:seen.append((p,d))}):
            V['validate'](f,r,DIR,dimensions=[1280,780])
        self.assertEqual(seen,[(DIR/f'{f}-{name}.png',[1280,780]) for name in V['STAGES']])
        self.reject(lambda row:row.update(capture='/foreign.png'))

    def test_final_state_custody(self):
        f,r=sample();r['final']['epoch']=99
        with self.assertRaises(RuntimeError):self.check(f,r)


if __name__=='__main__':unittest.main()
