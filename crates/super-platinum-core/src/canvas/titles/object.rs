//! Source first-present card matcher and exact cached-object lookup.
use super::{Error, MountedData, RawMessage, Section};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Kind {
    File,
    Document,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Reference {
    kind: Kind,
    lookup: String,
    file_id: String,
}
impl Reference {
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn lookup(&self) -> &str {
        &self.lookup
    }
    pub fn file_id(&self) -> &str {
        &self.file_id
    }

    pub(super) fn card(section: &Section) -> Result<Option<Self>, Error> {
        // These identify Linkcard content; no write style/permission requirement.
        if section.section_type()? != Some(56) {
            return Err(Error::Unsupported);
        }
        let card = section
            .raw()
            .message(12)?
            .ok_or(Error::Unsupported)?
            .message(50)?
            .ok_or(Error::Unsupported)?;
        let Some(content) = card.message(3)? else {
            return Ok(None);
        };
        for tag in [2, 4, 3, 5, 6, 8, 9, 11, 12] {
            let Some(member) = content.message(tag)? else {
                continue;
            };
            let Some(id) = member.string(1)?.filter(|id| !id.is_empty()) else {
                return Ok(None);
            };
            return match tag {
                2 => Ok(simple_file(id).map(|file| Self {
                    kind: Kind::File,
                    lookup: id.into(),
                    file_id: file.into(),
                })),
                4 => {
                    let Some(file) = id.strip_prefix("sd:").filter(|s| !s.is_empty()) else {
                        return Ok(None);
                    };
                    // Section-reference composition is a separate source branch.
                    if file.contains('/') {
                        return Err(Error::Unsupported);
                    }
                    Ok(Some(Self {
                        kind: Kind::Document,
                        lookup: id.into(),
                        file_id: file.into(),
                    }))
                }
                _ => Err(Error::Unsupported),
            };
        }
        Ok(None)
    }
    pub(super) fn data(&self, payload: &RawMessage) -> Result<Option<MountedData>, Error> {
        let tag = match self.kind {
            Kind::File => 60,
            Kind::Document => 62,
        };
        let mut found = None;
        for record in payload.messages(tag)? {
            if record.string(1)? == Some(self.lookup.as_str()) && found.replace(record).is_some() {
                return Err(Error::Unsupported);
            }
        }
        let Some(raw) = found else { return Ok(None) };
        Ok(Some(match self.kind {
            Kind::File => MountedData::File {
                file_id: self.file_id.clone(),
                name: raw.string(3)?.map(str::to_owned),
                mime_type: raw.string(4)?.map(str::to_owned),
                deleted: raw.boolean(8)?.unwrap_or(false),
                file_not_found: raw.boolean(10)?.unwrap_or(false),
                permalink: raw.string(9)?.map(str::to_owned),
            },
            Kind::Document => MountedData::Document {
                file_id: self.file_id.clone(),
                name: raw.string(2)?.map(str::to_owned),
                rich_text_name: raw.string(10)?.map(str::to_owned),
                should_show_document: raw.boolean(9)?.unwrap_or(false),
                is_deleted: raw.boolean(11)?.unwrap_or(false),
                show_badge: raw.boolean(12)?.unwrap_or(false),
                permalink: raw.string(8)?.map(str::to_owned),
            },
        }))
    }
}
fn simple_file(id: &str) -> Option<&str> {
    let mut parts = id.strip_prefix("sf:")?.split('/');
    let first = parts.next()?;
    let file = parts.next().unwrap_or(first);
    (parts.next().is_none()
        && !file.is_empty()
        && file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')))
    .then_some(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::wire::EncodedField as F;
    fn msg(fields: &[F]) -> RawMessage {
        RawMessage::from_fields(fields).unwrap()
    }
    fn text(tag: u32, s: &str) -> F {
        F::bytes(tag, s.as_bytes()).unwrap()
    }
    fn nested(tag: u32, fields: &[F]) -> F {
        F::message(tag, &msg(fields)).unwrap()
    }
    fn card(arms: &[(u32, &str)]) -> Section {
        Section::from_raw(msg(&[
            text(1, "card"),
            F::uint(9, 56).unwrap(),
            nested(
                12,
                &[nested(
                    50,
                    &[nested(
                        3,
                        &arms
                            .iter()
                            .map(|(tag, id)| nested(*tag, &[text(1, id)]))
                            .collect::<Vec<_>>(),
                    )],
                )],
            ),
        ]))
        .unwrap()
    }
    #[test]
    fn first_present_arm_does_not_fall_through_missing_or_empty() {
        assert_eq!(
            Reference::card(&card(&[(2, ""), (4, "sd:F2")])).unwrap(),
            None
        );
        let r = Reference::card(&card(&[(2, "sf:F1"), (4, "sd:F2")]))
            .unwrap()
            .unwrap();
        assert_eq!(r.kind(), Kind::File);
        assert!(
            r.data(&msg(&[nested(62, &[text(1, "sd:F2")])]))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            Reference::card(&card(&[(3, "sc:C1"), (4, "sd:F2")]))
                .unwrap()
                .unwrap()
                .kind(),
            Kind::Document
        );
        assert_eq!(
            Reference::card(&card(&[(3, "sc:C1")])),
            Err(Error::Unsupported)
        );
    }
    #[test]
    fn section_reference_is_not_silently_rendered_as_root_title() {
        assert_eq!(
            Reference::card(&card(&[(4, "sd:F2/section")])),
            Err(Error::Unsupported)
        );
    }
    #[test]
    fn cached_presence_private_and_unknown_fields_stay_distinct() {
        let r = Reference::card(&card(&[(4, "sd:F2")])).unwrap().unwrap();
        let data = r
            .data(&msg(&[nested(
                62,
                &[
                    text(1, "sd:F2"),
                    text(2, ""),
                    text(10, "malformed"),
                    F::uint(9, 1).unwrap(),
                    text(999, "retained"),
                ],
            )]))
            .unwrap()
            .unwrap();
        assert!(
            matches!(data,MountedData::Document{name:Some(ref n),rich_text_name:Some(ref r),should_show_document:true,..} if n.is_empty() && r=="malformed")
        );
        let record = nested(62, &[text(1, "sd:F2")]);
        assert!(r.data(&msg(&[record.clone(), record])).is_err());
    }
    #[test]
    fn file_lookup_keeps_team_qualified_cache_id() {
        let r = Reference::card(&card(&[(2, "sf:T/F1")])).unwrap().unwrap();
        assert_eq!(r.lookup(), "sf:T/F1");
        assert_eq!(r.file_id(), "F1");
        assert!(
            r.data(&msg(&[nested(60, &[text(1, "sf:F1")])]))
                .unwrap()
                .is_none()
        );
    }
}
