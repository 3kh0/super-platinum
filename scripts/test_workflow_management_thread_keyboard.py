"""Request identity and native keyboard witness regressions; no OS input."""
import copy
import unittest
import workflow_management_thread_keyboard as K
import workflow_management_channel_workflow_thread as T
from windows_keyboard import input_batches, pending_releases, SCANCODE, KEYUP


class ThreadKeyboard(unittest.TestCase):
    def setUp(self):
        self.expected=dict(version=1,fixture='workflow-editor-narrow',nonce='closed-test',pid=123)
        self.request=dict(self.expected,nodeId=45,key='Enter',label='Workflows')
        self.action=dict(ok=True,kind='trusted-keyboard',key='Enter',label='Workflows',nodeId=45,
            rect=dict(left=300,right=400,top=50,bottom=80,width=100,height=30),hitPoint=None,
            keydown=dict(trusted=True,key='Enter',code='Enter',repeat=False,targetNodeId=45,focused=True,connected=True,enabled=True),
            click=dict(trusted=True,targetNodeId=45,detail=0,connected=True,enabled=True))
        self.receipt=dict(stages=[dict(stage='browse',proof=dict(exercise=dict(threadSetup=dict(tabActivation=self.action))))])

    def test_one_owned_request_matches_original_witness(self):
        K.validate_requests([self.request],self.expected,self.receipt,True)
        T.action_proof(self.action,'trusted-keyboard',[668,780],'Workflows',45)

    def test_reject_unrequested_or_wrong_identity(self):
        with self.assertRaises(RuntimeError):K.admit(self.request,self.expected,False)
        for key,value in (('pid',124),('nonce','foreign'),('fixture','workflow-editor'),('version',True),('nodeId',True),('key','Space'),('label','Send')):
            bad=dict(self.request);bad[key]=value
            with self.subTest(key=key),self.assertRaises(RuntimeError):K.admit(bad,self.expected,True)

    def test_request_is_once_and_requires_actual_witness(self):
        for requests in ([],[self.request,self.request],[dict(self.request,nodeId=46)]):
            with self.assertRaises(RuntimeError):K.validate_requests(requests,self.expected,self.receipt,True)
        self.action['kind']='dom-click'
        with self.assertRaises(RuntimeError):K.validate_requests([self.request],self.expected,self.receipt,True)

    def test_untrusted_foreign_repeated_or_disabled_event_rejected(self):
        for event,key,value in (('keydown','trusted',False),('keydown','targetNodeId',46),('keydown','repeat',True),('keydown','focused',False),('keydown','key','Space'),('click','trusted',False),('click','targetNodeId',46),('click','detail',1),('click','enabled',False)):
            bad=copy.deepcopy(self.action);bad[event][key]=value
            with self.subTest(event=event,key=key),self.assertRaises(RuntimeError):T.action_proof(bad,'trusted-keyboard',[668,780],'Workflows',45)

    def test_balanced_enter_and_owned_partial_release(self):
        batch=input_batches('enter')[0]
        self.assertEqual(batch,[(0x1C,SCANCODE),(0x1C,SCANCODE|KEYUP)])
        self.assertEqual(pending_releases(batch,1),[(0x1C,SCANCODE|KEYUP)])
        self.assertEqual(pending_releases(batch,2),[])

    def test_completion_requires_clean_single_batch(self):
        good=dict(batches=[dict(action='enter',batch=0,requested=2,inserted=2)],foregroundRestoration=dict(observerClean=True,interruption=None,heldModifiers=[]))
        K.completed(good)
        for change in (lambda d:d['batches'][0].update(inserted=1),lambda d:d['batches'].append(d['batches'][0].copy()),lambda d:d['foregroundRestoration'].update(observerClean=False),lambda d:d['foregroundRestoration'].update(interruption='foreign input'),lambda d:d['foregroundRestoration'].update(heldModifiers=[16])):
            bad=copy.deepcopy(good);change(bad)
            with self.assertRaises(RuntimeError):K.completed(bad)

    def test_share_exact_two_sequential_named_enters(self):
        requests=[dict(self.request,label=label,nodeId=45+i) for i,label in enumerate(('Finish Up','Messages'))]
        actions=[]
        for request in requests:
            a=copy.deepcopy(self.action);a.update(label=request['label'],nodeId=request['nodeId'])
            for key in ('keydown','click'):a[key]['targetNodeId']=request['nodeId']
            actions.append(a)
        receipt=dict(shareDraftCustody=True,stages=[dict(stage='finish',proof=dict(exercise=dict(draftOpen=actions[0])))],
                     shareDrafts=dict(returned=dict(messagesActivation=actions[1])))
        K.validate_requests(requests,self.expected,receipt,False,share_drafts=True)
        for bad in (requests[::-1],[requests[0],requests[0]],[dict(requests[0],label='Configuration')]):
            with self.assertRaises(RuntimeError):K.admit_requests(bad,self.expected,False,share_drafts=True)
        for index in (0,1):
            bad=copy.deepcopy(receipt)
            action=bad['stages'][0]['proof']['exercise']['draftOpen'] if index==0 else bad['shareDrafts']['returned']['messagesActivation']
            action['keydown']['trusted']=False
            with self.assertRaises(RuntimeError):K.validate_requests(requests,self.expected,bad,False,share_drafts=True)
        with self.assertRaises(RuntimeError):K.validate_requests(requests[:1],self.expected,receipt,False,share_drafts=True)
        data=dict(batches=[dict(action='enter',batch=0,requested=2,inserted=2)]*2,
                  foregroundRestoration=dict(observerClean=True,interruption=None,heldModifiers=[]))
        K.completed(data,requests,share_drafts=True)
        with self.assertRaises(RuntimeError):K.completed(data)
        with self.assertRaises(RuntimeError):K.completed(data,requests[:1],share_drafts=True)

    def test_share_keyboard_does_not_expand_other_modes(self):
        request=dict(self.request,label='Finish Up')
        for enabled,share in ((False,False),(True,False),(True,True)):
            with self.assertRaises(RuntimeError):K.admit(request,self.expected,enabled,share_drafts=share)
        with self.assertRaises(RuntimeError):K.admit(request,dict(self.expected,fixture='workflow-editor'),False,share_drafts=True)
        with self.assertRaises(RuntimeError):K.admit(self.request,self.expected,False,share_drafts=True)


if __name__=='__main__':unittest.main()
