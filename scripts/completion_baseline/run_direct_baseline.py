"""Root-only runner for a prebuilt pure direct-scan example; never builds or opens UI.

Import and validate_data are pure. Only run() launches benchmark processes.
The immutable original receipt is mandatory, and every query/choice/sample remains.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import platform
import time

ROOT = Path(__file__).resolve().parents[2]
VALIDATOR_PATH = ROOT / 'scripts/completion_baseline/run_index_baseline.py'
spec = importlib.util.spec_from_file_location('completion_direct_validator', VALIDATOR_PATH)
VALIDATOR = importlib.util.module_from_spec(spec)
spec.loader.exec_module(VALIDATOR)


def validate_data(data, rounds, seed, mode, expected):
    VALIDATOR.validate(data, rounds, seed)
    if (data.get('experiment'), data.get('mode')) != ('completion-direct-v1', mode):
        raise ValueError('Wrong direct experiment or selected mode')
    if VALIDATOR.ranking(data) != expected:
        raise ValueError('Full identity/label/detail/order differs from immutable original')
    return data


def run(args):
    binary = args.binary.resolve(strict=True)
    reference = args.reference.resolve(strict=True)
    before = VALIDATOR.reference(reference, args.rounds, args.repeats)
    reference_hash = VALIDATOR.digest(reference)
    validator_hash = VALIDATOR.digest(VALIDATOR_PATH)
    expected = VALIDATOR.ranking(before['runs'][0])
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    receipt = {
        'schema': 1, 'experiment': 'completion-direct-v1', 'mode': args.mode, 'ok': False,
        'binary': str(binary), 'sha256_before': VALIDATOR.digest(binary),
        'platform': platform.platform(), 'runner_sha256': VALIDATOR.digest(Path(__file__)),
        'validator_sha256': validator_hash, 'reference': str(reference),
        'reference_sha256': reference_hash, 'reference_binary_sha256': before['sha256_before'],
        'rounds': args.rounds, 'repeats': args.repeats, 'runs': [],
        'limitations': [
            'Pure warmed synthetic model calls; no native input, startup, idle or physical paint claim.',
            'Function-pointer dispatch in both candidate modes; immutable original was compiled separately.',
            'Direct mode delegates channels to unchanged actual legacy; no snapshot/cache lifecycle exists.',
            'Unicode uses exact allocating str::to_lowercase fallback. Admitted rank strings and final rows still allocate.',
            'All cases retained. Pooled rounds are correlated within each process; descriptive p95 only.',
            'Construction, trigger parsing, oracle checks, serialization and returned-row destruction excluded.',
        ],
    }
    try:
        for seed in range(args.repeats):
            started = time.perf_counter()
            process = VALIDATOR.invoke(binary, args.rounds, seed, args.mode, output)
            if process.returncode:
                raise RuntimeError(f'Benchmark exited {process.returncode}; process-{seed} logs retained')
            data = validate_data(json.loads(process.stdout), args.rounds, seed, args.mode, expected)
            data['process_wall_seconds'] = time.perf_counter() - started
            receipt['runs'].append(data)
        receipt['sha256_after'] = VALIDATOR.digest(binary)
        if (receipt['sha256_before'] != receipt['sha256_after']
                or VALIDATOR.digest(reference) != reference_hash
                or VALIDATOR.digest(VALIDATOR_PATH) != validator_hash):
            raise RuntimeError('Binary, original receipt or validator changed during cohort')
        receipt['summary'] = VALIDATOR.summarize(receipt['runs'])
        receipt['original_summary'] = VALIDATOR.summarize(before['runs'])
        receipt['ok'] = True
    except BaseException as error:
        receipt['error'] = str(error)
        raise
    finally:
        (output / 'receipt.json').write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'ok': True, 'mode': args.mode, 'receipt': str(output / 'receipt.json'), 'runs': len(receipt['runs'])}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--mode', choices=['direct', 'legacy'], required=True)
    parser.add_argument('--rounds', type=int, default=12)
    parser.add_argument('--repeats', type=int, default=5)
    args = parser.parse_args()
    if not (1 <= args.rounds <= 100 and 1 <= args.repeats <= 20):
        parser.error('bounded rounds/repeats required')
    run(args)
