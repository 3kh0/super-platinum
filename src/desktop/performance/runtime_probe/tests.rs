use super::*;
use serde_json::json;

fn sample() -> serde_json::Value {
    json!({
        "phase": "timer", "elapsed_ms": 1000, "ready": "complete",
        "visibility": "hidden", "focused": false, "interpreter": true,
        "headless": true, "socket_state": 1, "queued_batches": 0,
        "root_children": 1, "raf_callbacks": 0,
        "socket_observer_generation": null, "socket_observer_since_ms": null,
        "socket_messages": null, "socket_opens": null, "socket_closes": null,
        "socket_errors": null, "socket_buffered_bytes": null,
        "interpreter_nodes": null, "interpreter_stack": null,
        "script_errors": 0, "resource_errors": 0, "unhandled_rejections": 0
    })
}

#[test]
fn accepts_structural_sample_without_losing_unknown_interpreter_state() {
    let mut value = sample();
    value["headless"] = serde_json::Value::Null;
    value["socket_state"] = serde_json::Value::Null;
    let decoded = decode(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
}

#[test]
fn rejects_unlisted_fields_strings_and_out_of_bound_payloads() {
    for (key, value) in [
        ("url", json!("https://example.invalid/private")),
        ("ready", json!("untrusted content")),
        ("phase", json!("unknown")),
        ("socket_state", json!(4)),
        ("queued_batches", json!(-1)),
        ("socket_observer_generation", json!(17)),
        ("socket_observer_generation", json!(0)),
        ("socket_observer_since_ms", json!(1001)),
        ("socket_messages", json!(1000001)),
        ("socket_opens", json!(-1)),
        ("socket_closes", json!("private reason")),
        ("socket_errors", json!(1000001)),
        ("socket_buffered_bytes", json!(1000001)),
        ("interpreter_nodes", json!(1000001)),
        ("interpreter_stack", json!(-1)),
        ("elapsed_ms", json!(20001)),
        ("root_children", json!(1000001)),
        ("raf_callbacks", json!(3)),
        ("script_errors", json!(1000001)),
        ("resource_errors", json!(-1)),
        ("unhandled_rejections", json!("private error")),
    ] {
        let mut candidate = sample();
        candidate[key] = value;
        assert!(
            decode(&serde_json::to_vec(&candidate).unwrap()).is_none(),
            "{key}"
        );
    }
    assert!(decode(&vec![b' '; 1025]).is_none());
}

fn probe() -> Probe {
    Probe {
        started: Instant::now(),
        native_events: AtomicU64::new(0),
        user_events: AtomicU64::new(0),
        window_events: AtomicU64::new(0),
        focus: AtomicU64::new(0),
        heartbeats: AtomicU64::new(0),
        last_heartbeat: AtomicU64::new(0),
        js_samples: AtomicU64::new(0),
    }
}

fn request(method: Method, path: &str) -> Request<Vec<u8>> {
    Request::builder()
        .method(method)
        .uri(path)
        .body(serde_json::to_vec(&sample()).unwrap())
        .unwrap()
}

#[test]
fn protocol_is_bounded_and_ignores_wrong_route_method_and_expired_samples() {
    let mut probe = probe();
    receive(&probe, request(Method::GET, "/sample"));
    receive(&probe, request(Method::POST, "/other"));
    assert_eq!(probe.js_samples.load(Ordering::Relaxed), 0);
    // Begin just below the cap so this test prints one whitelisted sample.
    probe.js_samples.store(MAX_SAMPLES - 1, Ordering::Relaxed);
    receive(&probe, request(Method::POST, "/sample"));
    receive(&probe, request(Method::POST, "/sample"));
    assert_eq!(probe.js_samples.load(Ordering::Relaxed), MAX_SAMPLES);
    probe.started = Instant::now() - LIMIT;
    probe.js_samples.store(0, Ordering::Relaxed);
    receive(&probe, request(Method::POST, "/sample"));
    assert_eq!(probe.js_samples.load(Ordering::Relaxed), 0);
}

#[test]
fn passive_socket_numbers_are_optional_and_bounded() {
    let mut value = sample();
    for key in [
        "socket_messages",
        "socket_opens",
        "socket_closes",
        "socket_errors",
        "socket_buffered_bytes",
        "interpreter_nodes",
        "interpreter_stack",
    ] {
        value[key] = json!(1_000_000);
    }
    value["socket_observer_generation"] = json!(16);
    value["socket_observer_since_ms"] = json!(1000);
    assert!(decode(&serde_json::to_vec(&value).unwrap()).is_some());
    let object = value.as_object_mut().unwrap();
    for key in [
        "socket_observer_generation",
        "socket_observer_since_ms",
        "socket_messages",
        "socket_opens",
        "socket_closes",
        "socket_errors",
        "socket_buffered_bytes",
        "interpreter_nodes",
        "interpreter_stack",
    ] {
        object.remove(key);
    }
    assert!(decode(&serde_json::to_vec(&value).unwrap()).is_some());
}
