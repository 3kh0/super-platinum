"""Strict acceptance for channel occurrences made inert by member composition."""


def validate(exercise, model, channel, viewport, require, rectangle, labels, *, expected=None):
    require(isinstance(exercise, dict), 'Missing composition exercise')
    e = exercise
    require(e.get('kind') == 'composition-refused'
            and e.get('mode') == 'inert-while-composing',
            'Composition must exercise the actual inert occurrence')
    require(e.get('stateComposing') is True and e.get('observedMs') == 100,
            'Missing actual composition state or observation')
    require(e.get('before') == e.get('after') == model
            and e.get('channelBefore') == e.get('channelAfter') == channel,
            'Inert composition changed model or endpoint custody')
    labels(e.get('beforeLabels'), viewport)
    attempt, native = e.get('attempt', {}), e.get('native', {})
    count = attempt.get('refusals')
    require(type(count) is int and count >= 0, 'Missing original refusal count')
    for value in (attempt, native):
        require(value.get('ok') is True and value.get('composing') is True
                and value.get('value') == '' and value.get('refusals') == count,
                'Composition changed native buffer or dispatched a refusal')
    require(native.get('notice') == '', 'Inert occurrence fabricated refusal guidance')
    inert = e.get('inert', {})
    require(inert.get('ok') is True and isinstance(inert.get('detailRevision'), str)
            and inert['detailRevision'].isdigit(), 'Missing original detail revision')
    rows = inert.get('rows')
    expected = expected if expected is not None else [
        ('workflow-activity-trigger-message', 0, 'workflow-activity-channel-trigger-1', 'CACTKNOWN01', 'activity-known'),
        ('workflow-activity-trigger-message', 1, 'workflow-activity-channel-trigger-3', 'CACTKNOWN01', 'activity-known'),
        ('workflow-activity-step-message-0', 0, 'workflow-activity-channel-step-0-1', 'CACTMISS01', 'Channel unavailable'),
    ]
    require(isinstance(rows, list) and len(rows) == len(expected), 'Missing inert occurrences')
    for row, identity in zip(rows, expected):
        require(tuple(row.get(k) for k in ('container', 'occurrence', 'originalButtonId',
                                         'originalChannel', 'text')) == identity,
                'Inert occurrence lost its original identity')
        require(row.get('ok') is True and row.get('tag') == 'SPAN'
                and row.get('buttonPresent') is False and row.get('interactiveAncestor') is False
                and row.get('visible') is True and row.get('hit') is True,
                'Composition occurrence is interactive, hidden or covered')
        rectangle(row.get('rect'), viewport)
    require(inert.get('activation') == dict(ok=True, event='click', occurrence=0, observedMs=100),
            'Missing actual inert activation and observation')
    restored = e.get('restored', {})
    require(restored.get('stateComposing') is False
            and restored.get('model') == model and restored.get('channel') == channel,
            'Composition end did not restore original custody')
    labels(restored.get('labels'), viewport)
    restored_native = restored.get('native', {})
    require(restored_native.get('ok') is True and restored_native.get('composing') is False
            and restored_native.get('value') == '' and restored_native.get('refusals') == count,
            'Composition end lost original native buffer or refusal count')
