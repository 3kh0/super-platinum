"""Independent underline native receipt contract; shared bounded protobuf decoder."""
from html.parser import HTMLParser
from canvas_heading import require, decoded, wire, field, text, content, stable

STAGES = ("initial","cancel-draft","stale-draft","paragraph-toolbar","paragraph-saving","paragraph-confirmed","h1-confirmed","h2-confirmed","h3-confirmed","uncertain","readback-once","readback-twice")
IDS = ("title","paragraph","heading-one","heading-two","heading-three","heading-twin","neighbor")
LEVELS = (48,0,1,2,3,2,0)
INITIAL = (None,"Outside <u>under &amp; 中🙂</u><br>Tail toggle","<u>First <b>heading</b> 中🙂</u>","<u>Second <i>heading</i> 中🙂</u>","<u>Third <del>heading</del> 中🙂</u>","<u>Second <i>heading</i> 中🙂</u>","Untouched <i>neighbor</i> &amp; details.")
DESIRED = ("Before Outside <u>unINNER der &amp; 中🙂</u><br>Tail <u>toggle</u>","<b><u>First 中🙂</u></b><br><br><i><u>Second 日🦊</u></i> &amp; &lt;literal&gt;","Second <i><u>heading 中🙂</u></i> &amp; details","Third <u><del>heading 中🙂</del></u> &lt;done&gt;","<b><u>Unconfirmed 中🙂</u></b> keep this")
CANCEL = "<u>Cancelled 中🙂</u>"
STALE = "<i><u>Retained stale 日🦊</u></i>"
ATTEMPTS = (0,0,0,0,1,1,2,3,4,5,5,5)
ACCEPTED = (0,0,0,0,0,1,2,3,4,4,4,4)
READS = (1,1,1,2,2,3,4,5,6,6,7,8)
STYLE_KEYS = ("bold","italic","underline","strike","code")

def styled(rtml):
    class Runs(HTMLParser):
        def __init__(self):
            super().__init__(convert_charrefs=True)
            self.styles = [False]*5
            self.out=[]
        def handle_starttag(self,tag,attrs):
            if tag=='br': self.handle_data('\n'); return
            require(tag in ('b','i','u','del','code') and not attrs,'Unsupported oracle RTML')
            self.styles[('b','i','u','del','code').index(tag)] = True
        def handle_endtag(self,tag):
            require(tag in ('b','i','u','del','code'),'Unsupported closing RTML')
            self.styles[('b','i','u','del','code').index(tag)] = False
        def handle_data(self,data):
            self.out.extend((c,*self.styles) for c in data)
    p=Runs();p.feed(rtml);return p.out

def plain(rtml): return ''.join(x[0] for x in styled(rtml))

def native_runs(blocks):
    require(isinstance(blocks,list) and len(blocks)==1 and blocks[0].get('type')=='rich_text','Bad native blocks')
    sections=blocks[0].get('elements')
    require(isinstance(sections,list) and len(sections)==1 and sections[0].get('type')=='rich_text_section','Bad native paragraph count')
    runs=sections[0].get('elements');require(isinstance(runs,list),'No native runs')
    out=[]
    for run in runs:
        require(run.get('type')=='text' and isinstance(run.get('text'),str),'Unexpected native token')
        style=run.get('style',{})
        require(isinstance(style,dict) and all(k in STYLE_KEYS and type(v) is bool for k,v in style.items()),'Unexpected native marks')
        out.extend((c,*(style.get(k,False) for k in STYLE_KEYS)) for c in run['text'])
    return out

def inventory(rows):
    require(isinstance(rows,list) and len(rows)==len(IDS),'Wrong source inventory')
    result={}
    for row,identity,level in zip(rows,IDS,LEVELS):
        require(row.get('id')==identity and type(row.get('style')) is int and row['style']==level,'Source identity/style differs')
        raw=decoded(row.get('raw'))
        require(text(raw,1)==identity and field(raw,10,0)==level and row.get('sequence')==field(raw,2,0),'Wire identity/style differs')
        result[identity]=raw
    return result

