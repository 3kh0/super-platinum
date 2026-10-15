"""Pure native receipt contract; no process, network, or platform secret storage."""
import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
sys.path.insert(0,str(Path.cwd()/"scripts"))
from canvas_native import validate, validate_closed
from native_check import expected_viewport
spec=importlib.util.spec_from_file_location("continuation_runner",Path(__file__).with_name("agent-canvas-structure-text-native.py"))
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
count=0
def refuses(fn):
    global count
    try:fn()
    except RuntimeError:count+=1
    else:raise AssertionError("Invalid contract was admitted")
for fixture in [m.SPEC.fixture,m.SPEC.fixture+"-narrow"]:
    terminal={"ok":True,"noSession":True,"noTransport":True,"layout":{"ok":True,"viewport":expected_viewport(fixture)},**m.SPEC.counts,**{k:True for k in m.SPEC.required}}
    validate(m.SPEC,fixture,terminal);count+=1
    for key in ["ok","noSession","noTransport",*m.SPEC.required]:
        bad=copy.deepcopy(terminal);bad[key]=False;refuses(lambda:validate(m.SPEC,fixture,bad))
    for key in m.SPEC.counts:
        for value in [False,True,2.0,None,0,1,3]:
            bad=copy.deepcopy(terminal);bad[key]=value;refuses(lambda:validate(m.SPEC,fixture,bad))
    stages=[{"ok":True,"stage":name,"ordinal":i,"noSession":True,"noTransport":True} for i,name in enumerate(m.SPEC.stages)]
    assert len(stages)==13
    with tempfile.TemporaryDirectory(prefix="sp-continuation-") as directory:
        path=Path(directory)/"closed.log"
        def write(stage_values,finals):
            path.write_text(''.join("canvas-structure-text-stage: "+json.dumps(v)+"\n" for v in stage_values)+''.join("canvas-structure-text-check: "+json.dumps(v)+"\n" for v in finals),encoding="utf-8")
        write(stages,[terminal]);validate_closed(m.SPEC,fixture,path,stages,terminal);count+=1
        for bad in [stages[:-1],stages[::-1],stages+[stages[-1]]]:
            write(bad,[terminal]);refuses(lambda:validate_closed(m.SPEC,fixture,path,stages,terminal))
        write(stages,[terminal,terminal]);refuses(lambda:validate_closed(m.SPEC,fixture,path,stages,terminal))
        write(stages,[terminal]);path.write_text(path.read_text(encoding="utf-8").rstrip("\n"),encoding="utf-8");refuses(lambda:validate_closed(m.SPEC,fixture,path,stages,terminal))
print(f"{count} continuation receipt cases passed")
