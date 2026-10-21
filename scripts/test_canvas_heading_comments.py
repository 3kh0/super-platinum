"""Adversarial portable evidence tests; synthetic receipts do not claim native execution."""
import copy,json,unittest
import canvas_heading_comments as O
from test_canvas_heading import f,b64
from test_canvas_underline_annotation import form,anchor

def rtml(old=False,new=None):
    first='<b>A</b><annotation id="old"><b>&amp;</b><u>\U0001f600</u></annotation><u>B</u>' if old else '<b>A&amp;</b><u>\U0001f600B</u>'
    middle='<i>New</i> <u>&amp;\u4e2d\U0001f98a</u>'
    if new:middle='<annotation id="'+new+'">'+middle+'</annotation>'
    return first+' '+middle+' tail'
def attributes(id):return f(4,0)+f(13,0)+f(29,'retained-'+id)+f(177,'heading-attributes-opaque')

def section(id,level,body,seq=1000,content_id=False):
    return {'id':id,'raw':b64(f(1,id)+f(2,seq)+f(6,'thread')+f(7,'document')+f(8,id)+f(10,level)+f(12,f(1,f(1,body)+f(2,'ja')))+f(177,'heading-comment-opaque')+(f(16,attributes(id)) if content_id else b'')+(generated() if seq>=2000 else b''))}
def snippets():
    return {'mrkdwn':'_New_ &\u4e2d\U0001f98a','rich_text':json.dumps({'blocks':[{'type':'rich_text','elements':[{'type':'rich_text_section','elements':[{'type':'text','text':'New','style':{'italic':True}},{'type':'text','text':' '},{'type':'text','text':'&\u4e2d\U0001f98a','style':{'underline':True}}]}]}]})}
def generated():return f(9,0)+f(13,b'')+f(19,1)+f(20,1)+f(33,'fixture-session-0')
def requests(index,content_id=False):
    a=anchor();fields=dict(a['fields']);fields['data_binary']=b64(f(1,f(1,O.IDS[index])+generated()+(f(16,attributes(O.IDS[index])) if content_id else b'')));a['fields']=list(map(list,fields.items()))
    q=form('/api/conversations.replies',{'channel':'Cfixture','ts':'Qpc:thread:new'+str(index),'content_snippet':json.dumps(snippets())})
    return [a,q]
def sample(content_id=False):
    root=lambda id:{'document_comment':{'thread_id':id}}
    original=[section(id,level,rtml(id==O.IDS[0]),content_id=content_id) for id,level in zip(O.IDS,O.LEVELS)]
    original.append({'id':'old','raw':b64(f(1,'old')+f(9,9))})
    seed={'sections':original,'roots':[root('old')],'replies':[{'text':'Existing comment'}],'seedRequests':requests(0,content_id),'requests':[],'prepared':[],'acks':[],'sourceChanges':0,'document':b64(f(1,'document')),'thread':b64(f(1,'thread')+f(23,5)),'file':{'access':'comment'}}
    stages=[];acks=[]
    for n,name in enumerate(O.STAGES):
        p=copy.deepcopy(seed);count=O.WRITES[n]//2;p['sourceChanges']=int(n>=6)
        p['roots'] += [root('new'+str(i)) for i in range(count)]
        for i in range(3):
            if i<count:p['sections'][i]=section(O.IDS[i],O.LEVELS[i],rtml(i==0,'new'+str(i)),2000+int(i==1),content_id)
            elif i==1 and n>=6:p['sections'][i]=section(O.IDS[i],O.LEVELS[i],rtml(),1001,content_id)
        p['sections'] += [{'id':'new'+str(i),'raw':b64(f(1,'new'+str(i))+f(9,9))} for i in range(count)]
        if len(acks)<count:
            i=count-1;raw=__import__('base64').b64decode(p['sections'][i]['raw']);control=__import__('base64').b64decode(p['sections'][-1]['raw'])
            acks.append(b64(f(1,1)+f(2,f(1,raw)+f(1,control))))
        p['acks']=list(acks);p['prepared']=sum([requests(i,content_id) for i in range(count)],[]);p['requests']=copy.deepcopy(p['prepared'])
        headings=[{'id':id,'tag':'H'+str(level),'text':O.TEXT,'painted':O.DOM_PAINT,'marks':O.PAINT,'inView':True,'controls':O.shape(O.content(O.rows(p)[id][1]))[1]} for id,level in zip(O.IDS,O.LEVELS)]
        critical=[{'text':O.SELECTED,'inView':True},{'text':'Create comment thread','inView':True}]+([{'text':O.STALE,'inView':True}] if n==6 else [])
        g={'ok':True,'controls':[{'label':'Open created thread','inView':True,'disabled':False}] if name.endswith('-created') else [],'critical':critical,'headings':headings,'focusTarget':{'text':'Open created thread' if name.endswith('-created') else O.TEXT,'inView':True},'text':'Existing comment','alerts':[O.STALE] if n==6 else []}
        history=[{'id':str(i)+status,'status':status} for i in range(count) for status in ('anchor_acknowledged','root_accepted')]
        stages.append({'stage':name,'ordinal':n,'ok':True,'noSession':True,'noTransport':True,'fixture':p,'geometry':g,'journal':{'pending':None,'history':history}})
    selections=[{'ok':True,'id':id,'tag':'H'+str(level),'start':start,'end':end,'text':value,'source':O.TEXT} for id,level,start,end,value in [(O.IDS[0],1,1,4,'&\U0001f600'),(O.IDS[0],1,6,14,O.SELECTED),(O.IDS[1],2,6,14,O.SELECTED),(O.IDS[1],2,6,14,O.SELECTED),(O.IDS[2],3,6,14,O.SELECTED)]]
    return copy.deepcopy({'initial':seed,'fixture':stages[-1]['fixture'],'stages':stages,'selections':selections,'stale':{'id':O.IDS[1],'before':stages[5]['fixture']['sections'][1]['raw'],'after':stages[6]['fixture']['sections'][1]['raw'],'sequence':1001}})

