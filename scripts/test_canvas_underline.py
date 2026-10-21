"""Adversarial offline underline receipt tests; no browser/native execution."""
import copy
import unittest
import canvas_underline as U
from test_canvas_heading import f, b64, body, source

def blocks(rtml):
    return [{'type':'rich_text','elements':[{'type':'rich_text_section','elements':[
        {'type':'text','text':row[0],'style':dict(zip(U.STYLE_KEYS,row[1:]))} for row in U.styled(rtml)]}]}]

def sample():
    original=[source(i,l,r) for i,l,r in zip(U.IDS,U.LEVELS,U.INITIAL)]
    writes=[];acks=[]
    for n,rtml in enumerate(U.DESIRED):
        identity=('paragraph','heading-one','heading-two','heading-three','heading-three')[n]
        row=f(1,identity)+f(2,1000)+f(9,0)+f(12,body(rtml))+f(13,b'')+f(19,1)+f(20,1)+f(33,'original-session')+f(38,f(3,1000))
        writes.append({'section':identity,'documentData':b64(f(1,row)+f(14,b'')),'sectionFields':[1,2,9,12,13,19,20,33,38],'desired':rtml,'sequence':n+1})
        if n<4:acks.append(b64(f(1,identity)+f(2,2000)+f(6,'thread')+f(7,'document')+f(12,body(rtml))+f(38,f(3,2000))))
    result={'original':original,'layout':{'viewport':[668,380]},'stages':[]}
    for n,name in enumerate(U.STAGES):
        sections=[source(i,l,U.DESIRED[j-1] if 1<=j<=U.ACCEPTED[n] else r,1<=j<=U.ACCEPTED[n]) for j,(i,l,r) in enumerate(zip(U.IDS,U.LEVELS,U.INITIAL))]
        rows=[]
        for row in sections[1:]:
            rtml=U.content(U.decoded(row['raw']))
            rows.append({'id':row['id'],'text':U.plain(rtml),'tag':f"H{row['style']}" if row['style'] else 'DIV','nested':row['id']=='neighbor','editLabel':'Edit heading' if row['style'] else 'Edit paragraph','painted':[[c,u] for c,_,_,u,_,_ in U.styled(rtml)]})
        native=None
        if n in (1,2,3,4,9,10,11):
            expected={1:U.CANCEL,2:U.STALE,3:U.DESIRED[0],4:U.DESIRED[0],9:U.DESIRED[4],10:U.DESIRED[4],11:U.DESIRED[4]}[n]
            label='Paragraph' if n in (3,4) else 'Heading'
            native={'generation':'7','visit':'1','label':label,'toolbarLabel':'Editing '+label.lower(),'visible':True,'underlineButton':True,'blocks':blocks(expected),'text':U.plain(expected),'editable':n<4,'saveDisabled':n==2 or n>=4,'cancelDisabled':n>=4,
                    'toolbarProof':{'insideInput':True,'outsideInput':True,'toggles':[True,False,True],'original':blocks(U.DESIRED[0].replace('<u>toggle</u>','toggle'))}}
        proof={'attempts':U.ATTEMPTS[n],'reads':U.READS[n],'uncertain':n>=9,'writes':writes[:U.ATTEMPTS[n]],'acknowledgements':acks[:U.ACCEPTED[n]],'sections':sections}
        result['stages'].append({'ok':True,'stage':name,'ordinal':n,'noSession':True,'noTransport':True,'fixture':proof,'editorLayout':{'ok':True},'geometry':{'ok':True,'noOverflow':True,'viewport':[668,380],'rows':rows,'native':native}})
    result['fixture']=copy.deepcopy(result['stages'][-1]['fixture'])
    return copy.deepcopy(result)

