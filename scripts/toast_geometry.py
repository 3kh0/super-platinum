"""Independent actual-notice geometry oracle for disconnected download fixtures."""
import math

def require(ok,why):
    if not ok:raise RuntimeError('Toast geometry: '+why)

def vector(value,size):
    return isinstance(value,list) and len(value)==size and all(type(x) in (int,float) and math.isfinite(x) for x in value)

def rect(value):
    return vector(value,4) and value[0]<value[2] and value[1]<value[3]

def inside(a,b):return a[0]>=b[0] and a[1]>=b[1] and a[2]<=b[2] and a[3]<=b[3]
def clear(a,b,gap=0):return a[2]+gap<=b[0] or a[0]>=b[2]+gap or a[3]+gap<=b[1] or a[1]>=b[3]+gap

def validate_toast(proof,viewport,text):
    require(isinstance(proof,dict),'missing actual notice proof')
    require(all(proof.get(k) is True for k in ('ok','positioned','visible')),'hidden/unpositioned notice')
    require(proof.get('text')==text and isinstance(text,str) and text,'wrong notice text')
    require(vector(proof.get('viewport'),2) and proof['viewport']==viewport,'viewport mismatch')
    bounds=proof.get('bounds');require(rect(bounds) and inside(bounds,[0,0,*viewport]),'notice outside viewport')
    for key,hit_key in (('textBounds','textHit'),('dismissBounds','dismissHit')):
        child=proof.get(key);require(rect(child) and inside(child,bounds),'clipped notice child')
        hit=proof.get(hit_key);require(isinstance(hit,dict) and hit.get('hit') is True and vector(hit.get('point'),2),'unreachable notice child')
        midpoint=[(child[0]+child[2])/2,(child[1]+child[3])/2]
        require(all(abs(a-b)<=2*math.ulp(b) for a,b in zip(hit['point'],midpoint)),'child hit point differs')
    gap=proof.get('gap');require(type(gap) in (int,float) and gap==8,'protected-surface gap changed')
    rows=proof.get('obstacles');require(isinstance(rows,list) and rows,'missing protected surfaces')
    kinds=set()
    for row in rows:
        require(isinstance(row,dict) and row.get('kind') in ('composer','completion','dialog') and rect(row.get('rect')),'invalid protected surface')
        kinds.add(row['kind']);require(clear(bounds,row['rect'],gap),'notice overlaps protected surface')
    require({'dialog','composer'}<=kinds,'viewer/composer proof missing')
    focus=proof.get('focus')
    require(focus is None or rect(focus) and clear(bounds,focus),'notice overlaps focused control')
