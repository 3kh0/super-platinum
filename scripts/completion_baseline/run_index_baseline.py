"""Run a prebuilt pure completion example. Never builds or launches the GUI.

Only root should invoke --binary. Each process measures one path selected by --mode.
The required --reference is the complete immutable original-model receipt.
Every process's stdout/stderr and result order are retained. The benchmark binary
is hashed before and after the cohort; an interrupted/changed cohort is not valid.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import platform
import statistics
import subprocess
import time

QUERIES = ['@', '@a', '@al', '@alex', '@alex.000001', '@kim', '@morgan', '@nguyen',
           '@engineering', '@here', '@everyone', '@zzzzzzmissing', '@josé', '@田中',
           '#', '#p', '#proj', '#project', '#project-000001', '#platform', '#support',
           '#esign', '#zzzzzzmissing', '#日本語']
SIZES = {2000, 10000, 64000}


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def validate(data, rounds, seed):
    if (data.get('schema'), data.get('corpus'), data.get('rounds'), data.get('seed'),
            data.get('warmup_rounds')) != (1, 'completion-directory-v1', rounds, seed, 2):
        raise ValueError('Wrong benchmark protocol or workload')
    inventories = data.get('inventories', [])
    if len(inventories) != 3 or {i['total'] for i in inventories} != SIZES:
        raise ValueError('Missing/duplicate inventory')
    for inventory in inventories:
        cases = inventory['cases']
        if [c['query'] for c in cases] != QUERIES:
            raise ValueError('Wrong query workload/order')
        n = inventory['total']
        counts = inventory['counts']
        if (counts['users'], counts['groups'], counts['channels']) != (n*7//8, n//160, n-n*7//8-n//160):
            raise ValueError('Wrong inventory distribution')
        for case in cases:
            values = case['samples_ms']
            if (len(values) != rounds or any(type(v) not in (int, float) or not math.isfinite(v) or v < 0 for v in values)):
                raise ValueError('Missing or invalid duration')
            if len(case['expected']) > 8:
                raise ValueError('Unbounded completion results')
            if case['kind'] != ('person' if case['query'].startswith('@') else 'channel'):
                raise ValueError('Wrong completion kind')
    return data


def ranking(data):
    return {f"{i['total']}:{c['query']}": c['expected']
            for i in data['inventories'] for c in i['cases']}


def stats(values):
    ordered = sorted(values)
    return {'n': len(values), 'median_ms': statistics.median(values),
            'p95_ms': ordered[math.ceil(.95*len(values))-1],
            'min_ms': ordered[0], 'max_ms': ordered[-1],
            'sample_stdev_ms': statistics.stdev(values) if len(values) > 1 else None}


def summarize(runs):
    result = {}
    for run in runs:
        for inventory in run['inventories']:
            for case in inventory['cases']:
                key = f"{inventory['total']}:{case['query']}"
                result.setdefault(key, []).append(case['samples_ms'])
    return {key: {'pooled': stats([x for process in processes for x in process]),
                  'per_process': [stats(x) for x in processes],
                  'process_medians': stats([statistics.median(x) for x in processes])}
            for key, processes in sorted(result.items())}


def reference(path, rounds=None, repeats=None):
    data = json.loads(path.read_text(encoding='utf-8'))
    if (data.get('ok') is not True or data['sha256_before'] != data['sha256_after']
            or len(data['runs']) != data['repeats'] or data.get('mode') is not None):
        raise ValueError('Reference must be complete original-model cohort')
    if rounds is not None and (data['rounds'],data['repeats']) != (rounds,repeats):
        raise ValueError('Reference repetition mismatch')
    for seed, item in enumerate(data['runs']):
        validate(item, data['rounds'], seed)
        if ranking(item) != ranking(data['runs'][0]):
            raise ValueError('Reference ranking drift')
    return data


def validate_candidate(data, rounds, seed, mode, expected):
    validate(data, rounds, seed)
    if (data.get('experiment'),data.get('mode')) != ('completion-index-v1',mode):
        raise ValueError('Wrong experiment or measured mode')
    if ranking(data) != expected:
        raise ValueError('Exact identity/label/detail/order differs from immutable reference')
    for inventory in data['inventories']:
        lifecycle=inventory.get('lifecycle',[])
        if [x.get('phase') for x in lifecycle] != ['initial','rename','restore']:
            raise ValueError('Missing or reordered lifecycle')
        for item in lifecycle:
            for field in ('capture_ms','build_ms','publication_scan_ms','retire_ms'):
                v=item.get(field)
                if type(v) not in (int,float) or not math.isfinite(v) or v<0:
                    raise ValueError('Invalid lifecycle duration')
            for field in ('snapshot_string_capacity_bytes','index_string_capacity_bytes'):
                if type(item.get(field)) is not int or item[field]<0:
                    raise ValueError('Invalid string capacity')
    return data


def lifecycle_summary(runs):
    groups={}
    for data in runs:
        for inventory in data['inventories']:
            for item in inventory['lifecycle']:
                for field in ('capture_ms','build_ms','publication_scan_ms','retire_ms'):
                    key=f"{inventory['total']}:{item['phase']}:{field}"
                    groups.setdefault(key,[]).append(item[field])
    return {key:stats(values) for key,values in sorted(groups.items())}



def invoke(binary, rounds, seed, mode, out):
    try:
        result=subprocess.run([str(binary),str(rounds),str(seed),mode],
                              stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=300,check=False)
    except subprocess.TimeoutExpired as error:
        (out/f'process-{seed}.stdout').write_bytes(error.stdout or b'')
        (out/f'process-{seed}.stderr').write_bytes(error.stderr or b'')
        raise
    (out/f'process-{seed}.stdout').write_bytes(result.stdout)
    (out/f'process-{seed}.stderr').write_bytes(result.stderr)
    return result


def run(args):
    binary=args.binary.resolve(strict=True)
    ref_path=args.reference.resolve(strict=True)
    before=reference(ref_path,args.rounds,args.repeats)
    reference_hash=digest(ref_path)
    expected=ranking(before['runs'][0])
    out=args.output.resolve()
    out.mkdir(parents=True,exist_ok=False)
    receipt={'schema':1,'experiment':'completion-index-v1','mode':args.mode,'ok':False,
             'binary':str(binary),'sha256_before':digest(binary),'platform':platform.platform(),
             'runner_sha256':digest(Path(__file__)),
             'reference':str(ref_path),'reference_sha256':reference_hash,
             'reference_binary_sha256':before['sha256_before'],
             'rounds':args.rounds,'repeats':args.repeats,'runs':[],
             'limitations':['Pure warmed synthetic model timing, not native input/paint/startup.',
                 'checked includes exact validation plus query; index and scan are attribution only.',
                 'Lifecycle measures copying and CPU construction synchronously in the benchmark; no task scheduling or UI integration.',
                 'Old index retained while replacement is built; owned UTF-8 capacities exclude Vec/allocator/stack, private commit and RSS.',
                 'One lifecycle sample per phase/inventory/process; no warmed-loop lifecycle percentile claim.']}
    try:
        for seed in range(args.repeats):
            started=time.perf_counter()
            process=invoke(binary,args.rounds,seed,args.mode,out)
            if process.returncode:
                raise RuntimeError(f'Benchmark exited {process.returncode}; retained process-{seed} logs')
            data=validate_candidate(json.loads(process.stdout),args.rounds,seed,args.mode,expected)
            data['process_wall_seconds']=time.perf_counter()-started
            receipt['runs'].append(data)
        receipt['sha256_after']=digest(binary)
        if receipt['sha256_after']!=receipt['sha256_before'] or digest(ref_path)!=reference_hash:
            raise RuntimeError('Binary or reference changed during cohort')
        receipt['summary']=summarize(receipt['runs'])
        receipt['lifecycle_summary']=lifecycle_summary(receipt['runs'])
        if args.mode=='checked':
            receipt['original_summary']=summarize(before['runs'])
        receipt['ok']=True
    except BaseException as error:
        receipt['error']=str(error)
        raise
    finally:
        (out/'receipt.json').write_text(json.dumps(receipt,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'ok':True,'mode':args.mode,'receipt':str(out/'receipt.json'),'runs':len(receipt['runs'])}))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--reference',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--mode',choices=['checked','index','scan'],required=True)
    parser.add_argument('--repeats',type=int,default=5)
    parser.add_argument('--rounds',type=int,default=12)
    args=parser.parse_args()
    if not (1<=args.repeats<=20 and 1<=args.rounds<=100):
        parser.error('bounded repeats/rounds required')
    run(args)
