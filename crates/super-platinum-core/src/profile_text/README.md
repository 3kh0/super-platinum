# Captured profile-text rules

These are public standard emoji and reserved-name data, plus synthetic test strings;
no workspace content, identifiers, credentials, or live profile values are included.

Source is the existing local Slack bundle
`tmp/sb/js/gantry-v2-shared.0b81595876785d7a.min.js`:

- `ehbb`, byte 4886732: name/display-name/pronouns validators and their ordering.
- `ZOnG._w`, byte 753866: all three fields have an 80 UTF-16-unit limit.
- `ZOnG.xt`, byte 757099: exactly 99 reserved values (including source duplicates).
- `zllF`, byte 68698: default emoji catalog, 3786 unique native glyph sequences,
  including `skinVariations[*].unicode`. Deduplicating does not alter presence checks.
- `7YoG`, byte 4295033: native emoji alternation, longest UTF-16 sequences first;
  a non-Unicode regexp rejects a match followed by ZWJ or the skin-modifier
  character-class code units. `ehbb` calls this with undefined workspace; it uses
  `tCC+`'s default catalog, not the `emoji_v17` experiment catalog.
- `628N`, `eVj1`, `KRDC`: JS/Lodash trimmed required/length/restricted-character
  checks. Leading-at and punctuation-only checks retain the original string.

`rguE` at byte 15164190 of
`gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js`
normalizes text-field changes with `String.normalize()` (NFC). It does not trim
submitted text. The custom username policy is displayed as workspace rules at
15165238, rather than interpreted as a client regex.

`oracle.json` is an offline Node synthetic oracle: the actual captured `ehbb`
function is evaluated with the captured limits/reserved data, equivalent basic
Lodash trim/compact validation adapters, and the exact `7YoG` regexp reconstructed
from the captured glyphs. It contains 675 field cases and 1725 regexp cases,
including Unicode composition, UTF-16 limits, empty/whitespace, every reserved
word, sampled native glyphs, and ZWJ/skin-modifier/flag suffixes. The Rust trie is
compared against the JavaScript regexp's boolean match result, not a broad
Unicode emoji category. All captured glyphs are additionally tested separately.

This helper does not establish edit permissions, parse organization policy,
write profiles, or implement title/profile-section editing. Those require the
separately captured metadata and authenticated mutation/readback lifecycle.
