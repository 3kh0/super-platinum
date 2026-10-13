//! Offline projection benchmark: synthetic data only, no Slack session/network.
use serde_json::json;
use std::{hint::black_box, time::Instant};
use super_platinum_core::slack_lists::{Document, RecordsPage, saved_views};

fn main() {
    for scenario in ["number", "date", "board"] {
        for count in [1_000, 10_000] {
            let mut raw = json!({"id":"F_BENCH","mode":"list","list_metadata":{
            "schema":[
                {"id":"C_NAME","key":"name","name":"Item","type":"text"},
                {"id":"C_NUMBER","key":"estimate","name":"Estimate","type":"number"},
                {"id":"C_SELECT","key":"team","name":"Team","type":"select","options":{"format":"multi_select","choices":[
                    {"value":"A","label":"Design"},{"value":"B","label":"Engineering"},{"value":"C","label":"Operations"}]}}
            ],"views":[{"id":"V_BENCH","name":"Grouped estimates","type":"table","position":"1",
                "columns":[{"id":"C_NAME","key":"name","visible":true},{"id":"C_NUMBER","key":"estimate","visible":true},{"id":"C_SELECT","key":"team","visible":true}],
                "filters":[{"key":"estimate","column_id":"C_NUMBER","operator":"greater_than_or_equal","values":[100]}],
                "sorts":[{"key":"estimate","column_id":"C_NUMBER","ascending":false}],
                "grouping":{"group_by":"team","group_by_column_id":"C_SELECT"}}]}});
            if scenario == "board" {
                let view = &mut raw["list_metadata"]["views"][0];
                view["type"] = json!("board");
                view["grouping"]["order"] = json!([{ "select": ["C"] }, { "select": ["A"] }]);
            }
            if scenario == "date" {
                raw["list_metadata"]["schema"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"C_DATE","key":"due","name":"Due date","type":"date"}));
                let view = &mut raw["list_metadata"]["views"][0];
                view["filters"] = json!([{"key":"due","column_id":"C_DATE","operator":"equal","values":["this_week"]}]);
                view["sorts"] = json!([{"key":"due","column_id":"C_DATE","ascending":false}]);
            }
            let doc = Document::parse(raw).unwrap();
            let rows = (0..count).map(|i|json!({"id":format!("R{i}"),"position":i,"fields":[
            {"key":"name","text":format!("Synthetic item {i}")},
            {"key":"estimate","number":[(i*7919)%count]},
            {"key":"due","date":[format!("2026-10-{:02}",i%28+1)],"timestamp":[-1]},
            {"key":"team","select":if i%3==0 {vec!["A","B"]} else if i%3==1 {vec!["B"]} else {vec!["C"]}}
        ]})).collect::<Vec<_>>();
            let records = RecordsPage::parse(json!({"ok":true,"records":rows})).unwrap();
            let view = saved_views::catalog(&doc).unwrap().remove(0);
            let context = saved_views::ProjectionContext {
                editable: true,
                date: Some(
                    saved_views::DateContext::from_local_with_offset(
                        "2026-10-13T12:00:00",
                        None,
                        -14400,
                    )
                    .unwrap(),
                ),
                date_uses_timestamps: Some(false),
                ..Default::default()
            };
            let mut samples = Vec::new();
            let mut matched = 0;
            for iteration in 0..25 {
                let start = Instant::now();
                let result = black_box(
                    saved_views::project(
                        black_box(&doc),
                        black_box(&records),
                        black_box(&view),
                        black_box(&context),
                    )
                    .unwrap(),
                );
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                matched = result.unique_record_count;
                if iteration >= 5 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "{}",
                json!({"scenario":scenario,"synthetic_records":count,"matched":matched,"runs":samples.len(),"median_ms":samples[samples.len()/2],"p95_ms":samples[(samples.len()*95/100).min(samples.len()-1)]})
            );
        }
    }
}