class Tests(unittest.TestCase):
    def bad(self,fn):
        r=sample();fn(r)
        with self.assertRaises(RuntimeError):O.validate(r)
    def test_complete(self):O.validate(sample())
    def test_actual_utf16(self):
        s=O.TEXT.encode('utf-16-le');self.assertEqual(s[12:28].decode('utf-16-le'),O.SELECTED)
        self.assertEqual(s[2:8].decode('utf-16-le'),'&\U0001f600')
    def test_foreign_section(self):self.bad(lambda r:r['selections'][1].__setitem__('id',O.IDS[3]))
    def test_surrogate_split(self):self.bad(lambda r:r['selections'][0].__setitem__('end',3))
    def test_wrong_semantic_tag(self):self.bad(lambda r:r['stages'][4]['geometry']['headings'][0].__setitem__('tag','P'))
    def test_explicit_bold_lost_despite_heading_weight(self):self.bad(lambda r:r['stages'][4]['geometry']['headings'][0]['marks'][0].__setitem__(1,False))
    def test_underline_lost(self):self.bad(lambda r:r['stages'][8]['geometry']['headings'][1]['painted'][2].__setitem__(3,False))
    def test_selected_action_clipped(self):self.bad(lambda r:r['stages'][3]['geometry']['focusTarget'].__setitem__('inView',False))
    def test_selected_snippet_clipped(self):self.bad(lambda r:r['stages'][2]['geometry']['critical'][0].__setitem__('inView',False))
    def test_created_result_offscreen(self):self.bad(lambda r:r['stages'][3]['geometry']['controls'][0].__setitem__('inView',False))
    def test_created_result_disabled(self):self.bad(lambda r:r['stages'][3]['geometry']['controls'][0].__setitem__('disabled',True))
    def test_created_result_missing(self):self.bad(lambda r:r['stages'][3]['geometry'].__setitem__('controls',[]))
    def test_scroll_anchor_need_not_be_result(self):
        r=sample();r['stages'][3]['geometry']['focusTarget']['text']='Create comment thread';O.validate(r)
    def test_canonical_heading_clipped(self):self.bad(lambda r:r['stages'][11]['geometry']['headings'][2].__setitem__('inView',False))
    def test_twin_changed(self):self.bad(lambda r:r['stages'][12]['fixture']['sections'][3].__setitem__('raw',section(O.IDS[3],2,'Changed')['raw']))
    def test_opaque_mutation(self):self.bad(lambda r:r['stages'][4]['fixture']['sections'][0].__setitem__('raw',r['stages'][4]['fixture']['sections'][0]['raw'][:-4]+'AAAA'))
    def test_duplicate_write(self):self.bad(lambda r:r['stages'][3]['fixture']['prepared'].append(requests(0)[0]))
    def test_seed_mixed_with_driven(self):self.bad(lambda r:r['initial']['prepared'].append(requests(0)[0]))
    def test_stale_warning_required(self):self.bad(lambda r:r['stages'][6]['geometry'].__setitem__('alerts',[]))
    def test_pending_lock_not_dropped_from_test(self):self.bad(lambda r:r['stages'][4]['journal'].__setitem__('pending',{'stage':'AnchorStarted'}))
    def test_history_both_receipts(self):self.bad(lambda r:r['stages'][3]['journal']['history'].pop())
    def test_ack_identity(self):self.bad(lambda r:r['stages'][4]['fixture']['acks'].__setitem__(0,b64(f(1,1)+f(2,f(1,f(1,'wrong'))))))
    def test_duplicate_control_wrapper(self):self.bad(lambda r:r['stages'][4]['geometry']['headings'][0]['controls'].append({'id':'old','text':'&\U0001f600'}))
    def test_permission_grant(self):self.bad(lambda r:r['initial'].__setitem__('thread',b64(f(1,'thread')+f(23,13))))
    def test_stale_mutates_body(self):self.bad(lambda r:r['stale'].__setitem__('after',section(O.IDS[1],2,'changed',1001)['raw']))
    def test_original_thread(self):self.bad(lambda r:r['stages'][4]['fixture']['roots'][0].__setitem__('text','changed'))
    def test_snippet_style(self):
        s=snippets();s['rich_text']=s['rich_text'].replace('"underline": true','"underline": false')
        with self.assertRaises(RuntimeError):O.snippet(s)
    def test_wrong_generated_actor(self):
        def mutate(r):
            row=r['stages'][3]['fixture']['sections'][0]
            raw=__import__('base64').b64decode(row['raw']).replace(b'fixture-session-0',b'incorrect-actor-0')
            row['raw']=b64(raw)
        self.bad(mutate)
    def test_wrong_issued_actor(self):
        def mutate(r):
            for key in ('prepared','requests'):
                pairs=r['stages'][3]['fixture'][key][0]['fields']
                for pair in pairs:
                    if pair[0]=='data_binary':pair[1]=b64(f(1,f(1,O.IDS[0])+generated().replace(b'fixture-session-0',b'incorrect-actor-0')))
        self.bad(mutate)
    def test_unexpected_structural_field(self):
        def mutate(r):
            row=r['stages'][3]['fixture']['sections'][0]
            row['raw']=b64(__import__('base64').b64decode(row['raw'])+f(29,1))
        self.bad(mutate)
    def test_missing_type_defaults_only_to_ordinary(self):
        self.assertEqual(O.optional_uint([],9,0),0)
        self.assertEqual(O.optional_uint(O.wire(f(9,0)),9,0),0)
        with self.assertRaises(RuntimeError):O.optional_uint(O.wire(f(9,0)+f(9,0)),9,0)
    def test_no_stage_omission(self):self.bad(lambda r:r['stages'].pop())
    def test_no_secret_form(self):
        a=requests(0)[0];a['fields'].append(['token','fixture'])
        with self.assertRaises(RuntimeError):O.form(a)

