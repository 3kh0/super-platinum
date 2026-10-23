"""Closed native Canvas root, preview and reply selected-map lifecycle."""
import json
import secrets
import argparse
import os
from pathlib import Path
import canvas_native
import canvas_readiness
import canvas_comment_emoji as oracle

SPEC = canvas_native.CanvasCheck(fixture=oracle.PREFIX, flag=oracle.FLAG, required=(),
    counts={'requestCount': 2}, stages=oracle.STAGES, readiness=True,
    private_files=(('SUPER_PLATINUM_CANVAS_REPLY_DB', 'replies.sqlite'),))
NARROW_SPEC = canvas_native.CanvasCheck(fixture=oracle.PREFIX, flag=oracle.FLAG, required=(),
    counts={'requestCount': 2}, stages=oracle.capture_stages(oracle.FIXTURES[1]), readiness=True,
    private_files=SPEC.private_files)
_validate, _context, _closed = canvas_native.validate, canvas_native.context_valid, canvas_native.validate_closed
_prepare = canvas_readiness.prepare


def paired(spec, fixture):
    return (spec is SPEC and fixture == oracle.FIXTURES[0]) or (spec is NARROW_SPEC and fixture == oracle.FIXTURES[1])


def context(spec, fixture, receipt):
    if spec is SPEC or spec is NARROW_SPEC:
        return paired(spec, fixture) and oracle.context(receipt)
    return _context(spec, fixture, receipt)


def prepare(spec, fixture, directory, checks):
    if spec is not SPEC and spec is not NARROW_SPEC:
        return _prepare(spec, fixture, directory, checks)
    oracle.require(paired(spec, fixture) and checks.get(oracle.FLAG) == '1' and canvas_readiness.NONCE not in checks,
                   'Unexpected readiness owner')
    owner = dict(version=1, nonce=secrets.token_hex(16), fixture=fixture)
    for name in ('reader-ready.json', 'reader-ready.pending', 'reader-owner.json'):
        oracle.require(not (directory / name).exists(), 'Readiness files must be fresh')
    with (directory / 'reader-owner.json').open('x', encoding='utf-8') as output:
        output.write(json.dumps(owner) + '\n')
    checks[canvas_readiness.NONCE] = owner['nonce']
    return owner


def validate(spec, fixture, receipt):
    oracle.require(paired(spec, fixture), 'Unexpected Canvas emoji check')
    _validate(spec, fixture, receipt)
    oracle.validate(receipt, fixture)


def closed(spec, fixture, log, stages, terminal):
    _closed(spec, fixture, log, stages, terminal)
    oracle.require([{k:v for k,v in s.items() if k != 'screenshot'} for s in stages] == terminal.get('stages'),
                   'Original closed staged/terminal receipts differ')


if __name__ == '__main__':
    canvas_native.context_valid = context
    canvas_native.validate = validate
    canvas_native.validate_closed = closed
    canvas_readiness.prepare = prepare
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path(os.environ.get('SUPER_PLATINUM_BINARY', canvas_native.DEFAULT_BINARY)))
    parser.add_argument('--captures', type=Path, default=Path(os.environ.get('SUPER_PLATINUM_UI_CAPTURE_DIR', canvas_native.ROOT / 'tmp/agent-ui')))
    parser.add_argument('--fixture', action='append', choices=oracle.FIXTURES)
    args = parser.parse_args()
    for fixture in args.fixture or oracle.FIXTURES:
        canvas_native.run(SPEC if fixture == oracle.FIXTURES[0] else NARROW_SPEC, fixture, args.binary.resolve(), args.captures.resolve())
