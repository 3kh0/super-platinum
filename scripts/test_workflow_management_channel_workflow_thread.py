import copy
import unittest
import workflow_management_channel_workflow_thread as T

def model():
    return dict(root=T.ROOT,activeRoot=T.ROOT,threadGeneration=3,scope='original-thread',loading=False,
        threadComposer=dict(text=T.DRAFT,selection=dict(start=0,end=0),reply_broadcast=False),
        draftKey=copy.deepcopy(T.KEY),activeDraftKey=copy.deepcopy(T.KEY),attachments=0,
        historyLoaded=True,threadLoaded=True,history=[copy.deepcopy(T.PARENT)],
        thread=[copy.deepcopy(T.PARENT),copy.deepcopy(T.RESPONSE)],historyPending=[],threadPending=[],
        renderedRows=[dict(id=ts,ts=ts,pending=False) for ts in (T.ROOT,T.REPLY)])

class ThreadCustody(unittest.TestCase):
    def setUp(self):
        self.original=model()
        self.original['storedDraft']=dict(composer=copy.deepcopy(self.original['threadComposer']),attachments=[])
    def reject(self,change,active=True):
        m=copy.deepcopy(self.original);change(m)
        with self.assertRaises((RuntimeError,AssertionError)):
            T.model_proof(m,active,True,self.original)
    def test_typed_original_draft(self):T.model_proof(self.original,True,True,self.original)
    def test_lost_draft(self):self.reject(lambda m:m.update(storedDraft=None))
    def test_wrong_thread(self):self.reject(lambda m:m.update(activeRoot='another'))
    def test_reopened_thread(self):self.reject(lambda m:m.update(threadGeneration=4))
    def test_send_queued(self):self.reject(lambda m:m['threadPending'].append('new-send'))
    def test_reply_mutated(self):self.reject(lambda m:m['thread'][1].update(text='replacement'))
    def test_broadcast_enabled(self):self.reject(lambda m:m['threadComposer'].update(reply_broadcast=True))
    def test_stored_draft_wrong_destination(self):self.reject(lambda m:m['draftKey'].update(channel='other'))
    def test_return_retains_exact_draft(self):
        m=copy.deepcopy(self.original)
        m.update(activeRoot=None,activeDraftKey=None,renderedRows=[],threadComposer=dict(text='',selection=dict(start=0,end=0),reply_broadcast=False))
        T.model_proof(m,False,True,self.original)
    def test_input_middle_of_unicode_byte(self):
        offset=len('Unsent workflow review '.encode())+1
        self.reject(lambda m:m['threadComposer']['selection'].update(start=offset,end=offset))
    def test_unexposed_activation(self):
        action=dict(ok=True,kind='dom-click',nodeId=1,label='Workflows',rect=dict(left=100,top=100,right=200,bottom=120,width=100,height=20),hitPoint=dict(x=90,y=110))
        with self.assertRaises((RuntimeError,AssertionError)):T.action_proof(action,'dom-click',[800,600],'Workflows')
    def test_foreign_input_node(self):
        action=dict(ok=True,kind='dom-input',nodeId=2,rect=dict(left=100,top=100,right=200,bottom=120,width=100,height=20),hitPoint=dict(x=150,y=110))
        with self.assertRaises((RuntimeError,AssertionError)):T.action_proof(action,'dom-input',[800,600],node=3)

if __name__=='__main__':unittest.main()
