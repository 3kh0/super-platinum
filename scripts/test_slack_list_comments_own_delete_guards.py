"""Permission/source refusal and actual cooldown custody adversarial receipts."""
import copy
import json
from pathlib import Path
import runpy
import unittest

V=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_delete_guards.py')))
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_own_delete.py')))
D,O=V['D'],V['O'];DIRECTORY=B['DIRECTORY']


def sample(case='own-delete-guards',narrow=False):
    fixture,base=B['sample'](narrow=narrow);guard=case=='own-delete-guards'
    mapping=(0,1,1,2,1,1,2,6) if guard else (0,1,1,2,1,1,4,5,6)
    initial=base['stages'][0]['model']['comments'];readset=initial['ledger']['requests'][:4];rows=[];deadline=1015
    for i,(name,old) in enumerate(zip(V['STAGES'][case],mapping)):
        r=copy.deepcopy(base['stages'][old]);r.update(stage=name,capture=str(DIRECTORY/f'{fixture}-{name}.png'))
        modal,held,removed,published,reopened,count=V['layout'](case,i)
        m,f,d=r['model']['comments'],r['model']['fixture'],r['model']['delete'];c=m['comments'];p=r['proof'];g=r['geometry']
        n=V['READS'][case][i];m['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=j+1) for j,q in enumerate(readset*(n//4))],completed=n,replies=n//4,cancelled=0,held=False)
        if guard and reopened:
            c.update(page=copy.deepcopy(initial['comments']['page']),pageMessages=copy.deepcopy(initial['comments']['pageMessages']),deleted=[],visibleTs=[V['TS']],reactions=copy.deepcopy(initial['comments']['reactions']))
            g['rows']=copy.deepcopy(base['stages'][0]['geometry']['rows'])
        if guard and i in (5,6):
            changed=V['REC']['changed_row']();c['pageMessages']=[V['E']['typed_edit'](row,changed['text'],changed['blocks'],changed['edited'],None,changed['reactions']) if row.get('ts')==V['TS'] else row for row in c['pageMessages']]
            g['rows'][0]['text']=changed['text']+' Second editable line. Preserved original context'
        policy=[copy.deepcopy(q) for _ in range(V['POLICIES'][case][i]) for q in D['policy_cycle']()]
        if guard and i>=2:policy[3]['response']['allowed']=[]
        f.update(policy=policy,prepared=[D['wire']()]*count,deletes=[D['wire']()]*count,records=[D['source'](initial)]*count,
            responses=[dict(disposition='unconfirmed-rate-limited',raw=None,retryAfter=15),dict(disposition='accepted',raw=copy.deepcopy(D['ACK']),retryAfter=None)][:count],
            held=False,releases=[],futureDeleted=V['TS'] if count==2 else None,published=published,deleted=[V['TS']] if published else [])
        if modal:d['active'].update(phase='RateLimited' if not guard and i>=2 else None,sourceCurrent=not(guard and i==5),canConfirm=V['can_confirm'](case,i),canCheck=False)
        d['retainedCount']=int(i>0)
        for ctl in g['controls']:ctl['disabled']=(ctl['id']==D['PREFIX']+'confirm' and not V['can_confirm'](case,i)) or (not guard and i==3 and ctl['id']=='slack-list-comment-send')
        wanted=V['notice'](case,i);g['notice']=dict(text=wanted,role='status',visible=True,hit=True,rect=B['B']['B']['rect']()) if wanted else None
        p.pop('release',None);p.pop('blockedConfirm',None)
        at=1000+i*2 if guard else (990,995,1001,1003,1005,1015,1017,1019,1021)[i]
        p.update(observedAt=at,observedUntil=at+1)
        c.update(retryAt=deadline if not guard and i in (2,3,4,6) else 0,retryAfter=max(0,deadline-at) if not guard and i in (2,3,4) else 0)
        if guard:
            if i>=2:p['permissionDenied']=V['permission'](True,False)
            if i>=4:p['permissionRestored']=V['permission'](False,True)
            if i>=5:p['blockedConfirm']=copy.deepcopy(D['BLOCKED'])
        elif i>=2:
            p.update(rateIssuedAt=1000,cooldownDeadline=deadline,blockedConfirm=copy.deepcopy(D['BLOCKED']))
            if i>=4:p['reopenedBlockedConfirm']=copy.deepcopy(D['BLOCKED'])
        inv=copy.deepcopy(base['stages'][1]['deleteInventory']) if i else None
        if not guard and i>=2:
            inv=copy.deepcopy(base['stages'][4]['deleteInventory']);inv['retry_at']=deadline;inv['attempt']['retry_at']=deadline
            if i<6:inv['attempt'].update(phase='RateLimited',raw=None)
            else:inv['revision']=4;inv['attempt']['id']='00000000-0000-4000-8000-000000000099'
        r['deleteInventory']=inv;rows.append(r)
    if guard:
        changed=V['REC']['changed_row']()
        event=dict(team='T1',generation=0,frame=dict(type='message',subtype='message_changed',channel=V['CHANNEL'],message=changed),parsed=True,
            witness=dict(kind='Changed',outerChannel=V['CHANNEL'],typedTs=V['TS'],typedChannel=V['CHANNEL'],raw=changed),
            before={k:copy.deepcopy(rows[4]['model'][k]) for k in ('delete','comments')},after={k:copy.deepcopy(rows[5]['model'][k]) for k in ('delete','comments')})
        for row in rows[5:]:row['proof']['sourceChanged']=copy.deepcopy(event)
    base.update(case=case,stages=rows,final=copy.deepcopy(rows[-1]['model']))
    return fixture,base


class GuardTests(unittest.TestCase):
    def check(self,f,r):V['validator'](r['case'])(f,r,DIRECTORY,png=False)
    def reject(self,change,i=2,case='own-delete-guards'):
        f,r=sample(case);change(r['stages'][i])
        with self.assertRaises((RuntimeError,ValueError,AssertionError)):self.check(f,r)
    def test_both_cases_both_widths(self):
        for case in V['STAGES']:
            for narrow in (False,True):self.check(*sample(case,narrow))
    def test_exact_runner_registration(self):
        runner=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
        for case,stages in V['STAGES'].items():
            self.assertEqual(runner['case_contract'](case)[0],stages)
            f,r=sample(case);runner['case_contract'](case)[1](f,r,DIRECTORY,png=False)
    def test_explicit_retry_restamps_only_original_expired_entry_deadline(self):
        case='own-delete-rate-limit'
        self.reject(lambda r:r['model']['comments']['comments'].update(retryAt=0),6,case)
        self.reject(lambda r:r['model']['comments']['comments'].update(retryAt=1016),6,case)
        self.reject(lambda r:r['model']['comments']['comments'].update(retryAfter=1),6,case)
        self.reject(lambda r:r['model']['comments']['comments'].update(retryAt=1015),7,case)

    def test_retained_rate_normal_b_full_receipt(self):
        path=Path(__file__).resolve().parents[1]/'tmp/list-comment-own-delete-guards-20261019/rate-normal-b/slack-list/slack-list.json'
        if not path.is_file():self.skipTest('Retained local offline native receipt is absent')
        wrapper=json.loads(path.read_text(encoding='utf-8'))
        self.assertFalse(wrapper['ok'])
        native=wrapper['terminalReceipts'][0]
        V['validator']('own-delete-rate-limit')(wrapper['fixture'],native,Path(wrapper['readyDirectory']),dimensions=wrapper['expectedPngDimensions'])
        self.assertEqual(native['stages'][5]['model']['comments']['comments']['retryAt'],0)
        self.assertEqual(native['stages'][6]['model']['comments']['comments']['retryAt'],native['stages'][2]['proof']['cooldownDeadline'])
        self.assertEqual(native['stages'][6]['model']['comments']['comments']['retryAfter'],0)

    def test_positive_permission_cannot_replace_observed_denial(self):
        self.reject(lambda r:r['model']['fixture']['policy'][3]['response'].update(allowed=['DELETE_OWN_MESSAGE']))
        self.reject(lambda r:r['proof']['permissionDenied'].update(user='UOTHER'))
        self.reject(lambda r:r['model']['fixture']['policy'].pop())
    def test_refusal_cannot_dispatch_or_create_attempt(self):
        for key in ('prepared','deletes','records','responses'):self.reject(lambda r,k=key:r['model']['fixture'][k].append({}))
        self.reject(lambda r:r['deleteInventory'].update(revision=1))
    def test_original_source_witness_is_parsed_and_scoped(self):
        self.reject(lambda r:r['proof']['sourceChanged']['frame'].update(channel='COTHER'),5)
        self.reject(lambda r:r['proof']['sourceChanged'].update(parsed=False),5)
        self.reject(lambda r:r['proof']['sourceChanged']['witness'].update(raw={}),5)
    def test_stale_source_refuses_confirm_but_retains_original_preview(self):
        self.reject(lambda r:r['model']['delete']['active'].update(sourceCurrent=True),5)
        self.reject(lambda r:r['geometry']['controls'][-1].update(disabled=False),5)
        self.reject(lambda r:r['geometry']['panel']['preview'].update(bold=[]),5)
        self.reject(lambda r:r['model']['delete']['active']['record']['message'].update(text=V['REC']['SOCKET_TEXT']),5)
    def test_socket_changes_typed_body_not_raw_http_or_reply(self):
        self.reject(lambda r:r['model']['comments']['comments']['page']['messages'][-1].update(text='Changed'),5)
        self.reject(lambda r:r['model']['comments']['comments']['pageMessages'][-1].update(text='Not observed edit'),5)
        self.reject(lambda r:r['journal'].update(blocks=[]),5)
    def test_original_rich_reply_same_node_until_actual_reopen(self):
        self.reject(lambda r:r['geometry']['reply'].update(sameNode=False),3)
        self.reject(lambda r:r['geometry']['reply'].update(inert=False),1)
        self.reject(lambda r:r['proof'].update(runtimeForgotten=False),7)
    def test_positive_typed429_is_issued_but_not_ack_or_unknown(self):
        case='own-delete-rate-limit'
        self.reject(lambda r:r['model']['fixture']['responses'][0].update(retryAfter=2),case=case)
        self.reject(lambda r:r['model']['fixture']['deletes'].clear(),case=case)
        self.reject(lambda r:r['deleteInventory']['attempt'].update(phase='Uncertain'),case=case)
        self.reject(lambda r:r['deleteInventory']['attempt'].update(raw=D['ACK']),case=case)
    def test_cooldown_clock_bounds_and_deadline_identity(self):
        case='own-delete-rate-limit'
        self.reject(lambda r:r['proof'].update(cooldownDeadline=1016),4,case)
        self.reject(lambda r:r['proof'].update(observedUntil=1015),4,case)
        self.reject(lambda r:r['proof'].update(observedAt=1014),5,case)
        self.reject(lambda r:r['proof'].update(rateIssuedAt=1001),2,case)
    def test_cooldown_cancel_keeps_reply_visible_but_send_disabled(self):
        case='own-delete-rate-limit'
        self.reject(lambda r:r['geometry']['controls'][-1].update(disabled=False),3,case)
        self.reject(lambda r:r['geometry']['reply'].update(hidden=True),3,case)
        self.reject(lambda r:r['geometry']['text'].update(ok=False),3,case)
    def test_expiry_alone_has_no_read_dispatch_or_journal_rewrite(self):
        case='own-delete-rate-limit'
        self.reject(lambda r:r['model']['fixture']['deletes'].append(D['wire']()),5,case)
        self.reject(lambda r:r['deleteInventory'].update(revision=999),5,case)
        self.reject(lambda r:r['deleteInventory'].update(retry_at=0),5,case)
        self.reject(lambda r:r['model']['fixture']['policy'].extend(D['policy_cycle']()),4,case)
    def test_actual_timer_must_retire_entry_without_rewriting_durable_deadline(self):
        case='own-delete-rate-limit'
        self.reject(lambda r:r['model']['comments']['comments'].update(retryAt=1015),5,case)
        self.reject(lambda r:r['model']['comments']['comments'].update(retryAt=0),4,case)
        self.reject(lambda r:r['model']['comments']['comments'].update(retryAfter=14),4,case)
    def test_denial_leaves_original_positive_permission_receipt_immutable(self):
        self.reject(lambda r:r['model']['fixture']['policy'][1]['response'].update(allowed=[]),2)
        self.reject(lambda r:r['proof']['permissionRestored'].update(allowedBefore=True),4)

    def test_explicit_retry_new_uuid_original_deadline_and_exact_ack(self):
        case='own-delete-rate-limit'
        self.reject(lambda r:r['deleteInventory']['attempt'].update(id='00000000-0000-4000-8000-000000000001'),6,case)
        self.reject(lambda r:r['deleteInventory']['attempt'].update(retry_at=0),6,case)
        self.reject(lambda r:r['deleteInventory']['attempt']['raw'].update(ts=V['READ']['ROOT_TS']),6,case)
    def test_rate_notice_retained_after_reopen_and_expiry(self):
        for i in (2,4,5):self.reject(lambda r:r['geometry'].update(notice=None),i,'own-delete-rate-limit')
    def test_final_readback_and_reopen_preserve_exact_attempt(self):
        case='own-delete-rate-limit'
        self.reject(lambda r:r['model']['comments']['comments']['page'].pop('deleted'),7,case)
        self.reject(lambda r:r['deleteInventory']['attempt'].update(id='00000000-0000-4000-8000-000000000088'),8,case)
    def test_physical_png_dimensions_remain_independent(self):
        f,r=sample();old=V['H']['png_signature'];calls=[];V['H']['png_signature']=lambda p,d:calls.append(d)
        try:V['validator'](r['case'])(f,r,DIRECTORY,dimensions=(1942,1256))
        finally:V['H']['png_signature']=old
        self.assertEqual(calls,[(1942,1256)]*8)


if __name__=='__main__':unittest.main()
