"""Strict visible-surface and non-ticket native observations for enrollment."""
import math
import re


def install(h):
    globals().update({k:h[k] for k in ("same","require","keys","number","rectangle","expected_viewport","native","BASE","INITIAL","NEWER")})


def button(value, viewport, *, text=None, disabled=False):
    keys(value,("id","text","label","rect","disabled","hit","visible"),"button proof")
    require(all(isinstance(value[k],str) for k in ("id","text","label")),"Invalid button identity")
    same(value["disabled"],disabled,"Unexpected button enabled state");same(value["visible"],True,"Button not fully visible")
    if text is not None: same(value["text"],text,"Wrong exact control text")
    r=value["rect"];require(rectangle(r,viewport),"Clipped button")
    proof=value["hit"];keys(proof,("ok","method","points","computedRadii","inset","radius"),"rounded hit proof")
    same(proof["ok"],True,"Button obscured");same(proof["method"],"rounded-arc","Wrong button sampler");same(proof["inset"],1,"Wrong hit inset")
    radius=proof["radius"];w,h=r["width"],r["height"]
    require(number(radius) and 0<=radius<=min(w,h)/2 and min(w,h)>2,"Invalid rounded geometry")
    require(isinstance(proof["computedRadii"],list) and len(proof["computedRadii"])==4,"Missing four computed radii")
    for css in proof["computedRadii"]:
        require(isinstance(css,str),"Invalid radius CSS")
        parts=css.split();require(len(parts) in (1,2) and all(re.fullmatch(r"[0-9]+(?:\.[0-9]+)?px",v) for v in parts),"Unsupported radius CSS")
        require(all(float(v[:-2])==radius for v in parts),"Nonuniform/noncircular radius")
    d=radius-(radius-1)/math.sqrt(2) if radius>1 else 1
    positions=[("center",w/2,h/2),("top",w/2,1),("right",w-1,h/2),("bottom",w/2,h-1),("left",1,h/2),
               ("top-left",d,d),("top-right",w-d,d),("bottom-right",w-d,h-d),("bottom-left",d,h-d)]
    require(isinstance(proof["points"],list) and len(proof["points"])==9,"Missing button hit points")
    for point,(name,x,y) in zip(proof["points"],positions):
        keys(point,("name","x","y","owned","element"),"button hit")
        same(point["name"],name,"Wrong ordered hit");same(point["owned"],True,"Unowned button hit")
        for axis,offset in (("x",x),("y",y)):
            require(number(point[axis]) and abs(point[axis]-r[axis]-offset)<1e-6,"Wrong button hit coordinate")
        keys(point["element"],("tag","id","classes"),"hit element")
        require(all(isinstance(v,str) for v in point["element"].values()) and point["element"]["tag"],"Missing hit diagnostic")


def clicked(value, control, text, viewport, generation):
    keys(value,("ok","control","generation","clicked","proof","bodyScroll","focused"),"actual click")
    for k,v in dict(ok=True,control=control,generation=generation,clicked=True,focused=True).items():same(value[k],v,"Wrong actual click "+k)
    same(value["proof"]["id"],"protected-enrollment-"+control,"Wrong clicked element")
    button(value["proof"],viewport,text=text)
    same(value["bodyScroll"],0 if generation else None,"Modal body not restored before actual click")


def observed(value, fixture, target, text):
    keys(value,("ok","target","before","native","readOnly","disabled","admission","fieldFocused","activeId","visibilityEvidence"),"native observation")
    for k,v in dict(ok=True,target=target,readOnly=False,disabled=False,admission=None,visibilityEvidence="not-measured").items():same(value[k],v,"Wrong native observation "+k)
    require(type(value["fieldFocused"]) is bool and isinstance(value["activeId"],str),"Missing focus observation")
    keys(value["before"],("text","start","end","direction"),"pre-observation state")
    native(value["native"],text,expected_viewport(fixture))


