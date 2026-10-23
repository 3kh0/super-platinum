import copy
import json
import unittest
from urllib.parse import urlencode
import workflow_management_share as V

CID='4d11c69e-0dc9-4f95-825f-3e0292717e85'
TS='1781524998.000001'
def request():
    q=dict(_x_id='1'*32,_x_csid='2'*32,fp='3'*32,_x_desktop_ia='true',_x_foreground='true',_x_frontend_build_type='current',_x_gantry='true',_x_num_retries='0',_x_version_ts='capture-required',slack_route='default')
    return dict(method='chat.postMessage',url='https://example.test/api/chat.postMessage?'+urlencode(q),tokenPresent=True,
        fields=[['channel',V.A.DEST],['ts',TS],['type','message'],['blocks',json.dumps(V.blocks())],['unfurl',json.dumps([{'url':V.URL}])],['client_msg_id',CID],['include_channel_perm_error','true'],['_x_reason','webapp_message_send']])
def sample(case,stage):
    pre=stage in V.A.PREFIX or stage in ('share-open','share-edited');sent=not pre and case!='destination-failure';held=case=='held' and stage in ('share-held','share-cancelled')
    n=(3 if case=='lost-response' else 1) if sent else 0
    reads=[] if pre else [dict(serial=1,method='conversations.info',body=[['channel',V.A.DEST],['include_num_members','false']],endpoint='https://example.test/api/conversations.info',retrySafe=True,owner=copy.deepcopy(V.OWNER),result={'error':'offline'} if case=='destination-failure' else {'raw':dict(ok=True,channel=V.A.CHANNELS[V.A.DEST],opaque_receipt='original-share-channel')})]
    raw=V.raw_reply(case,CID)
    requests=[dict(serial=i,request=request(),owner=copy.deepcopy(V.OWNER)) for i in range(1,n+1)]
    responses=[dict(serial=i,error='transport',applied=True) if case=='lost-response' else dict(serial=i,raw=raw,applied=case!='rejected') for i in range(1,n+1)]
    attempts=[] if held else [dict(number=i,status=None if case=='lost-response' else 200,outcome='unknown' if case=='lost-response' else 'rejected' if case=='rejected' else 'acknowledged',raw=None if case=='lost-response' else raw,responseText=None if case=='lost-response' else json.dumps(raw),retryAfterMillis=None,mayHaveApplied=case!='rejected') for i in range(1,n+1)]
    v=dict(case=case,reads=reads,requests=requests,responses=responses,attempts=attempts,settlements=[] if held else [dict(serial=i,returned=True) for i in range(1,n+1)],releases=[dict(serial=1,receiverRetired=False,delivered=True)] if case=='held' and sent and not held else [],held=[1] if held else [],applied=[V.raw_reply('success',CID)['message']] if sent and case!='rejected' else [],started=sent,networkCalls=0,retrySchedule=dict(kind='closed-fixture-source-minimum',delayMillis=1000,maxAttempts=3))
    return {'confirmationAdd':{'share':v}}

