"""Prove the earlier Rust composition gate without inventing a native refusal."""


def validate(exercise, model, ledger, native, viewport, require, rectangle):
    e = exercise
    require(e.get('mode') == 'rust-composition-guard'
            and e.get('composition') is True and e.get('stateComposing') is True
            and e.get('observedMs') == 100, 'Missing actual export composition gate')
    require(e.get('modelBefore') == e.get('modelAfter') == model
            and e.get('ledgerBefore') == e.get('ledgerAfter') == ledger,
            'Composition export changed model or dispatched')
    before, after = e.get('nativeBefore', {}), e.get('nativeAfter', {})
    require(before == after and before.get('ok') is True and before.get('sameNode') is True
            and before.get('composing') is True
            and before.get('value') == '' and type(before.get('refusals')) is int
            and before['refusals'] >= 0 and isinstance(before.get('notice'), str),
            'Composition export changed native buffer, refusal count or notice')
    require((native.get('nativeNotice') or {}).get('text', '') == before['notice'],
            'Composition capture changed existing feedback')
    click = e.get('click', {})
    require(click.get('ok') is True and click.get('id') == 'workflow-activity-export'
            and click.get('tag') == 'BUTTON' and click.get('text') == 'Export CSV'
            and click.get('sameNode') is True and click.get('visible') is True
            and click.get('hit') is True and click.get('disabled') is False
            and click.get('viewport') == viewport, 'Missing original visible export activation')
    rectangle(click.get('rect'), viewport)
    restored = e.get('restored', {})
    require(restored.get('stateComposing') is False and restored.get('model') == model
            and restored.get('ledger') == ledger, 'Export composition end changed custody')
    require(restored.get('native') == {**before, 'composing': False},
            'Export composition end lost original native state')