def surface(value,fixture,scenario,stage,page,target,generation,message):
    keys(value,("ok","page","generation","surfaces","hits","buttons","scroll","viewport","documentWidth","text","title","rowKinds","pending"),"visible enrollment surface")
    viewport=expected_viewport(fixture);modal=page!="status";busy=stage in ("prepared","enqueued","interleaved") and modal
    for k,v in dict(ok=True,page=page,generation=generation if modal else None,viewport=viewport,pending=scenario=="failed-editor-channel" and modal,rowKinds=["channel","thread"] if modal else []).items():same(value[k],v,"Wrong surface identity "+k)
    require(number(value["documentWidth"]) and 0<value["documentWidth"]<=viewport[0]+1,"Horizontal overflow")
    names=("panel","selected","body","header","footer") if modal else ("panel","selected")
    keys(value["surfaces"],names,"surface bounds");keys(value["hits"],names,"surface hit proofs")
    for name in names:
        require(rectangle(value["surfaces"][name],viewport),"Clipped visible "+name)
        BASE["hit_grid"](value["hits"][name],value["surfaces"][name])
    bounds=value["surfaces"]
    BASE["rect_inside"](bounds["selected"],bounds["body"] if modal else bounds["panel"],viewport)
    if modal:
        for name in ("body","header","footer"):BASE["rect_inside"](bounds[name],bounds["panel"],viewport)
        require(bounds["body"]["y"]>=bounds["header"]["y"]+bounds["header"]["height"]-1
                and bounds["body"]["y"]+bounds["body"]["height"]<=bounds["footer"]["y"]+1,"Body overlaps fixed header/footer")
    scroll=value["scroll"];keys(scroll,("before","after","bodyHeight","bodyScrollHeight"),"modal scroll evidence")
    for label in ("before","after"):
        keys(scroll[label],("document","body"),"scroll observation");same(scroll[label]["document"],0,"Ancestor document scrolled")
        if modal: require(number(scroll[label]["body"]) and scroll[label]["body"]>=0,"Invalid body scroll")
        else:same(scroll[label]["body"],None,"Unexpected modal scroll")
    if modal:require(number(scroll["bodyHeight"]) and scroll["bodyHeight"]>0 and number(scroll["bodyScrollHeight"]) and scroll["bodyScrollHeight"]>=scroll["bodyHeight"],"Invalid scroll height")
    else:
        same(scroll["bodyHeight"],None,"Unexpected body height");same(scroll["bodyScrollHeight"],None,"Unexpected scroll height")
    intro="Keep this draft with recovery on this device. Enabling recovery does not send the draft or change its text."
    channel="Channel draft"+INITIAL+"Saved on this device";thread="Thread draft"+INITIAL+"Saved on this device"
    pending="Input waiting to be saved"+NEWER+"Recover this input before enabling recovery for another version."
    if modal:
        same(value["title"],"Enable "+target+" draft recovery?","Wrong consent destination")
        same(value["text"],{"channel":channel,"thread":thread,"pending":pending,"modal":intro+channel+thread+(pending if scenario=="failed-editor-channel" else "")}[page],"Wrong displayed whole inventory section")
        keys(value["buttons"],("close","protected-enrollment-cancel","protected-enrollment-submit"),"modal controls")
        for key,proof in value["buttons"].items():
            button(proof,viewport,text="" if key=="close" else ("Cancel waiting" if busy else "Cancel") if key.endswith("cancel") else "Enabling recovery..." if busy else "Enable recovery",disabled=busy and key.endswith("submit"))
        same(value["buttons"]["close"]["label"],"Close enrollment","Wrong close control")
    else:
        same(value["title"],None,"Unexpected modal title")
        pending_result=scenario=="canceled-waiter-channel" and stage=="interleaved"
        controls=["protected-enrollment-reload","protected-enrollment-open"]+(["protected-enrollment-check"] if pending_result else [])
        keys(value["buttons"],controls,"status controls")
        for key,proof in value["buttons"].items():button(proof,viewport,text={"protected-enrollment-reload":"Reload saved drafts","protected-enrollment-open":"Review draft","protected-enrollment-check":"Check status"}[key],disabled=pending_result and key.endswith("open"))
        same(value["text"],message+("Check status" if pending_result else "")+"Reload saved draftsReview draft","Wrong actual status message/actions")
