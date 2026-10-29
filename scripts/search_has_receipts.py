"""Independent finite Search UI receipt validation, not a query implementation."""
import math
from native_check import expected_viewport

FIXTURE='search-has-filters'
QUERY='launch has:file has:file -has:file "has:link" has:pin HAS:link has:links'
KEYBOARD='report "exact phrase" -has:link has:pin'
ERROR='Close the quoted phrase before adding a filter.'
CASES=(
 ('initial',QUERY,[True,False],None,0),
 ('pending',QUERY,[False,True],None,0),
 ('applied','launch   -has:file "has:link" has:pin HAS:link has:links has:link',None,None,1),
 ('cleared','launch   -has:file "has:link"  HAS:link has:links ',None,None,2),
 ('keyboard-applied',KEYBOARD+' has:file',None,None,3),
 ('quote-refused','report "unfinished',[True,True],ERROR,3),
 ('recovered','report "unfinished" has:file has:link',[True,True],None,4),
)
ACTIONS=(('flag','Has files'),('flag','Has links'),('click','Apply filters'),('click','Filters'),('click','Clear filters'),
 ('type',KEYBOARD),('click','Filters'),('flag','Has files'),('enter',''),('type','report "unfinished'),
 ('click','Filters'),('flag','Has files'),('flag','Has links'),('click','Apply filters'),
 ('type','report "unfinished"'),('click','Filters'),('flag','Has files'),('flag','Has links'),('click','Apply filters'),('click','Filters'))

def require(ok,message):
    if not ok:raise RuntimeError(message)

def vector(value,size):
    return isinstance(value,list) and len(value)==size and all(type(x) in (int,float) and math.isfinite(x) for x in value)

def geometry(g,viewport,interactive=True):
    require(isinstance(g,dict) and g['visible'] is True and g['hit'] is True and g['disabled'] is False,'Invisible or disabled Search control')
    require(type(g['tabIndex']) is int and (not interactive or g['tabIndex']>=0),'Search control not keyboard focusable')
    r,c,p=g['rect'],g['clip'],g['point']
    require(vector(r,4) and vector(c,4) and vector(p,2),'Nonfinite Search geometry')
    require(0<=c[0]<c[2]<=viewport[0] and 0<=c[1]<c[3]<=viewport[1],'Invalid Search clip')
    require(c[0]-1<=r[0]<r[2]<=c[2]+1 and c[1]-1<=r[1]<r[3]<=c[3]+1,'Search control clipped')
    # Rect endpoints and the JS-computed point cross JSON independently. Allow
    # only two binary64 rounding steps, not a CSS-pixel/layout tolerance.
    midpoint=[(r[0]+r[2])/2,(r[1]+r[3])/2]
    require(all(abs(actual-center)<=2*math.ulp(center) for actual,center in zip(p,midpoint)),'Search hit point differs')
    require(r[0]<=p[0]<=r[2] and r[1]<=p[1]<=r[3] and c[0]<=p[0]<=c[2] and c[1]<=p[1]<=c[3],'Search hit point outside control/clip')

def focus_geometry(g,viewport):
    geometry(g,viewport)
    require(g['focused'] is True and g['focusVisible'] is True,'Search action focus missing')
    width,offset=g['outlineWidth'],g['outlineOffset']
    require(vector([width,offset],2) and width==2 and g['outlineStyle']=='solid','Search outline style differs')
    require(isinstance(g['outlineColor'],str) and g['outlineColor'] not in ('','transparent','rgba(0, 0, 0, 0)'),'Search outline color missing')
    r,c=g['rect'],g['clip'];expansion=max(0,width+offset)
    expected=[r[0]-expansion,r[1]-expansion,r[2]+expansion,r[3]+expansion]
    require(vector(g['outline'],4) and all(abs(a-b)<=2*math.ulp(b) for a,b in zip(g['outline'],expected)),'Search outline derivation differs')
    # No CSS-pixel allowance: the entire ring must fit, even when the center hits.
    require(c[0]<=expected[0]<expected[2]<=c[2] and c[1]<=expected[1]<expected[3]<=c[3],'Search outline clipped')

