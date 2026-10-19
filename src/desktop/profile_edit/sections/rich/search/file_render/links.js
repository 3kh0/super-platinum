// Boolean v+tG existence for the default Texty options. Merge/sort cannot
// change empty vs nonempty, so do not manufacture link labels or URI authority.
function createProfileLinkQuery(data, urlMatcher) {
  const common = new Set(data.commonTlds);
  const cjk = new RegExp(`[${data.cjk}]`);
  const delimiters = ['*', '`', '_', '~', '\uFF0A', '\uFF40', '\uFF3F'];
  const email = /(?:\b|_)([A-Z0-9][A-Z0-9._%+-]{0,69})@(?:([A-Z0-9-]{1,63})\.){1,125}([A-Z]{2,63})(\b|_)/gi;
  const phone = new RegExp(data.phone, 'g');
  function hasEmail(input) {
    email.lastIndex = 0;
    for (const found of input.matchAll(email)) {
      let text = found[0], offset = found.index;
      if (found[4]) text = text.slice(0, text.length - found[4].length);
      if (text.length > 254 || !common.has(found[3].toLowerCase())) continue;
      if (delimiters.includes(text[0])) { text = text.slice(1); offset++; }
      if (offset > 0 && input.charAt(offset - 1) === '@') continue;
      if (text.startsWith('git@') && input.charAt(offset + text.length) === ':') continue;
      return true;
    }
    return false;
  }
  function hasPhone(input) {
    phone.lastIndex = 0;
    const before = [' ', '\u3000', '\n', '(', '[', '{', "'", '"'];
    const after = [' ', '\u3000', '\n', '.', ')', ']', '}', "'", '"', ':', '?', '!', ';', ','];
    for (const found of input.matchAll(phone)) {
      if (/^\d+$/.test(found[0])) continue;
      let text = found[0], offset = found.index;
      if (text[0] === '(' && !text.includes(')')) { text = text.slice(1); offset++; }
      const end = offset + text.length;
      const a = input.charAt(offset - 1), b = input.charAt(end);
      if ((offset === 0 || before.includes(a) || cjk.test(a)) &&
          (end === input.length || after.includes(b) || cjk.test(b))) return true;
    }
    return false;
  }
  return input => {
    if (typeof input !== 'string' || input.length > 32768) throw new Error('Invalid matching query');
    return urlMatcher.hasURL(input) || hasEmail(input) || hasPhone(input);
  };
}
