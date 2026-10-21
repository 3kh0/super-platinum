"""Synthetic receipt rejection tests; these do not establish native acceptance."""
import copy
import unittest

from test_workflow_activity_native import R, DIRECTORY, channel_sample


class SpecialChannelTests(unittest.TestCase):
    def validate(self, fixture, receipt):
        return R['validate'](fixture, receipt, DIRECTORY, 'channel-special',
                             platform='win32', png=False)

    def reject(self, stage, change):
        fixture, receipt = channel_sample(special=True)
        change(next(row for row in receipt['stages'] if row['stage'] == stage))
        with self.assertRaises(RuntimeError):
            self.validate(fixture, receipt)

    def test_complete_both_widths(self):
        for narrow in (False, True):
            self.validate(*channel_sample(narrow, special=True))

    def test_exact_eighteen_stages_and_four_returns(self):
        _, receipt = channel_sample(special=True)
        self.assertEqual(len(receipt['stages']), 18)
        returns = [row for row in receipt['stages'] if row['stage'] in (
            'returned', 'archived-return', 'private-archived-return', 'final-return')]
        self.assertEqual([len(row['proof']['channel']['ledger']['aboutRequests'])
                          for row in returns], [4, 8, 12, 16])

    def test_wrong_private_icon(self):
        self.reject('detail', lambda r: r['proof']['channelLabels']['rows'][0].update(iconKind='Tag'))

    def test_archived_precedes_private(self):
        for wrong in ('LockFilled', 'Archive', 'Tag', None):
            with self.subTest(wrong=wrong):
                self.reject('detail', lambda r: r['proof']['channelLabels']['rows'][4].update(iconKind=wrong))

    def test_public_archive_not_filled(self):
        self.reject('detail', lambda r: r['proof']['channelLabels']['rows'][3].update(iconKind='ArchiveFilled'))

    def test_actual_svg_path_required(self):
        self.reject('detail', lambda r: r['proof']['channelLabels']['rows'][4]['icon'].update(pathMatches=False))

    def test_exact_archived_suffix_and_no_hash(self):
        for index, name in ((3, 'activity-archived'), (4, '#activity-private-archived (archived)'),
                            (0, '#activity-known'), (3, 'activity-archived (formerly old)')):
            with self.subTest(index=index, name=name):
                self.reject('detail', lambda r: r['proof']['channelLabels']['rows'][index].update(text=name, name=name))

    def test_no_fake_missing_icon(self):
        self.reject('offline', lambda r: r['proof']['channelLabels']['rows'][2].update(iconKind='Tag'))

    def test_fetched_ordinary_tag_preserved(self):
        self.reject('final-return', lambda r: r['proof']['exercise']['reentryLabels']['rows'][2].update(iconKind='LockFilled'))

    def test_all_five_detail_occurrences_required(self):
        self.reject('detail', lambda r: r['proof']['exercise']['rows'].pop())

    def test_duplicate_original_ids(self):
        self.reject('detail', lambda r: r['proof']['channelLabels']['rows'][4].update(id='workflow-activity-channel-step-1-1'))

    def test_all_five_composition_occurrences(self):
        self.reject('composition-refused', lambda r: r['proof']['exercise']['inert']['rows'].pop())

    def test_original_inert_archive_identity(self):
        self.reject('composition-refused', lambda r: r['proof']['exercise']['inert']['rows'][4].update(originalChannel='CACTARCH01'))
        self.reject('composition-refused', lambda r: r['proof']['exercise']['inert']['rows'][3].update(container='workflow-activity-step-message-0'))

    def test_inert_archive_not_interactive(self):
        self.reject('composition-refused', lambda r: r['proof']['exercise']['inert']['rows'][4].update(buttonPresent=True))

    def test_restored_archive_label_guard(self):
        self.reject('composition-refused', lambda r: r['proof']['exercise']['restored']['labels']['rows'][3].update(iconKind='Tag'))

    def test_archive_composer_absence_required(self):
        for change in (dict(noComposer=False), dict(composerUnavailable=False),
                       dict(archivedReason=''), dict(placeholder='Message #activity-archived'),
                       dict(composerScope='successor')):
            with self.subTest(change=change):
                self.reject('archived', lambda r: r['geometry'].update(change))

    def test_archive_owner_composer_must_be_explicit_null(self):
        self.reject('private-archived', lambda r: r['proof']['conversationOwner'].update(composer='foreign'))
        self.reject('private-archived', lambda r: r['proof']['conversationOwner'].pop('composer'))

    def test_archive_status_must_have_actual_geometry(self):
        self.reject('archived', lambda r: r['geometry']['controls'].pop())
        self.reject('archived', lambda r: r['geometry']['controls'][-1].update(visible=False))

    def test_target_header_is_raw_without_suffix(self):
        self.reject('archived', lambda r: r['geometry'].update(headerText='#activity-archived (archived)'))

    def test_target_timeline_has_original_owner(self):
        self.reject('private-archived', lambda r: r['proof']['conversationOwner'].update(timeline='foreign'))

    def test_local_click_original_button(self):
        self.reject('private-archived', lambda r: r['proof']['exercise'].update(button='workflow-activity-channel-step-1-1'))

    def test_local_click_no_network_or_draft_mutation(self):
        self.reject('archived', lambda r: r['proof']['exercise']['before']['ledger'].update(completed=1))
        self.reject('archived', lambda r: r['proof']['exercise']['before']['documents'][0].update(description='other'))

    def test_four_fresh_about_returns(self):
        self.reject('private-archived-return', lambda r: r['proof']['channel']['ledger']['aboutRequests'].pop())
        self.reject('archived-return', lambda r: r['proof']['channel']['ledger'].update(aboutRequests=[]))

    def test_no_new_missing_reads_for_archives(self):
        self.reject('archived', lambda r: r['proof']['channel']['ledger']['requests'].append(
            dict(method='conversations.info', args={'channel':'CACTARCH01','include_num_members':'false'})))

    def test_archive_not_forced_into_inactive_sidebar(self):
        self.reject('detail', lambda r: r['proof']['channel']['sidebar']['rows'].append(
            dict(id='CACTARCH01', name='activity-archived', index=99, section='channels', active=False)))

    def test_current_archive_must_appear_in_sidebar(self):
        self.reject('archived', lambda r: r['proof']['channel']['sidebar']['rows'].pop())

    def test_archived_source_always_retained(self):
        self.reject('about', lambda r: r['proof']['channel']['sourceRows'].pop())

    def test_initial_source_flags_not_self_attested(self):
        fixture, receipt = channel_sample(special=True)
        for row in receipt['stages']:
            for source in row['proof']['channel']['sourceRows']:
                if source['id'] == 'CACTPRARCH01':
                    source['isArchived'] = False
        with self.assertRaises(RuntimeError):
            self.validate(fixture, receipt)

    def test_source_flag_types_and_unknown_fields(self):
        for change in (dict(isPrivate=1), dict(isGroup=True), dict(isArchived=False), dict(isFrozen=False)):
            with self.subTest(change=change):
                self.reject('about', lambda r: r['proof']['channel']['sourceRows'][-1].update(change))

    def test_missing_source_flag(self):
        self.reject('about', lambda r: r['proof']['channel']['sourceRows'][0].pop('isGroup'))

    def test_reentry_preserves_original_native_draft(self):
        self.reject('archived-return', lambda r: r['geometry']['fields'].update(description='Lost draft'))
        self.reject('private-archived-return', lambda r: r['proof']['channel']['documents'][0].update(id=999))

    def test_ordinary_case_unchanged_both_widths(self):
        for narrow in (False, True):
            fixture, receipt = channel_sample(narrow)
            original = copy.deepcopy(receipt)
            R['validate'](fixture, receipt, DIRECTORY, 'channel', platform='win32', png=False)
            self.assertEqual(receipt, original)

    def test_ordinary_rejects_special_source_fields(self):
        fixture, receipt = channel_sample()
        receipt['stages'][0]['proof']['channel']['sourceRows'][0]['isPrivate'] = False
        with self.assertRaises(RuntimeError):
            R['validate'](fixture, receipt, DIRECTORY, 'channel', platform='win32', png=False)

    def test_special_validation_does_not_transform_receipts(self):
        fixture, receipt = channel_sample(special=True)
        original = copy.deepcopy(receipt)
        self.validate(fixture, receipt)
        self.assertEqual(receipt, original)


if __name__ == '__main__':
    unittest.main()