def validate_stage(stage,index,fixture,first_revision=None,platform=None):
    require(type(index) is int and 0<=index<len(CASES),'Unknown Search stage')
    name,query,flags,error,revision=CASES[index]
    require(type(stage['ordinal']) is int and stage['ordinal']==index and stage['stage']==name,'Search stage order')
    require(all(stage[k] is True for k in ('ok','noSession','noTransport')),'Search isolation')
    native,dom=stage['native'],stage['dom'];viewport=expected_viewport(fixture) if platform is None else expected_viewport(fixture,platform)
    require(dom['viewport']==viewport and all(type(x) is int for x in dom['viewport']),'Search viewport')
    require(dom['ok'] is True and dom['query']==native['query']==query and dom['queryRevision']==query,'Search query changed')
    require(dom['panel'] is (flags is not None) and native['panel'] is (flags is not None),'Search panel state')
    require(dom['expanded']==str(flags is not None).lower(),'Search aria-expanded differs')
    for value in (native['flags'],dom['flags']):
        require(value is None if flags is None else isinstance(value,list) and len(value)==2 and all(type(v) is bool for v in value) and value==flags,'Search selected state')
    require(native['error']==dom['alert']==error,'Search refusal changed')
    require(all(native[k] is True for k in ('noSession','noTransport')) and native['loading'] is False,'Search credential/loading state')
    require(all(type(v) is int and v==0 for v in (native['results'],native['page'],dom['results'])),'Fabricated Search results')
    require(type(native['revision']) is int and native['revision']>=0,'Search revision type')
    if first_revision is not None:require(native['revision']==first_revision+revision,'Query remount/refusal revision changed')
    require(dom['labels']==([] if flags is None else ['Has files','Has links','In threads','Saved items','Pinned messages']),'Search labels changed')
    require(dom['horizontalOverflow'] is False,'Search horizontal overflow')
    r=dom['dialog'];require(vector(r,4) and 0<=r[0]<r[2]<=viewport[0] and 0<=r[1]<r[3]<=viewport[1],'Search dialog outside viewport')
    keys={'query','filters'}|({'file','link','apply','clear'} if flags is not None else set())|({'alert'} if error else set())
    require(set(dom['controls'])==keys,'Missing Search geometry')
    for key,g in dom['controls'].items():geometry(g,viewport,key!='alert')
    if name=='quote-refused':
        focus=dom['controls']['apply']['focusGeometry'];focus_geometry(focus,viewport)
        require(all(focus[k]==dom['controls']['apply'][k] for k in ('rect','clip','point','visible','hit','disabled','tabIndex')),'Search retained focus control differs')

def validate(fixture,receipt,platform=None):
    require(fixture in (FIXTURE,FIXTURE+'-narrow') and receipt['fixture']==fixture,'Search fixture differs')
    require(all(receipt[k] is True for k in ('ok','noSession','noTransport')),'Search final isolation')
    require(receipt['credentialBoundary']=='actual search credentials refused; no raw request ledger','Unexpected credential claim')
    rows=receipt['records'];require(isinstance(rows,list) and len(rows)==len(CASES),'Missing Search stages')
    first=rows[0]['native']['revision']
    for index,row in enumerate(rows):validate_stage(row,index,fixture,first,platform)
    require(type(receipt['refusedRevision']) is int and receipt['refusedRevision']==first+3,'Refusal changed query revision')
    actions=receipt['actions'];require(len(actions)==len(ACTIONS),'Missing Search actions')
    for action,(kind,value) in zip(actions,ACTIONS):
        expected={'ok':True,'kind':kind,'value':value,'focusedAtDispatch':True}
        if kind=='click' and value in ('Apply filters','Clear filters'):
            focus_geometry(action['focusGeometry'],rows[0]['dom']['viewport'])
            expected['focusGeometry']=action['focusGeometry']
        require(action==expected and type(action['ok']) is bool and type(action['focusedAtDispatch']) is bool,'Search action/focus sequence differs')
    for key in ('query','filters','file','link','apply','clear'):
        a=rows[0]['dom']['controls'][key]['rect'];b=rows[1]['dom']['controls'][key]['rect']
        require(all(abs(x-y)<=1 for x,y in zip(a,b)),'Checkbox toggle shifted Search layout')
    require(receipt['layout']=={'ok':True,'viewport':rows[-1]['dom']['viewport']} and type(receipt['layout']['ok']) is bool,'Search final layout differs')
