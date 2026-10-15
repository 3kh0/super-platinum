#!/usr/bin/env python3
"""Run the one already-built exact ignored test once. No Cargo/build commands."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
from emoji_retirement_overhead import validate
from emoji_retirement_receipts import require

TEST='performance::emoji_retirement::overhead::finite_actual_empty_wrapper_observer_overhead'
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--test-binary',required=True,type=Path)
    parser.add_argument('--build-receipt',required=True,type=Path)
    parser.add_argument('--output',required=True,type=Path)
    args=parser.parse_args()
    require(sys.platform=='win32','Exact overhead gate requires Windows')
    native=runpy.run_path(str(Path(__file__).with_name('check-emoji-retirement-native.py')))
    binary=args.test_binary.resolve();build=native['sealed'](binary,args.build_receipt.resolve())
    output=args.output.resolve();output.mkdir(parents=True,exist_ok=False)
    directory=output/'emoji-retirement-overhead-single-attempt'
    env={k:v for k,v in os.environ.items() if not k.startswith('SUPER_PLATINUM_')}
    env.update(SUPER_PLATINUM_EMOJI_RETIREMENT_OVERHEAD_DIR=str(directory),
               SUPER_PLATINUM_EMOJI_RETIREMENT_MANIFEST_SHA256=build['manifest_sha256'])
    evidence={'schema':1,'kind':'sealed-empty-wrapper-overhead','ok':False,'build':build,'test':TEST,
        'binary':str(binary),'build_receipt':str(args.build_receipt.resolve())}
    process=None
    try:
        with (output/'test.log').open('w',encoding='utf-8') as log:
            process=subprocess.Popen([str(binary),'--ignored','--exact',TEST,'--nocapture','--test-threads=1'],
                env=env,cwd=native['ROOT'],stdout=log,stderr=log,creationflags=subprocess.CREATE_NO_WINDOW)
            evidence['pid']=process.pid
            try:code=process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                process.kill();process.wait(timeout=5);raise RuntimeError('Single overhead attempt timed out; kill is cleanup only')
        evidence['exitCode']=code
        require(code==0,'Ignored overhead test failed; retain attempt, no rerun')
        require(native['sealed'](binary,args.build_receipt.resolve())==build,'Overhead post-run provenance changed')
        receipt=json.loads((directory/'receipt.json').read_text(encoding='utf-8'))
        require(receipt['pid']==process.pid,'Overhead test process mismatch')
        disabled=(directory/'disabled.bin').read_bytes();enabled=(directory/'enabled.bin').read_bytes()
        evidence['analysis']=validate(receipt,disabled,enabled,build['manifest_sha256'])
        evidence['receipt']=str(directory/'receipt.json')
        evidence['raw']={name:hashlib.sha256((directory/name).read_bytes()).hexdigest() for name in ('receipt.json','disabled.bin','enabled.bin')}
        evidence['ok']=True
    except BaseException as error:
        evidence['error']=f'{type(error).__name__}: {error}';raise
    finally:
        errors=[]
        try:
            if process is not None and process.poll() is None:
                process.kill();process.wait(timeout=5)
                errors.append('Owned test required cleanup termination; not a successful observation')
            evidence['exited']=process is None or process.poll() is not None
        except BaseException as error:
            errors.append(f'{type(error).__name__}: {error}');evidence['exited']=False
        evidence['cleanupErrors']=errors
        if errors:evidence['ok']=False
        (output/'gate.json').write_text(json.dumps(evidence,indent=2)+'\n',encoding='utf-8')
    return evidence

if __name__=='__main__':main()
