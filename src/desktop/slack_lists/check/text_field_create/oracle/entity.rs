//! Independent literal expected request/readback options, not the fixture builder.
use super::*;
use crate::slack_lists::rename::fixture::EntityCase;
pub(super) fn options(case: EntityCase) -> Value {
    match case {
        EntityCase::PeopleSingle => {
            json!({"format":"single_entity","show_member_name":false,"default_value":"U00000002|U00000001","default_value_typed":{"user":["U00000002","U00000001"]}})
        }
        EntityCase::PeopleMultiple => {
            json!({"format":"multi_entity","show_member_name":false,"default_value":"U00000002|U00000001","default_value_typed":{"user":["U00000002","U00000001"]}})
        }
        EntityCase::PeopleNull => {
            json!({"format":"single_entity","show_member_name":false,"default_value":null})
        }
        EntityCase::PeopleOmitted => json!({"format":"multi_entity","show_member_name":false}),
        EntityCase::ChannelSingle => {
            json!({"format":"single_entity","show_member_name":true,"default_value":"G00000002|C00000001","default_value_typed":{"channel":["G00000002","C00000001"]}})
        }
        EntityCase::ChannelMultiple => {
            json!({"format":"multi_entity","show_member_name":true,"default_value":"G00000002|C00000001","default_value_typed":{"channel":["G00000002","C00000001"]}})
        }
        EntityCase::ChannelNull => {
            json!({"format":"single_entity","show_member_name":true,"default_value":null})
        }
        EntityCase::ChannelOmitted => json!({"format":"multi_entity","show_member_name":true}),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn each_literal_case_requires_one_exact_create_no_replay_or_normalization() {
        for case in EntityCase::ALL {
            let mode = FieldCreateMode::Entity(case);
            assert_eq!(options(case), case.options());
            let docs = documents(
                &json!({"id":"F_LIST_FIXTURE","list_metadata":{"schema":[{"id":"old"}],"views":[{"id":"V_FILTER","columns":[{"id":"old"}]}]}}),
                mode,
            );
            let ledger = json!({"requests":requests(mode),"responses":responses(&docs),"finished":completions(9),"server":docs[1],"writePending":false,"readPending":false});
            exact(&ledger, 11, 4, false, false, 1, &docs, 9, mode).unwrap();
            assert_eq!(
                ledger["requests"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|r| r["path"] == "/api/lists.columns.create")
                    .count(),
                1
            );
            for branch in 0..4 {
                let mut wrong = ledger.clone();
                match branch {
                    0 => wrong["requests"][3]["fields"][5][1] = json!("{}"),
                    1 => {
                        let duplicate = wrong["requests"][3].clone();
                        wrong["requests"].as_array_mut().unwrap().push(duplicate);
                    }
                    2 => wrong["finished"][2]["applied"] = json!(true),
                    _ => {
                        wrong["server"]["list_metadata"]["schema"][1]["options"]["default_value"] =
                            json!("normalized")
                    }
                }
                assert!(exact(&wrong, 11, 4, false, false, 1, &docs, 9, mode).is_err());
            }
        }
    }
}
