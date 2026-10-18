(async function captureProfileDescriptionBridge(capture, visit, serial, key, dioxus) {
  const ticket = capture(visit, serial, key);
  if (!ticket) {
    try { dioxus.send(null); } catch {}
    return;
  }
  let sent = false;
  try {
    dioxus.send(ticket.captured);
    sent = true;
    // A dropped Rust task must not keep the surviving editor sealed forever.
    // Rust's strictly checked two-second deadline precedes the five-second
    // native watchdog. After send, bridge failure must NOT release early:
    // the host may already hold the captured snapshot.
    const ack = await Promise.race([dioxus.recv(), ticket.expired]);
    if (ack === true) ticket.release();
    else if (ack !== 'expired') await ticket.expired;
  } catch {
    if (sent) await ticket.expired;
    else ticket.fail();
  } finally {
    ticket.release();
  }
})
