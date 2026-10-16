#!/usr/bin/env python3
"""Root-only publisher after reading the actual observed PNG; never selects controls.

Pass explicit zero-based target/filename indexes from the observed controls and
the SHA-256 of the PNG root has just read. This is review evidence, not a claim
that a script can establish visual correctness or human consent.
"""
import argparse
from pathlib import Path
from native_fixture import ROOT
from cached_save_contracts import ACTIONS, role, path_index, digest, read_json, require, save, validate_review, unlinked


def publish(observed_path, target_index, image_sha256, filename_index=None):
    path = Path(observed_path).resolve(strict=True)
    directory = path.parent
    observed = read_json(path)
    stage = observed['stage']
    require(stage in ACTIONS and path.name == f'{stage}-observed.json', 'exact review observation name')
    nonce = observed['probe']['nonce']
    require(directory == (ROOT / 'tmp' / f'cached-viewer-save-{nonce}').resolve(strict=True), 'owned review directory')
    for parent in [directory, *directory.parents]:
        unlinked(parent)
    require(observed['allowedPaths'] == [str(directory / 'downloads' / f'{prefix}-{nonce}.txt') for prefix in ('overwrite', 'retire')], 'exact nonce-owned paths')
    image = Path(observed['image'])
    require(image.parent == directory and image.suffix == '.png', 'owned review image')
    unlinked(image)
    require(digest(image) == image_sha256 == observed['imageSha256'], 'exact PNG root reviewed')
    probe = observed['probe']; controls = probe['controls']
    require(type(target_index) is int and 0 <= target_index < len(controls), 'explicit observed target index')
    request = {key: probe[key] for key in ('nonce', 'pid', 'creation')}
    request.update(mode='set-path' if stage.startswith('set-') else 'invoke', kind=probe['kind'],
                   hwnd=probe['window']['hwnd'], window=probe['window'], target=controls[target_index])
    if stage.startswith('set-') or stage.startswith('save-'):
        request['allowedPath'] = observed['allowedPaths'][path_index(stage)]
    if not stage.startswith('set-'):
        request['role'] = role(stage)
    if stage.startswith('save-'):
        require(type(filename_index) is int and 0 <= filename_index < len(controls), 'explicit observed filename index')
        request['filenameTarget'] = controls[filename_index]
    else:
        require(filename_index is None, 'filename index only for Save')
    if probe['kind'] == 'overwrite':
        request['saveHwnd'] = observed['saveHwnd']
    observed['_sha256'] = digest(path)
    review = dict(version=1, stage=stage, observedSha256=observed['_sha256'], imageSha256=image_sha256, request=request)
    validate_review(review, observed)
    destination = directory / f'{stage}-review.json'
    # Publish atomically and exclusively; runner never observes a partial review.
    import os
    temporary = directory / f'{stage}-review.staged'
    save(temporary, review)
    try:
        os.link(temporary, destination)
    finally:
        temporary.unlink()
    return destination


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--observed', required=True, type=Path)
    parser.add_argument('--target-index', required=True, type=int)
    parser.add_argument('--filename-index', type=int)
    parser.add_argument('--reviewed-image-sha256', required=True)
    args = parser.parse_args()
    print(publish(args.observed, args.target_index, args.reviewed_image_sha256, args.filename_index))
