use super::*;
fn sample(stage: usize) -> (Value, Value, [Value; 3], Value) {
    let raw = json!({"ok":true,"records":(0..73).map(|i|json!({"id":format!("R_LIST_{i:03}"),"opaque":{"retain":i},"fields":[{"key":"NAME","text":"original"}]})).collect::<Vec<_>>(),"response_metadata":{"next_cursor":"opaque+/="}});
    let p = pages(&raw).unwrap();
    let doc = json!({"id":"F_LIST_FIXTURE","opaque":"unchanged"});
    let sync =
        json!({"creationSerial":7,"visit":10,"selection":11,"owner":[null,"T1","U0"],"epoch":4});
    let attempts=[true,false].map(|value|json!({"serial":if value {20}else{30},"visit":10,"selection":11,"revision":if value {12}else{25},"owner":[null,"T1","U0"],"list":"F_LIST_FIXTURE","row":"R_LIST_003","column":"C_CHECKBOX_OBSERVED","key":"checkbox_observed","cells":[{"row_id":"R_LIST_003","column_id":"C_CHECKBOX_OBSERVED","checkbox":value}]}));
    let (writes, done, reads) = COUNTS[stage];
    let pre=(0..writes).map(|i|json!({"attempt":attempts[i],"document":doc,"population":p[i],"allow":"full","phase":"Saving"})).collect::<Vec<_>>();
    let requests=(0..writes).map(|i|json!({"method":"POST","path":"/api/lists.cells.update","fields":[["token","fixture-not-a-token"],["list_id","F_LIST_FIXTURE"],["cells",attempts[i]["cells"].to_string()],["_x_reason","bulk-update-cells"]],"retrySafe":false,"attempt":attempts[i]})).collect::<Vec<_>>();
    let completed=(0..done).map(|i|json!({"attempt":attempts[i],"result":if i==0 {"response_lost"}else{"positive_ack"},"operation":{"attempt":attempts[i],"phase":"Uncertain"}})).collect::<Vec<_>>();
    let local=(0..reads).map(|i|{
        let before=json!({"attempt":attempts[usize::from(i==2)],"phase":"Uncertain"});
        json!({"readback":([Value::Null,json!(true),json!(false)][i]),"read":15+i as u64,"revision":18+i as u64,"visit":10,"before":before,"document":doc,"population":p[i],"after":{"read":0,"revision":19+i as u64,"operation":if i==0 {before}else{Value::Null}}})
    }).collect::<Vec<_>>();
    let phase = if matches!(stage, 0 | 4) {
        "Saving"
    } else {
        "Uncertain"
    };
    let op = if matches!(stage, 3 | 6) {
        Value::Null
    } else {
        json!({"attempt":attempts[writes-1],"phase":phase})
    };
    let pending = if writes > done {
        attempts[writes - 1].clone()
    } else {
        Value::Null
    };
    let l = json!({"sync":sync,"preflights":pre,"requests":requests,"completed":completed,"reads":local,"missingQueued":false,"attemptCount":writes,"preparedValid":true,"server":p[SERVER[stage]],"pending":pending,"operation":op});
    (l, doc, p, sync)
}
#[test]
fn every_checkpoint_requires_actual_forms_and_missing_read_remains_unresolved() {
    for stage in 0..7 {
        let (l, d, p, s) = sample(stage);
        exact(&l, stage, &d, &p, &s).unwrap();
    }
    let (base, d, p, s) = sample(2);
    for branch in 0..8 {
        let mut bad = base.clone();
        match branch {
            0 => bad["reads"][0]["after"]["operation"] = Value::Null,
            1 => bad["reads"][0]["population"] = p[1].clone(),
            2 => bad["operation"] = Value::Null,
            3 => bad["requests"]
                .as_array_mut()
                .unwrap()
                .push(base["requests"][0].clone()),
            4 => bad["preflights"][0]["population"] = p[1].clone(),
            5 => bad["server"] = p[0].clone(),
            6 => bad["reads"][0]["visit"] = json!(999),
            _ => bad["reads"][0]["after"]["revision"] = bad["reads"][0]["revision"].clone(),
        }
        assert!(exact(&bad, 2, &d, &p, &s).is_err(), "branch {branch}");
    }
}
#[test]
fn final_receipt_rejects_wrong_wire_ownership_ack_or_unrelated_raw_mutation() {
    let (base, d, p, s) = sample(6);
    for branch in 0..10 {
        let mut bad = base.clone();
        match branch {
            0 => bad["requests"][1]["fields"]
                .as_array_mut()
                .unwrap()
                .push(json!(["options", "{}"])),
            1 => bad["requests"][1]["retrySafe"] = json!(true),
            2 => bad["completed"][1]["result"] = json!("response_lost"),
            3 => bad["preflights"][1]["attempt"]["serial"] = json!(20),
            4 => bad["preflights"][1]["attempt"]["owner"] = json!([null, "T1", "OTHER"]),
            5 => bad["reads"][2]["after"]["operation"] = base["reads"][2]["before"].clone(),
            6 => bad["server"]["records"][0]["opaque"] = json!(false),
            7 => bad["reads"][2]["population"] = p[0].clone(),
            8 => bad["missingQueued"] = json!(true),
            _ => bad["attemptCount"] = json!(true),
        }
        assert!(exact(&bad, 6, &d, &p, &s).is_err(), "branch {branch}");
    }
}
#[test]
fn completed_read_consumes_id_but_each_input_read_must_be_positive_and_monotonic() {
    let (base, d, p, s) = sample(6);
    exact(&base, 6, &d, &p, &s).unwrap();
    for index in 0..3 {
        for after in [
            base["reads"][index]["read"].clone(),
            json!(1),
            json!(false),
            Value::Null,
        ] {
            let mut bad = base.clone();
            bad["reads"][index]["after"]["read"] = after;
            assert!(exact(&bad, 6, &d, &p, &s).is_err());
        }
        let mut bad = base.clone();
        bad["reads"][index]["read"] = json!(0);
        assert!(exact(&bad, 6, &d, &p, &s).is_err());
    }
    for index in 1..3 {
        let mut bad = base.clone();
        bad["reads"][index]["read"] = bad["reads"][index - 1]["read"].clone();
        assert!(exact(&bad, 6, &d, &p, &s).is_err());
    }
}
#[test]
fn absent_and_explicit_false_pages_are_distinct_and_present_malformed_is_not_missing() {
    let (_, _, p, _) = sample(6);
    assert_ne!(p[0], p[2]);
    let mut restored = p[2].clone();
    restored["records"][3]["fields"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert_eq!(restored, p[0]);
    for value in [
        json!({"key":"checkbox_observed"}),
        json!({"key":"checkbox_observed","checkbox":null}),
        json!({"column_id":"C_CHECKBOX_OBSERVED","key":"other"}),
    ] {
        let mut malformed = p[0].clone();
        malformed["records"][5]["fields"]
            .as_array_mut()
            .unwrap()
            .push(value);
        assert!(pages(&malformed).is_err());
    }
}