class UnderlineTests(unittest.TestCase):
    def refuses(self,change):
        value=sample();change(value)
        with self.assertRaises(RuntimeError):U.validate(value)
    def test_complete(self):U.validate(sample())
    def test_underlined_entities_unicode_softbreak_and_companion_marks(self):
        self.assertEqual(U.plain(U.DESIRED[1]),'First 中🙂\n\nSecond 日🦊 & <literal>')
        self.assertEqual(U.native_runs(blocks(U.DESIRED[1])),U.styled(U.DESIRED[1]))
        self.assertTrue(any(row[1] and row[3] for row in U.styled(U.DESIRED[1])))
        self.assertTrue(any(row[3] and row[4] for row in U.styled(U.DESIRED[3])))
    def test_native_underline_cannot_silently_flatten(self):
        self.refuses(lambda r:r['stages'][3]['geometry']['native']['blocks'][0]['elements'][0]['elements'][15]['style'].__setitem__('underline',False))
    def test_interior_insertion_is_underlined_and_boundary_result_refused(self):
        current=U.DESIRED[0]
        self.assertIn('<u>unINNER der &amp;',current)
        old=current.replace('<u>unINNER der &amp;','INNER <u>under &amp;')
        self.refuses(lambda r:r['stages'][3]['geometry']['native'].__setitem__('blocks',blocks(old)))
        self.refuses(lambda r:r['stages'][3]['geometry']['native']['toolbarProof'].__setitem__('original',blocks(old.replace('<u>toggle</u>','toggle'))))
    def test_companion_mark_cannot_disappear(self):
        self.refuses(lambda r:r['stages'][9]['geometry']['native']['blocks'][0]['elements'][0]['elements'][0]['style'].__setitem__('bold',False))
    def test_rendered_underline_is_independently_required(self):
        self.refuses(lambda r:r['stages'][5]['geometry']['rows'][0]['painted'][15].__setitem__(1,False))
    def test_toolbar_must_really_toggle_off_and_on(self):
        for value in ([True,True,True],[True,False],[],None):
            with self.subTest(value=value):self.refuses(lambda r:r['stages'][3]['geometry']['native']['toolbarProof'].__setitem__('toggles',value))
    def test_inside_and_outside_input_required(self):
        for key in ('insideInput','outsideInput'):
            self.refuses(lambda r:r['stages'][3]['geometry']['native']['toolbarProof'].__setitem__(key,False))
    def test_toggle_cannot_rewrite_existing_run(self):
        self.refuses(lambda r:r['stages'][3]['geometry']['native']['toolbarProof'].__setitem__('original',blocks('Different')))
    def test_original_structure_and_duplicate_twin_retained(self):
        for key,value in [('id','heading-one'),('style',1),('raw','AA==')]:
            self.refuses(lambda r:r['stages'][8]['fixture']['sections'][5].__setitem__(key,value))
    def test_no_style_field_on_content_request(self):
        def mutation(r):
            w=r['stages'][4]['fixture']['writes'][0]
            row=U.field(U.decoded(w['documentData']),1,2)
            w['documentData']=b64(f(1,row+f(10,0)))
        self.refuses(mutation)
    def test_stale_save_disabled_cancel_enabled(self):
        self.refuses(lambda r:r['stages'][2]['geometry']['native'].__setitem__('saveDisabled',False))
        self.refuses(lambda r:r['stages'][2]['geometry']['native'].__setitem__('cancelDisabled',True))
    def test_pending_duplicate_save_is_refused(self):
        for key in ('saveDisabled','cancelDisabled'):
            self.refuses(lambda r:r['stages'][4]['geometry']['native'].__setitem__(key,False))
    def test_uncertain_readback_preserves_original_holder(self):
        for key,value in [('generation','8'),('visit','2'),('editable',True),('text','Dropped'),('blocks',[])]:
            self.refuses(lambda r:r['stages'][11]['geometry']['native'].__setitem__(key,value))
    def test_uncertain_has_no_new_send_or_ack(self):
        self.refuses(lambda r:r['stages'][11]['fixture'].__setitem__('attempts',6))
        self.refuses(lambda r:r['stages'][11]['fixture']['acknowledgements'].append('AA=='))
    def test_exact_stage_read_order(self):
        self.refuses(lambda r:r['stages'].reverse())
        self.refuses(lambda r:r['stages'][10]['fixture'].__setitem__('reads',6))
    def test_no_live_authority(self):
        self.refuses(lambda r:r['stages'][1].__setitem__('noSession',False))
    def test_unconfirmed_source_is_not_new_canonical(self):
        self.refuses(lambda r:r['stages'][9]['fixture']['sections'].__setitem__(4,source('heading-three',3,U.DESIRED[4],True)))
    def test_heading_semantics_and_controls_visible(self):
        self.refuses(lambda r:r['stages'][7]['geometry']['rows'][2].__setitem__('tag','DIV'))
        self.refuses(lambda r:r['stages'][4]['editorLayout'].__setitem__('ok',False))

if __name__=='__main__':unittest.main()
