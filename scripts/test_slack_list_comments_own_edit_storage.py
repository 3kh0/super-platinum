"""Portable adversarial receipts for real edit SQLite contention boundaries."""
import copy
import json
from pathlib import Path
import runpy
import unittest
from unittest import mock

V=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit_storage.py')))
BASE=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_own_edit.py')))
O,B=V['O'],BASE['B']
DIRECTORY=Path('/private/own-edit-storage')


def sample(narrow=False):
    fixture,base=BASE['sample'](narrow)
    first=base['stages'][0]
    scope=first['journal']['scope']
    seed=V['RST']['seed_value'](first['model'])
    ready=base['readiness']
    def blocks(suffix):
        b=O['received']()['blocks'][:1]
        b[0]['elements'][-1]['elements'][-1]['text']+=suffix
        return b
    b,newer,final=(blocks(v) for v in (V['B'],V['B']+V['C_TEXT'],V['D']))
    def inv(revision,body,seed=seed,attempt=None):
        return dict(scope=copy.deepcopy(scope),ts=O['TS'],revision=revision,
            capture='' if not revision else f'00000000-0000-4000-8000-{revision:012d}',seed=copy.deepcopy(seed),
            blocks=copy.deepcopy(body),attempt=copy.deepcopy(attempt),retry_at=0)
    draft=inv(0,[],None)
    disk_b=inv(1,b)
    disk_c=inv(2,newer)
    cleared=inv(3,[],None)
    disk_d=inv(4,final)
    pending=dict(id='00000000-0000-4000-8000-000000000080',record=dict(version=1,seed=copy.deepcopy(seed),
        blocks=copy.deepcopy(final)+O['received']()['blocks'][1:],text=V['G']['display'](final)),
        capture=disk_d['capture'],phase='Pending',raw=None,readback=None,retry_at=0)
    disk_pending=inv(5,final,attempt=pending)
    disk_pending['capture']=disk_d['capture']
    final_clear=inv(7,[],None)
    inventories=[draft,disk_b,None,disk_c,None,cleared,disk_d,None,disk_d,disk_pending,None,final_clear,final_clear]
    acks=[]
    for index,disk in enumerate((disk_b,disk_c,disk_d,disk_pending)):
        for action in ('hold','release'):
            a=dict(ready,sequence=len(acks)+1,action=action,ok=True,account=first['model']['comments']['account'],
                inventories=[copy.deepcopy(first['journal'])],editInventories=[copy.deepcopy(disk)],
                editHistory=[copy.deepcopy(pending)] if index==3 else [],contentSha256=str(index+1)*64)
            if action=='release':a['unchanged']=True
            acks.append(a)
    mounts=[dict(ok=True,scope='list-edit:100:77',mount='storage-first-mount',generation='77',blocks=copy.deepcopy(b)),
            dict(ok=True,scope='list-edit:100:88',mount='storage-second-mount',generation='88',blocks=copy.deepcopy(final))]
    ack=dict(ok=True,channel=O['CHANNEL'],ts=O['TS'],message=O['update_row'](final))
    readset=first['model']['comments']['ledger']['requests']
    rows=[]
    for i,name in enumerate(V['STAGES']):
        row=copy.deepcopy(first)
        row.update(stage=name,capture=str(DIRECTORY/f'{fixture}-{name}.png'),editInventory=copy.deepcopy(inventories[i]))
        m,f,e=(row['model'][k] for k in ('comments','fixture','edit'))
        current=None if i==0 else b if i==1 else newer if i<6 else final
        mount=mounts[0 if i<6 else 1]
        m['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=j+1) for j,q in enumerate(readset*(V['READS'][i]//4))],
            completed=V['READS'][i],replies=V['READS'][i]//4,cancelled=0,held=False)
        if i==12:
            m['comments']['generation']=200
            d=m['draftStore']['drafts'][0]
            d.update(id='reopened-original',generation=41)
            m['draftStore']['active']=f"{d['id']}:200:{d['generation']}"
        f['policy']=[dict(request=q,response=r) for _ in range(V['POLICIES'][i]//4) for q,r in O['policy_cycle']()]
        f.update(prepared=[O['wire'](final)] if i>=9 else [],updates=[O['wire'](final)] if i>=9 else [],
            responses=[dict(disposition='accepted',raw=copy.deepcopy(ack))] if i>=9 else [],
            futureRows=[copy.deepcopy(ack['message'])] if i>=9 else [],held=i==9,
            releases=[O['RELEASE']] if i>=10 else [],published=i==12,publishedRow=copy.deepcopy(ack['message']) if i==12 else None)
        if i>=11:
            selected=O['one_message'](m['comments']['pageMessages'])
            for key in ('text','blocks','edited'):selected[key]=copy.deepcopy(ack['message'][key])
        if i==12:
            m['comments']['page']['messages']=[copy.deepcopy(ack['message']) if v.get('ts')==O['TS'] else v for v in m['comments']['page']['messages']]
        e.update(opening=False,events=0,retired=[],attempts=[])
        for j in range(0 if i<7 else 1 if i<9 else 2):
            e['attempts'].append(dict(id=120+j,ts=O['TS'],blocks=copy.deepcopy(final)+O['received']()['blocks'][1:],
                phase='not-sent' if j==0 else 'sent' if i==9 else 'confirmed',
                receipt=copy.deepcopy(ack) if j==1 and i>=10 else None,retryAt=0))
        active=i in V['ACTIVE']
        e['active']=dict(scope=mount['scope'],generation=int(mount['generation']),ts=O['TS'],current=True,
            blocks=copy.deepcopy(current),revision=2,busy=i==9,notice=V['NOTICES'].get(i),composing=False,
            source=O['received'](),preserved=O['received']()['blocks'][1:],nativeReady=i not in (2,4),captureFailed=i==2,
            attempt=e['attempts'][-1]['id'] if i>=7 else None) if active else None
        row['model']['capture']=dict(scope=mount['scope'],generation=int(mount['generation']),
            mount=None if i==4 else mount['mount'],failed=i==2,canSave=i in (1,3,6,7,8),
            canCancel=i not in (2,9),canCheck=False,canRetry=i==2,busy=i==9,finishing=i==4) if active else None
        p=row['proof']
        p.pop('actions',None)
        p['editBlocks']=copy.deepcopy(current)
        p['storageControls']=copy.deepcopy(acks[:V['COMMANDS'][i]])
        n=V['COMMANDS'][i]
        p['storage']=None if not n else dict(hold=copy.deepcopy(acks[(n-1)//2*2]),release=None if n%2 else copy.deepcopy(acks[n-1]))
        if i>=1:p.update(originalEditBlocks=copy.deepcopy(b),originalEditRemembered=copy.deepcopy(mounts[0]),editRemembered=copy.deepcopy(mount))
        if i>=2:p.update(newerEditBlocks=copy.deepcopy(newer),blockedCapture=dict(ok=True,saveDisabled=True,cancelDisabled=True))
        if i>=4:p['blockedReturnSave']=dict(ok=True,disabled=True)
        if i>=6:p['finalEditBlocks']=copy.deepcopy(final)
        if i>=9:p['blockedSaving']=dict(ok=True,disabled=True)
        if i>=10:p.update(blockedReceiptSave=dict(ok=True,disabled=True),updateRelease=copy.deepcopy(O['RELEASE']))
        if i==12:
            p['runtimeForgotten']=True
            p['serverPublication']=dict(originalReceipt=copy.deepcopy(ack),publishedRow=copy.deepcopy(ack['message']),updates=1)
        row['renderedBody']=dict(ok=True,ts=O['TS'],text=V['body_text'](ack['message']['blocks']),
            expectedText=V['body_text'](ack['message']['blocks']),bodyCount=1) if i>=11 else None
        if i in V['LOCKED']:row['journal']=None
        g=row['geometry']
        g.update(editing=active,captureFault=None,replyNotice=None,inertTextPaint=None,
            controls=[dict(id=id,disabled=disabled,visible=True,hit=True,rect=B['rect']()) for id,disabled in V['controls'](i)],
            edit=dict(blocks=copy.deepcopy(current),editable=i not in (4,9),sameNode=True,rect=B['rect']()) if active else None,
            notice=dict(text=V['NOTICES'][i],visible=True,hit=True,rect=B['rect']()) if i in V['NOTICES'] else None)
        g['reply'].update(sameNode=i!=12,hidden=active,inert=active,visible=not active)
        if i>=11:g['row']['text']=ack['message']['text']+' Preserved original context'
        if i in (4,9):
            g['text'].update(ok=False,visibleLines=0)
            for line in g['text']['lines']:
                line['hit']=False
                for fragment in line['fragments']:fragment['hit']=False
            g['inertTextPaint']=BASE['inert_paint'](g['text'])
            if i==4:g['inertTextPaint'].update(finishing=True,parentClass='slack-list-comment-compose',parentId='slack-list-comment-edit')
        rows.append(row)
    receipt=dict(ok=True,case='own-edit-storage',fixture=fixture,readiness=ready,stages=rows,
        final=copy.deepcopy(rows[-1]['model']),initialRecords=base['initialRecords'],noLiveNetwork=True)
    accepted=copy.deepcopy(pending)
    accepted.update(phase='Confirmed',raw=ack)
    closed=dict(account=first['model']['comments']['account'],inventories=[first['journal']],editInventories=[final_clear],
        editHistory=[accepted],contentSha256='a'*64)
    return fixture,receipt,closed


class StorageOracleTests(unittest.TestCase):
    def check(self,f,r):return V['validate'](f,r,DIRECTORY,png=False)

    def reject(self,index,mutation):
        f,r,_=sample()
        mutation(r['stages'][index])
        r['final']=copy.deepcopy(r['stages'][-1]['model'])
        with self.assertRaises((RuntimeError,ValueError,TypeError,KeyError,IndexError)):
            self.check(f,r)

    def test_both_widths_and_closed_accepted_history(self):
        for narrow in (False,True):
            f,r,closed=sample(narrow)
            self.assertIs(self.check(f,r),r)
            self.assertIs(V['validate_closed'](closed,r),closed)

    def test_shared_runner_registration(self):
        runner=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
        stages,validator=runner['case_contract']('own-edit-storage')
        self.assertEqual(stages,V['STAGES'])
        f,r,_=sample()
        self.assertIs(validator(f,r,DIRECTORY,png=False),r)

    def test_png_validation_uses_original_dimensions_for_every_stage(self):
        f,r,_=sample()
        # Window captures include native chrome and physical display scaling;
        # DOM rectangles remain measured in CSS pixels.
        dimensions=[1942,1256]
        self.assertNotEqual(dimensions,r['stages'][0]['geometry']['viewport'])
        probe=mock.Mock(return_value=dimensions)
        with mock.patch.dict(V['H'],png_signature=probe):
            V['validate'](f,r,DIRECTORY,dimensions=dimensions)
        self.assertEqual(probe.call_args_list,[mock.call(Path(row['capture']),dimensions) for row in r['stages']])
        with mock.patch.dict(V['H'],png_signature=mock.Mock(side_effect=RuntimeError('PNG dimensions differ'))):
            with self.assertRaisesRegex(RuntimeError,'dimensions'):
                V['validate'](f,r,DIRECTORY,dimensions=dimensions)

    def test_actual_scaled_normal_a_rejects_missing_recovered_publication(self):
        path=Path(__file__).resolve().parents[1]/'tmp/list-comment-own-edit-storage-20261019/normal-a/slack-list/slack-list.json'
        if not path.is_file():self.skipTest('Optional original closed-fixture receipt not retained')
        original=path.read_bytes()
        value=json.loads(original)
        native=value['terminalReceipts'][0]
        self.assertNotEqual(native['stages'][0]['geometry']['viewport'],value['expectedPngDimensions'])
        old='The edit was not sent because its recovery record could not be saved. Retry local capture; both drafts are retained.'
        # This immutable historical binary predates the corrected user guidance.
        # Only this explicitly named regression admits its former exact copy.
        for row in native['stages']:
            V['H']['png_signature'](Path(row['capture']),value['expectedPngDimensions'])
        with mock.patch.dict(V['NOTICES'],{7:old,8:old}):
            with self.assertRaisesRegex(RuntimeError,'Recovered accepted body missing from typed projection'):
                V['validate'](value['fixture'],native,Path(value['readyDirectory']),dimensions=value['expectedPngDimensions'])
        self.assertEqual(path.read_bytes(),original)

    def test_recovered_row_requires_both_projection_and_native_body(self):
        for index in (11,12):
            self.reject(index,lambda r:O['one_message'](r['model']['comments']['comments']['pageMessages']).update(text=O['received']()['text']))
            self.reject(index,lambda r:r['geometry']['row'].update(text=O['received']()['text']+' Preserved original context'))

    def test_original_http_is_immutable_until_independent_reopen_read(self):
        self.reject(11,lambda r:r['model']['comments']['comments']['page']['messages'].__setitem__(1,O['update_row'](r['proof']['finalEditBlocks'])))
        self.reject(12,lambda r:r['model']['comments']['comments'].update(page=sample()[1]['stages'][0]['model']['comments']['comments']['page']))
        self.reject(12,lambda r:r['model']['fixture'].update(published=False,publishedRow=None))

    def test_recovered_projection_cannot_drop_context_reactions_or_change_author(self):
        for mutate in (lambda m:m.update(reactions=[]),lambda m:m.update(user='UOTHER'),lambda m:m['blocks'].pop()):
            self.reject(11,lambda r:mutate(O['one_message'](r['model']['comments']['comments']['pageMessages'])))

    def test_rendered_body_cannot_certify_its_own_wrong_expected_text(self):
        self.reject(11,lambda r:r['renderedBody'].update(text='old',expectedText='old'))
        self.reject(11,lambda r:r['renderedBody'].update(bodyCount=2))
        self.reject(12,lambda r:r.update(renderedBody=None))
        self.reject(11,lambda r:r['renderedBody'].update(text=r['renderedBody']['text']+' '))

    def test_canonical_publication_is_bound_to_immutable_original_ack(self):
        self.reject(12,lambda r:r['proof']['serverPublication'].update(updates=2))
        self.reject(12,lambda r:r['proof']['serverPublication']['originalReceipt']['message'].update(text='rewritten'))

    def test_runtime_requires_actionable_save_guidance(self):
        self.reject(7,lambda r:r['geometry']['notice'].update(text='The edit was not sent because its recovery record could not be saved. Retry local capture; both drafts are retained.'))

    def test_lock_ack_bound_to_pid_and_sequence(self):
        for key,value in (('pid',999),('sequence',3),('action','release'),('ok',False)):
            self.reject(12,lambda r:r['proof']['storageControls'][0].update({key:value}))

    def test_lock_read_cannot_be_fabricated(self):
        self.reject(2,lambda r:r.update(journal=sample()[1]['stages'][0]['journal']))

    def test_locked_database_cannot_claim_newer_unsaved_buffer(self):
        self.reject(12,lambda r:r['proof']['storageControls'][0]['editInventories'][0].update(blocks=r['proof']['newerEditBlocks']))

    def test_ack_must_not_replace_pending_history_while_locked(self):
        self.reject(12,lambda r:r['proof']['storageControls'][6]['editHistory'][0].update(phase='Confirmed'))

    def test_release_must_preserve_actual_locked_rows(self):
        self.reject(12,lambda r:r['proof']['storageControls'][7].update(contentSha256='b'*64))

    def test_cannot_skip_fourth_storage_pair(self):
        self.reject(12,lambda r:r['proof']['storageControls'].pop())

    def test_capture_retry_keeps_exact_editor_node_mount(self):
        self.reject(3,lambda r:r['geometry']['edit'].update(sameNode=False))
        self.reject(3,lambda r:r['model']['capture'].update(mount='replacement'))

    def test_unsaved_edit_and_reply_are_independent(self):
        self.reject(2,lambda r:r['model']['edit']['active']['blocks'][0]['elements'][0]['elements'][0].update(style={}))
        self.reject(4,lambda r:r['geometry']['reply'].update(hidden=False))
        self.reject(11,lambda r:r['journal']['blocks'][0]['elements'][0]['elements'][-1].update(text='lost'))

    def test_finishing_is_exact_inert_region_not_arbitrary_parent(self):
        for key,value in (('parentId','other'),('parentClass','slack-list-comment-compose-editor'),('finishing',False)):
            self.reject(4,lambda r:r['geometry']['inertTextPaint'].update({key:value}))
        self.reject(4,lambda r:r['geometry']['edit'].update(editable=True))
        self.reject(4,lambda r:r['model']['capture'].update(canSave=True))

    def test_finishing_must_show_real_text_not_only_toolbar(self):
        self.reject(4,lambda r:r['geometry']['inertTextPaint']['lines'][0]['fragments'][0].update(hitParent=False))
        self.reject(4,lambda r:r['geometry']['inertTextPaint'].update(visibleLines=0))
        self.reject(4,lambda r:r['geometry']['inertTextPaint']['lines'][0]['rect'].update(bottom=9999))

    def test_failed_begin_cannot_issue_update_or_durable_attempt(self):
        self.reject(7,lambda r:r['model']['fixture']['updates'].append(O['wire'](r['proof']['finalEditBlocks'])))
        self.reject(8,lambda r:r['editInventory'].update(attempt=sample()[1]['stages'][9]['editInventory']['attempt']))

    def test_receipt_retry_cannot_reissue_or_enable_save(self):
        self.reject(10,lambda r:r['model']['fixture']['updates'].append(copy.deepcopy(r['model']['fixture']['updates'][0])))
        self.reject(10,lambda r:r['model']['capture'].update(canSave=True))
        self.reject(10,lambda r:r['geometry']['controls'][1].update(disabled=False))

    def test_raw_ack_and_original_body_never_rewritten(self):
        self.reject(10,lambda r:r['model']['fixture']['responses'][0]['raw']['message'].update(user='UOTHER'))
        self.reject(11,lambda r:r['model']['comments']['comments']['page']['messages'][1].update(text='ack replaced HTTP'))
        self.reject(10,lambda r:r['model']['edit']['attempts'][1].update(receipt=None))

    def test_rich_update_has_no_text_fallback_or_wrong_target(self):
        for key,value in (('text','fallback'),('channel','COTHER'),('ts','1781524800.000001')):
            self.reject(9,lambda r:r['model']['fixture']['prepared'][0]['args'].update({key:value}))

    def test_no_unrelated_post_delete_or_reaction_write(self):
        self.reject(9,lambda r:r['model']['comments']['composeLedger']['posts'].append({}))
        self.reject(9,lambda r:r['model']['fixture']['prepared'][0].update(path='/api/chat.delete'))

    def test_tombstone_and_actual_forget_reopen_required(self):
        self.reject(11,lambda r:r['editInventory'].update(seed=V['RST']['seed_value'](r['model'])))
        self.reject(12,lambda r:r['proof'].update(runtimeForgotten=False))
        self.reject(12,lambda r:r['geometry']['reply'].update(sameNode=True))

    def test_closed_history_retains_original_uuid_and_original_ack(self):
        f,r,closed=sample()
        for mutate in (lambda c:c.update(editHistory=[]),lambda c:c['editHistory'][0].update(phase='Observed'),
                lambda c:c['editHistory'][0].update(id='00000000-0000-4000-8000-000000000081'),
                lambda c:c['editHistory'][0].update(raw=None),lambda c:c['editInventories'][0].update(blocks=r['stages'][6]['proof']['finalEditBlocks'])):
            value=copy.deepcopy(closed)
            mutate(value)
            with self.assertRaises((RuntimeError,ValueError,TypeError,KeyError)):
                V['validate_closed'](value,r)

    def test_original_failure_notice_and_closed_identity_required(self):
        self.reject(4,lambda r:r['geometry'].update(notice=None))
        self.reject(10,lambda r:r['model']['fixture'].update(httpCapability=True))
        self.reject(0,lambda r:r['model']['comments'].update(account='00000000-0000-4000-8000-000000000099'))


if __name__=='__main__':unittest.main()