class ShareWire(unittest.TestCase):
    def valid(self,case='success',stage='share-settled'):
        s=sample(case,stage);V.ledger(case,stage,s);return s
    def reject(self,mutation,case='success',stage='share-settled'):
        s=self.valid(case,stage);mutation(s['confirmationAdd']['share'])
        with self.assertRaises((RuntimeError,ValueError,TypeError)):V.ledger(case,stage,s)
    def test_all_phase_ledgers(self):
        for case,stages in V.STAGES.items():
            for stage in stages:
                with self.subTest(case=case,stage=stage):self.valid(case,stage)
    def test_no_send_stages_do_not_dispatch(self):self.reject(lambda v:v['requests'].append({}),'success','share-edited')
    def test_wrong_channel(self):self.reject(lambda v:v['requests'][0]['request']['fields'][0].__setitem__(1,V.A.PRIVATE))
    def test_fabricated_plain_text_wire(self):self.reject(lambda v:v['requests'][0]['request']['fields'].append(['text','invented']))
    def test_wrong_reason(self):self.reject(lambda v:v['requests'][0]['request']['fields'][-1].__setitem__(1,'wrong'))
    def test_wrong_block_url(self):
        self.reject(lambda v:v['requests'][0]['request']['fields'][3].__setitem__(1,json.dumps([{'type':'rich_text','elements':[]}])) )
    def test_wrong_singular_unfurl(self):self.reject(lambda v:v['requests'][0]['request']['fields'][4].__setitem__(0,'unfurls'))
    def test_new_uuid_on_retry(self):self.reject(lambda v:v['requests'][1]['request']['fields'][5].__setitem__(1,'4d11c69e-0dc9-4f95-825f-3e0292717e86'),'lost-response')
    def test_lost_reply_fake_raw(self):self.reject(lambda v:v['responses'][0].update(raw={'ok':True}),'lost-response')
    def test_lost_ack_fabricated(self):self.reject(lambda v:v['attempts'][0].update(outcome='acknowledged'),'lost-response')
    def test_retry_loses_original_application(self):self.reject(lambda v:v.update(applied=[]),'lost-response')
    def test_rejected_application(self):self.reject(lambda v:v.update(applied=[V.raw_reply('success',CID)['message']]),'rejected')
    def test_response_drops_opaque(self):self.reject(lambda v:v['responses'][0]['raw'].pop('opaque_receipt'))
    def test_foreign_account_owner(self):self.reject(lambda v:v['requests'][0]['owner'].update(epoch=999))
    def test_dispatch_serial_boolean(self):self.reject(lambda v:v['requests'][0].update(serial=True))
    def test_wrong_token_presence(self):self.reject(lambda v:v['requests'][0]['request'].update(tokenPresent=False))
    def test_origin_redirect(self):self.reject(lambda v:v['requests'][0]['request'].update(url=v['requests'][0]['request']['url'].replace('example.test','other.test')))
    def test_held_cannot_pretend_returned(self):self.reject(lambda v:v['settlements'].append(dict(serial=1,returned=True)),'held','share-held')
    def test_release_actual_receiver(self):self.reject(lambda v:v['releases'][0].update(delivered=False),'held','late-settled')
    def test_destination_failure_no_api_ack(self):self.reject(lambda v:v['reads'][0].update(result={'raw':{'ok':True}}),'destination-failure','share-unavailable')
    def test_duplicate_destination_validation(self):
        s=self.valid();openrow={'proof':{'server':sample('success','share-open')}}
        s['confirmationAdd']['share']['reads'].append(copy.deepcopy(s['confirmationAdd']['share']['reads'][0]));s['confirmationAdd']['share']['reads'][1]['serial']=2
        with self.assertRaises(RuntimeError):V.ledger('success','share-settled',s,openrow)
    def test_original_blocks_link_only_field(self):
        fields=dict(request()['fields']);self.assertNotIn('text',fields);self.assertNotIn('unfurl_links',fields)
    def test_changed_unrelated_draft(self):
        before={'model':{'shareCustody':dict(composer={'text':'original'},threadComposer={'text':'thread'},composerAttachments=0,threadAttachments=0,threadRoot=None,threadGeneration=1,drafts=[])}}
        after=copy.deepcopy(before);V.custody(before,after);after['model']['shareCustody']['composer']['text']='consumed'
        with self.assertRaises(RuntimeError):V.custody(before,after)
    def test_queue_failure_not_ack(self):
        raw=V.raw_reply('success',CID)['message'];m=dict(raw,ts=TS,channel=V.A.DEST)
        row={'model':{'shareCustody':dict(messages=[m],pending=[TS],delivery={TS:'Failed'})}}
        V.message_proof('rejected','share-settled',row,CID,TS)
        row['model']['shareCustody']['delivery']={}
        with self.assertRaises(RuntimeError):V.message_proof('rejected','share-settled',row,CID,TS)
    def test_operation_raw_mismatch(self):
        server=self.valid();v=server['confirmationAdd']['share'];q={'operation':dict(outcome='acknowledged',finished=True,queued=True,request=v['requests'][0]['request'],attempts=v['attempts'],message=dict(channel=V.A.DEST,clientMsgId=CID,localTs=TS,blocks=V.blocks(),url=V.URL,text=V.MESSAGE+'\n'+V.URL),raw=V.raw_reply('success',CID))}
        V.operation_proof('success','share-settled',q,server);q['operation']['raw']=None
        with self.assertRaises(RuntimeError):V.operation_proof('success','share-settled',q,server)
    def test_scope(self):self.assertEqual([len(v) for v in V.STAGES.values()],[11,11,11,13,11])

if __name__=='__main__':unittest.main()
