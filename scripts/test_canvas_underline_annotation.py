"""Portable adversarial receipt checks; no synthetic native acceptance claim."""
import copy,json,unittest
import canvas_underline_annotation as O
from test_canvas_heading import f,b64

RTML='<u><b>A<annotation id="old">&amp;\U0001f600</annotation>B</b></u><br><i><u>New &amp;\U0001f98a</u></i> tail'
def section():return {'id':'paragraph','raw':b64(f(1,'paragraph')+f(2,1)+f(12,f(1,f(1,RTML)+f(2,'en'))))}
def form(path,fields):return {'method':'POST','url':'https://fixture.slack.com'+path,'fields':list(map(list,fields.items()))}
def anchor():return form('/canvas/-/edit-document',dict(thread='thread',document='document',retry_count='0',secret_path='',session='fixture-session',sequence='1',nav_action_id='null',search_session_id='null',data_binary=b64(f(1,f(1,'paragraph'))),title='Fixture'))
def snippet():return {'mrkdwn':'\n_New &\U0001f98a_','rich_text':json.dumps({'blocks':[{'type':'rich_text','elements':[{'type':'rich_text_section','elements':[{'type':'text','text':'\n'},{'type':'text','text':'New &\U0001f98a','style':{'underline':True,'italic':True}}]}]}]})}
def sample():
    old={'document_comment':{'thread_id':'old'}};new={'document_comment':{'thread_id':'new'}}
    a=anchor();q=form('/api/conversations.replies',dict(channel='Cfixture',ts='Qpc:t:new',content_snippet=json.dumps(snippet())))
    seed={'seedRequests':[a,q],'roots':[old],'prepared':[],'requests':[],'acks':[],'sections':[section()],'sourceChanges':0}
    stages=[]
    for i,name in enumerate(O.STAGES):
        p=copy.deepcopy(seed);p['prepared']=copy.deepcopy([a,q,a][:O.WRITES[i]]);p['requests']=copy.deepcopy(p['prepared']);p['roots']=[old,new][:O.ROOTS[i]];p['acks']=['fixture-ack']*(0 if i<5 else 1 if i<8 else 2);p['sourceChanges']=int(i>=3)
        g={'ok':True,'source':{'text':O.TEXT,'painted':O.PAINT},'text':'Existing comment. The original annotation control is present. The original annotation and its delivery state are retained locally.','alerts':[O.STALE] if i==3 else [O.UNKNOWN] if i==8 else [],'controls':[{'label':label,'disabled':disabled,'id':''} for label,disabled in [('Close annotation review',True),('Keep outcome unknown and remove local lock',True),('Refresh current annotation state',False)]] if i>=9 else [],'preview':{'text':O.TEXT,'painted':O.PAINT} if i>=9 else None}
        j={'pending':{'stage':'AnchorStarted','receipt':{'id':'original'}} if i>=8 else None,'history':[{'id':'accepted','status':'root_accepted'},{'id':'anchor','status':'anchor_acknowledged'}] if i>=5 else []}
        stages.append({'stage':name,'ordinal':i,'ok':True,'noSession':True,'noTransport':True,'fixture':p,'geometry':g,'journal':j})
    selections=[{'ok':True,'start':s,'end':e,'text':t,'source':O.TEXT} for s,e,t in [(1,4,'&\U0001f600'),(5,13,'\nNew &\U0001f98a'),(5,13,'\nNew &\U0001f98a'),(14,18,'tail')]]
    return copy.deepcopy({'initial':seed,'stages':stages,'fixture':stages[-1]['fixture'],'selections':selections})
