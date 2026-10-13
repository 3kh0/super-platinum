//! CPU-only probe linked to the real desktop-profile core artifact.
use std::{hint::black_box, time::Instant};
use super_platinum_core::slack_lists::LinkValue;

fn main() {
    let cold = Instant::now();
    assert_eq!(
        LinkValue::from_input("example.com", None)
            .unwrap()
            .unwrap()
            .original_url,
        "http://example.com"
    );
    println!("cold_ms={:.3}", cold.elapsed().as_secs_f64() * 1000.0);
    for (name, input) in [
        ("word_10k", "a".repeat(10_000)),
        (
            "parentheses_10k",
            format!(
                "{}https://example.com{}",
                "(".repeat(5000),
                ")".repeat(5000)
            ),
        ),
        ("numeric_10k", "202-".repeat(2500)),
        ("dot_near_match_10k", "a.".repeat(5000)),
        ("dots_colon_2k", format!("{}:", "a.".repeat(1000))),
        ("dots_colon_10k", format!("{}:", "a.".repeat(5000))),
        ("colon_dots_10k", format!(":{}", "a.".repeat(5000))),
        ("dots_tld_10k", format!("{}com", "a.".repeat(5000))),
        (
            "valid_tld_then_dots",
            format!("example.com {}", "a.".repeat(5000)),
        ),
        (
            "www_then_dots",
            format!("www.example.com {}", "a.".repeat(5000)),
        ),
        ("unicode_16kb", "道😀.".repeat(2000)),
    ] {
        let started = Instant::now();
        let value = black_box(LinkValue::from_input(black_box(&input), None).unwrap());
        println!(
            "{name} input_bytes={} output_bytes={} elapsed_ms={:.3}",
            input.len(),
            value.unwrap().original_url.len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
    let warm = Instant::now();
    for _ in 0..1000 {
        black_box(LinkValue::from_input(black_box("https://example.com/spec"), None).unwrap());
    }
    println!("warm_1000_ms={:.3}", warm.elapsed().as_secs_f64() * 1000.0);
}