class ContentIdTests(unittest.TestCase):
    @staticmethod
    def relocate_outer_attributes(receipt):
        """Mirror partial merge relocation, preserving every other field span/order."""
        def relocate(raw):
            fields=O.wire(raw)
            return b''.join(v[3] for v in fields if v[0]!=16)+b''.join(v[3] for v in fields if v[0]==16)
        for stage in receipt['stages']:
            p=stage['fixture'];count=len(p['acks'])
            for row in p['sections']:
                if row['id'] in O.IDS[:count]:
                    row['raw']=b64(relocate(__import__('base64').b64decode(row['raw'])))
            for i,encoded in enumerate(p['acks']):
                ack=O.decoded(encoded);entries=O.wire(O.field(ack,2,2))
                data=b''.join(f(v[0],relocate(v[2]) if O.text(O.wire(v[2]),1) in O.IDS[:3] else v[2]) for v in entries)
                p['acks'][i]=b64(b''.join(f(2,data) if v[0]==2 else v[3] for v in ack))
        receipt['fixture']=copy.deepcopy(receipt['stages'][-1]['fixture'])

    def test_legitimate_outer_attributes_relocation_in_ack_and_canonical(self):
        r=sample(True);before=O.rows(r['stages'][7]['fixture'])[O.IDS[1]][1]
        self.relocate_outer_attributes(r)
        after=O.rows(r['stages'][7]['fixture'])[O.IDS[1]][1]
        self.assertNotEqual(b''.join(v[3] for v in before),b''.join(v[3] for v in after))
        self.assertEqual([v[3] for v in before if v[0]!=16],[v[3] for v in after if v[0]!=16])
        self.assertEqual(O.field(before,16,2),O.field(after,16,2))
        O.validate(r,content_id=True)

    def test_outer_relocation_does_not_allow_nested_attribute_reorder(self):
        r=sample(True);self.relocate_outer_attributes(r)
        row=r['stages'][7]['fixture']['sections'][1];fields=O.decoded(row['raw'])
        attrs=O.wire(O.field(fields,16,2))
        changed=b''.join(v[3] for v in attrs[1:]+attrs[:1])
        row['raw']=b64(b''.join(f(16,changed) if v[0]==16 else v[3] for v in fields))
        with self.assertRaisesRegex(RuntimeError,'Heading style/position/opaque field mutated'):
            O.validate(r,content_id=True)

    def test_complete_all_levels_and_old_variant_independent(self):
        O.validate(sample(True),content_id=True)
        O.validate(sample())
        with self.assertRaises(RuntimeError):O.validate(sample(),content_id=True)
        with self.assertRaises(RuntimeError):O.validate(sample(True))

    def request_attributes(self,replacement):
        r=sample(True)
        for key in ('prepared','requests'):
            for pair in r['stages'][3]['fixture'][key][0]['fields']:
                if pair[0]=='data_binary':
                    pair[1]=b64(f(1,f(1,O.IDS[0])+generated()+replacement))
        with self.assertRaisesRegex(RuntimeError,'CONTENT parent'):
            O.validate(r,content_id=True)

    def test_missing_full_attributes(self):self.request_attributes(b'')
    def test_partial_content_id_only(self):self.request_attributes(f(16,f(29,'retained-'+O.IDS[0])))
    def test_wrong_content_id(self):self.request_attributes(f(16,attributes(O.IDS[0]).replace(b'retained-',b'foreign--')))
    def test_lost_explicit_false(self):self.request_attributes(f(16,attributes(O.IDS[0])[len(f(4,0)):] ))
    def test_changed_explicit_zero(self):self.request_attributes(f(16,attributes(O.IDS[0]).replace(f(13,0),f(13,1))))
    def test_lost_unknown_wire(self):self.request_attributes(f(16,f(4,0)+f(13,0)+f(29,'retained-'+O.IDS[0])))
    def test_duplicate_outer_attributes(self):self.request_attributes(f(16,attributes(O.IDS[0]))*2)

    def test_ack_attributes_changed(self):
        r=sample(True);p=r['stages'][3]['fixture'];raw=__import__('base64').b64decode(p['sections'][0]['raw'])
        raw=raw.replace(b'heading-attributes-opaque',b'heading-attributes-mutant')
        control=__import__('base64').b64decode(p['sections'][-1]['raw'])
        p['acks'][0]=b64(f(1,1)+f(2,f(1,raw)+f(1,control)))
        with self.assertRaisesRegex(RuntimeError,'ACK/canonical bytes differ'):O.validate(r,content_id=True)

    def test_canonical_attributes_changed(self):
        r=sample(True);row=r['stages'][4]['fixture']['sections'][0]
        row['raw']=b64(__import__('base64').b64decode(row['raw']).replace(b'heading-attributes-opaque',b'heading-attributes-mutant'))
        with self.assertRaisesRegex(RuntimeError,'Heading style/position/opaque field mutated'):O.validate(r,content_id=True)

    def test_wrong_original_fixture_content_id(self):
        r=sample(True);row=r['initial']['sections'][2]
        row['raw']=b64(__import__('base64').b64decode(row['raw']).replace(b'retained-',b'foreign--'))
        with self.assertRaisesRegex(RuntimeError,'Wrong original full content-id attributes'):O.validate(r,content_id=True)

    def test_existing_control_seed_requires_complete_attributes(self):
        r=sample(True);r['initial']['seedRequests']=requests(0)
        with self.assertRaisesRegex(RuntimeError,'Existing-control seed lost'):O.validate(r,content_id=True)
if __name__=='__main__':unittest.main()
