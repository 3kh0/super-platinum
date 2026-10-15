// A short native-only admission lease. It never dispatches or routes input.
(() => {
  if (window.superPlatinumProtectedAdmission) return;
  const leases = new Map();
  const owns = lease => lease.field.isConnected
    && document.getElementById(lease.before.id) === lease.field
    && lease.field.dataset.projectionVisit === lease.before.visit
    && lease.field.dataset.draftScope === lease.before.scope
    && lease.field.dataset.protectedAdmission === lease.token;
  function release(token) {
    const lease = leases.get(token);
    if (!lease) return;
    leases.delete(token);
    clearTimeout(lease.timer);
    window.superPlatinumProjection.release(token);
    // Never unlock a replacement node or another admission's field.
    if (lease.field.dataset.protectedAdmission === token) {
      if (lease.field.readOnly === true) lease.field.readOnly = lease.priorReadOnly;
      delete lease.field.dataset.protectedAdmission;
    }
  }
  function capture(id, scope, visit, expected) {
    const result = window.superPlatinumProjection.capture(id, scope, visit, expected);
    if (result.status === 'captured') window.superPlatinumProjection.release(result.token);
    return result;
  }
  function lock(expected) {
    const field = document.getElementById(expected.id);
    if (!field || field.readOnly || field.disabled || field.dataset.protectedAdmission)
      return {status:'unavailable'};
    const result = window.superPlatinumProjection.capture(expected.id, expected.scope, expected.visit, expected.wire);
    if (result.status !== 'captured') return result;
    if (JSON.stringify(result.current) !== JSON.stringify(expected)) {
      window.superPlatinumProjection.release(result.token);
      return {status:'changed', current:result.current};
    }
    const lease = {field, before:result.current, token:result.token,
      priorReadOnly:field.readOnly, until:performance.now()+6000};
    field.dataset.protectedAdmission = lease.token;
    field.readOnly = true;
    leases.set(lease.token, lease);
    lease.timer = setTimeout(() => release(lease.token), 6000);
    return result;
  }
  function clear(token) {
    const lease = leases.get(token);
    if (!lease || !owns(lease) || !lease.field.readOnly || performance.now() >= lease.until) {
      release(token); return {status:'unavailable'};
    }
    // One JS turn: enable the existing strict projector just for its exact
    // counter/selection check, then keep the lease through Rust's paired ACK.
    lease.field.readOnly = false;
    try {
      return window.superPlatinumProjection.apply(token,
        {wire:'', start:0, end:0, direction:'forward'}, {});
    } finally {
      if (owns(lease)) lease.field.readOnly = true;
    }
  }
  window.superPlatinumProtectedAdmission = Object.freeze({capture, lock, clear, release});
})();