class Tests(unittest.TestCase):
    def refuse(self,mutate):
        value=sample();mutate(value)
        with self.assertRaises(RuntimeError):O.validate(value)
    def test_complete(self):O.validate(sample())
    def test_wire_canonical_underline(self):
        self.assertEqual(O.styles(RTML)[0],O.PAINT)
        self.assertEqual(''.join(x[0] for x in O.PAINT),O.TEXT)
    def test_lost_canonical_underline(self):
        self.refuse(lambda r:r['stages'][6]['fixture']['sections'][0].__setitem__('raw',b64(f(1,'paragraph')+f(12,f(1,f(1,RTML.replace('<u>','').replace('</u>','')))))))
    def test_reader_paint(self):self.refuse(lambda r:r['stages'][0]['geometry']['source']['painted'][0].__setitem__(1,False))
    def test_actual_preview_paint(self):self.refuse(lambda r:r['stages'][9]['geometry']['preview']['painted'][6].__setitem__(1,False))
    def test_nested_rich_snippet(self):
        O.snippet(snippet());s=snippet();s['rich_text']=s['rich_text'].replace('true','false')
        with self.assertRaises(RuntimeError):O.snippet(s)
    def test_markdown_cannot_fake_underline(self):
        s=snippet();s['mrkdwn']='<u>New &\U0001f98a</u>'
        with self.assertRaises(RuntimeError):O.snippet(s)
    def test_lost_existing_thread(self):self.refuse(lambda r:r['stages'][6]['fixture']['roots'][0].__setitem__('document_comment',{'thread_id':'other'}))
    def test_duplicate_write(self):self.refuse(lambda r:r['stages'][8]['fixture']['prepared'].append(anchor()))
    def test_stale_warning(self):self.refuse(lambda r:r['stages'][3]['geometry'].__setitem__('alerts',[]))
    def test_unknown_warning(self):self.refuse(lambda r:r['stages'][8]['geometry'].__setitem__('alerts',[]))
    def test_unknown_lock(self):self.refuse(lambda r:r['stages'][10]['journal'].__setitem__('pending',None))
    def test_unknown_identity(self):self.refuse(lambda r:r['stages'][10]['journal']['pending']['receipt'].__setitem__('id','new'))
    def test_unknown_replay_control(self):self.refuse(lambda r:r['stages'][9]['geometry']['controls'].append({'id':'canvas-comment-create','disabled':False}))
    def test_source_change_count(self):self.refuse(lambda r:r['stages'][3]['fixture'].__setitem__('sourceChanges',0))
    def test_native_surrogate_offset(self):self.refuse(lambda r:r['selections'][0].__setitem__('end',3))
    def test_native_text(self):self.refuse(lambda r:r['selections'][1].__setitem__('text','New'))
    def test_live_authority(self):self.refuse(lambda r:r['stages'][5].__setitem__('noTransport',False))
    def test_source_origin_and_duplicate_fields(self):
        x=anchor();x['url']='https://other.invalid/canvas/-/edit-document'
        with self.assertRaises(RuntimeError):O.form(x)
        x=anchor();x['fields'].append(x['fields'][0])
        with self.assertRaises(RuntimeError):O.form(x)
    def test_stage_order(self):self.refuse(lambda r:r['stages'].reverse())
    def test_history_requires_both_original_statuses(self):
        self.refuse(lambda r:r['stages'][5]['journal']['history'].pop())
    def test_history_stable_after_confirmed(self):
        self.refuse(lambda r:r['stages'][10]['journal']['history'][0].__setitem__('id','replacement'))
    def test_history_order_can_differ_between_runs_only(self):
        r=sample()
        for s in r['stages'][5:]:s['journal']['history'].reverse()
        O.validate(r)
    def test_review_clears_transient_alert(self):
        self.refuse(lambda r:r['stages'][9]['geometry'].__setitem__('alerts',[O.UNKNOWN]))
    def test_review_retains_explicit_delivery_guidance(self):
        self.refuse(lambda r:r['stages'][9]['geometry'].__setitem__('text','The original annotation control is present.'))
    def test_review_cannot_unlock_close(self):
        self.refuse(lambda r:r['stages'][9]['geometry']['controls'][0].__setitem__('disabled',False))
    def test_review_cannot_unlock_forget(self):
        self.refuse(lambda r:r['stages'][9]['geometry']['controls'][1].__setitem__('disabled',False))
    def test_review_requires_explicit_refresh(self):
        self.refuse(lambda r:r['stages'][9]['geometry']['controls'][2].__setitem__('disabled',True))
if __name__=='__main__':unittest.main()