def validate(receipt):
    original=inventory(receipt.get('original'))
    for identity,wanted in zip(IDS[1:],INITIAL[1:]): require(content(original[identity])==wanted,'Original content differs')
    stages=receipt.get('stages');require(isinstance(stages,list) and len(stages)==len(STAGES),'Missing stages')
    previous_writes=[];previous_acks=[]
    for index,(stage,name) in enumerate(zip(stages,STAGES)):
        require(stage.get('stage')==name and type(stage.get('ordinal')) is int and stage['ordinal']==index and all(stage.get(k) is True for k in ('ok','noSession','noTransport')),'Invalid stage/isolation')
        source=stage.get('fixture',{})
        require(type(source.get('attempts')) is int and source['attempts']==ATTEMPTS[index] and type(source.get('reads')) is int and source['reads']==READS[index] and source.get('uncertain') is (index>=9),'Wrong requests/readback')
        writes=source.get('writes');acks=source.get('acknowledgements')
        require(isinstance(writes,list) and len(writes)==ATTEMPTS[index] and isinstance(acks,list) and len(acks)==ACCEPTED[index] and writes[:len(previous_writes)]==previous_writes and acks[:len(previous_acks)]==previous_acks,'Original receipts changed')
        previous_writes,previous_acks=writes,acks
        for n,write in enumerate(writes):
            identity=("paragraph","heading-one","heading-two","heading-three","heading-three")[n]
            row=wire(field(decoded(write.get('documentData')),1,2));tags=[f[0] for f in row]
            require(tags==[1,2,9,12,13,19,20,33,38] and write.get('sectionFields')==tags and text(row,1)==identity and write.get('section')==identity and write.get('desired')==DESIRED[n] and content(row)==DESIRED[n] and field(row,9,0)==0 and field(row,13,2)==b'','Wrong CONTENT-only request')
        current=inventory(source.get('sections'))
        for i,identity in enumerate(IDS):
            old,new=original[identity],current[identity]
            require(stable(old)==stable(new),'Structure or opaque bytes changed')
            if 1<=i<=ACCEPTED[index]:
                require(content(new)==DESIRED[i-1] and field(new,2,0)==2000,'Accepted content differs')
                ack=decoded(acks[i-1])
                require([f[0] for f in ack]==[1,2,6,7,12,38] and text(ack,1)==identity and field(ack,2,0)==2000 and content(ack)==DESIRED[i-1],'Original ACK differs')
            else: require(old==new,'Unchanged/unconfirmed section mutated')
        geometry=stage.get('geometry',{})
        require(geometry.get('ok') is True and geometry.get('noOverflow') is True and geometry.get('viewport')==receipt['layout']['viewport'],'Invalid layout')
        rows=geometry.get('rows');require(isinstance(rows,list) and len(rows)==6,'Missing visible source rows')
        for row,identity,level in zip(rows,IDS[1:],LEVELS[1:]):
            expected=content(current[identity]);characters=styled(expected)
            require(row.get('id')==identity and row.get('text')==plain(expected) and row.get('tag')==(f'H{level}' if level else 'DIV') and (identity=='neighbor' or row.get('nested') is False) and row.get('editLabel')==('Edit heading' if level else 'Edit paragraph'),'Wrong rendered identity/body')
            require(row.get('painted')==[[c,under] for c,_,_,under,_,_ in characters],'Visible underline differs from canonical content')
        native=geometry.get('native');editing=index in (1,2,3,4,9,10,11)
        require(isinstance(native,dict) if editing else native is None,'Wrong editor presence')
        if editing:
            label='Paragraph' if index in (3,4) else 'Heading'
            require(native.get('label')==label and native.get('toolbarLabel')=='Editing '+label.lower() and native.get('underlineButton') is True and native.get('visible') is True and stage.get('editorLayout',{}).get('ok') is True,'Missing editor/underline controls')
            expected={1:CANCEL,2:STALE,3:DESIRED[0],4:DESIRED[0],9:DESIRED[4],10:DESIRED[4],11:DESIRED[4]}[index]
            require(native_runs(native.get('blocks'))==styled(expected),'Native underline/companion styles lost')
            if index in (3,4):
                proof=native.get('toolbarProof',{})
                require(proof.get('insideInput') is True and proof.get('outsideInput') is True and proof.get('toggles')==[True,False,True],'Missing real input/toggle sequence')
                require(native_runs(proof.get('original'))==styled(DESIRED[0].replace('<u>toggle</u>','toggle')),'Inside/outside input changed other marks')
            if index in (4,9,10,11): require(native.get('saveDisabled') is True and native.get('cancelDisabled') is True and native.get('editable') is False,'Pending write replayable')
            if index==2: require(native.get('saveDisabled') is True and native.get('cancelDisabled') is False,'Stale edit allowed save')
            if index>=9:
                require(native.get('text')==plain(DESIRED[4]) and native==stages[9]['geometry']['native'],'Uncertain native holder/draft replaced')
    require(receipt.get('fixture')==stages[-1]['fixture'],'Terminal source changed')
