// Slack client peer_message.proto: PeerMessage.reacji = 1,
// fromPeerIdString = 4; Reacji.emoji = 1, type = 2 (Standard = 2).
// Only these fields are encoded. Unknown peer fields are skipped on receive.
function protoVarint(value) {
  const bytes = [];
  do { const byte = value & 127; value >>>= 7; bytes.push(byte | (value ? 128 : 0)); } while (value);
  return bytes;
}
function protoBytes(field, bytes) { return [...protoVarint((field << 3) | 2), ...protoVarint(bytes.length), ...bytes]; }
function encodeReaction(emoji, attendee) {
  const utf8 = new TextEncoder();
  const reaction = [...protoBytes(1, utf8.encode(emoji)), 16, 2];
  return new Uint8Array([...protoBytes(1, reaction), ...protoBytes(4, utf8.encode(attendee))]);
}
function protoFields(bytes) {
  bytes = new Uint8Array(bytes);
  let offset = 0;
  const result = new Map();
  function read() {
    let value = 0, shift = 0;
    for (let count = 0; count < 10; count++) {
      if (offset >= bytes.length) throw new Error('Truncated protobuf');
      const byte = bytes[offset++];
      if (shift < 32) value |= (byte & 127) << shift;
      if (!(byte & 128)) return value >>> 0;
      shift += 7;
    }
    throw new Error('Invalid protobuf varint');
  }
  while (offset < bytes.length) {
    const tag = read(), field = tag >>> 3, wire = tag & 7;
    if (!field) throw new Error('Invalid protobuf field');
    if (wire === 0) { read(); continue; }
    if (wire === 1 || wire === 5) { offset += wire === 1 ? 8 : 4; }
    else if (wire === 2) {
      const length = read(), end = offset + length;
      if (end > bytes.length) throw new Error('Truncated protobuf field');
      result.set(field, bytes.subarray(offset, end));
      offset = end;
    } else throw new Error('Unsupported protobuf wire type');
    if (offset > bytes.length) throw new Error('Truncated protobuf');
  }
  return result;
}
function decodeReaction(bytes) {
  if (!bytes || bytes.length > 2048) return null;
  const message = protoFields(bytes).get(1);
  if (!message) return null;
  const emoji = protoFields(message).get(1);
  if (!emoji || emoji.length > 128) return null;
  return new TextDecoder('utf-8', { fatal: true }).decode(emoji);
}
function sendReaction(s, emoji) {
  if (!isCurrent(s) || !['thumbsup', 'heart', 'tada', 'eyes', 'raised_hands'].includes(emoji)) return;
  try {
    s.av.realtimeSendDataMessage('data-channel', encodeReaction(emoji, s.attendeeId), 5000);
    emit(s.generation, { type: 'reaction', user: s.localUser, emoji });
  } catch (_) {
    emit(s.generation, { type: 'control-error', reason: 'Could not send the huddle reaction. Try again once connected.' });
  }
}
