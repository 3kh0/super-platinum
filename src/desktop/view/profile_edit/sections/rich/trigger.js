// Source branches B 814n, 0BU3, V5YY and Z1kt. Data comes from the pinned
// vp1n/NS8g/ZOnG default detector; query URL existence shares the same parser.
(function profileCompletionMatcher(data) {
  "use strict";
  const cjk = new RegExp(`[${data.cjk}]`);
  const urlPattern = new RegExp(data.common, 'gi');
  const uncommon = new RegExp(data.uncommon, 'i');
  const underscoreSuffix = new RegExp(data.underscore);
  const allowed = new RegExp(`^(\\s|\\{|\\[|\\(|\\*|\\_|\\/|\\"|\\u201c|\\'|\\u2018|\\u2019|\\<|\\:|[${data.cjk}])+`);
  const escape = s => s.replace(/[-[\]{}()*+?.,\\^$|#\s]/g, '\\$&');
  const emoticons = new RegExp(`(^|\\s)(${data.emoticons.map(escape).join('|')})(?=$|[\\s|\\?\\.,!])`, 'g');
  const member = /(^|\n|.)?((?:@|\uFF20)([^\n@\uFF20]*))$/i;
  const channel = /(^|\n|.)([#\uFF03][^~`!@#\uFF03$%^&*()+=[\]{}\\|;:'",.<>/?\s]*)$/i;
  const friendlyChannel = /(^|\n|.)([#\uFF03][^~`!@#\uFF03$%^&*()+=[\]{}\\|;:'",.<>/?]*)$/i;
  const emoji = /(^|\s|\S)([:|\uFF1A][^~@#$%^&*()=[\]{}\\|!?`;:\uFF1A",.<>/\s]*)$/i;
  const badPrefix = value => !!value?.replace(allowed, '');
  const www = host => host?.startsWith('www.') && host.split('.').length >= 3;
  function uncommonWithoutPrefix(match) {
    if (`http://${match.text}` === match.url && uncommon.test(match.text)) return true;
    let parsed;
    try { parsed = new URL(match.url, 'https://app.slack.com/client/'); } catch { parsed = {}; }
    const host = parsed.hostname;
    if (parsed.protocol === 'hansoft:' || parsed.protocol === 'spotify:') return false;
    if (!host) return !!(match.text && !match.text.includes('://'));
    if (!host.includes('.')) return false;
    const tld = host.split('.').pop();
    if (data.commonTlds.includes(tld) || host === 'ms.now') {
      if (data.caseTlds.includes(tld) && !www(host) && (!parsed.pathname || parsed.pathname === '/') && `http://${match.text}` === match.url) {
        const rawTld = match.text.split(/[/:?#]/, 1)[0].split('.').pop();
        if (!(rawTld === tld || (tld && rawTld === tld.toUpperCase()))) return true;
      }
      return false;
    }
    return !(www(host) || /^[A-Za-z][-.+A-Za-z0-9]*:\/\//.test(match.text));
  }
  function closeBracket(text) {
    const close = text.slice(-1), open = {')':'(', '}':'{', ']':'['}[close];
    if (!open) return text;
    let a = 0, b = 0;
    for (const char of text) { if (char === open) a++; if (char === close) b++; }
    return a === b - 1 ? text.slice(0, -1) : text;
  }
  function urls(text) {
    const result = [];
    urlPattern.lastIndex = 0;
    for (let match = urlPattern.exec(text); match; match = urlPattern.exec(text)) {
      let value = match[0], offset = match.index, protocol = match[1];
      const hasWWW = !!match[5], bare = match[9], slash = match[6] || match[10];
      if (bare && underscoreSuffix.test(text.slice(offset + value.length))) continue;
      const before = offset > 0 && text.charAt(offset - 1);
      if (before === '@' || (text.charAt(offset + value.length) === '@' && !protocol && !hasWWW && bare) || (before === '/' && !protocol && !hasWWW)) continue;
      const scheme = protocol?.match(/^([A-Za-z][-.+A-Za-z0-9]*):/);
      if (scheme && ['mailto','javascript','vbscript'].includes(scheme[1])) continue;
      if (!protocol && !hasWWW) {
        const leading = value.match(/^_+/);
        if (leading) { offset += leading[0].length; value = value.slice(leading[0].length); }
      }
      const hasProtocol = protocol && /^[A-Za-z][-.+A-Za-z0-9]*:\/\//.test(protocol);
      const spotify = /^spotify:(track|user|artist|album|playlist):[a-zA-Z0-9]+/;
      const isSpotify = spotify.test(value);
      if (!isSpotify && (!protocol || !hasProtocol) && !(protocol ? value.slice(value.indexOf(':')) : value).includes('.')) continue;
      const authority = hasProtocol ? value.split('://')[1] : value;
      if (authority.split('/')[0].includes('..')) continue;
      if (!slash && bare && bare === value && /^\.[A-Za-z0-9]+(?:[)}\]?*_~:\s]|$)/.test(text.slice(offset + bare.length))) continue;
      const basic = ['http','https','sftp','ftp'].find(p => !!protocol && protocol.includes(`${p}://`));
      const at = basic ? value.indexOf(basic) : 0;
      if (at > 0) { value = value.slice(at); protocol = protocol.slice(at); offset += at; }
      if (isSpotify) { const actual = text.slice(offset).match(spotify); if (actual) value = actual[0]; }
      const pipe = value.indexOf('|');
      if (before === '|' && pipe > 0) { urlPattern.lastIndex = offset + pipe; value = value.slice(0, pipe).replace(/[!:,.;^\u2026\u2014]+$/, ''); }
      value = closeBracket(value);
      if (value.endsWith('?')) value = value.slice(0, -1);
      let found = {type:'url', offset, text:value, url: slash || protocol ? value : `http://${value}`};
      if (protocol) {
        for (const [prefix, expression] of [
          ['hansoft://', /^hansoft:\/\/\S+/], ['rdar://', /^rdar:\/\/[0-9][0-9&]*/], ['daylite://', /^daylite:\/\/Command=ShowObject&\S+/]
        ]) if (found.url.startsWith(prefix)) { const actual = text.slice(offset).match(expression); if (actual) found = {...found, url:actual[0], text:actual[0]}; }
      }
      found = stripQuote(found);
      if (cjk.test((found.offset > 0 && text.charAt(found.offset - 1)) || '') && cjk.test(found.url.slice(-1))) {
        const end = found.url.split('').findLastIndex(char => !cjk.test(char)) + 1;
        const trimmed = found.url.substring(0, end);
        if (trimmed && !cjk.test(trimmed)) {
          const textEnd = found.text.split('').findLastIndex(char => !cjk.test(char)) + 1;
          found = {...found, url:trimmed, text:found.text.substring(0, textEnd)};
        }
      }
      urlPattern.lastIndex = Math.max(urlPattern.lastIndex, found.offset + found.text.length);
      if (!uncommonWithoutPrefix(found)) result.push(found);
    }
    return result;
  }
  function stripQuote(found) {
    // Only an unmatched trailing apostrophe in the final path segment.
    if (!found.url.endsWith("'") || (found.url.split('/').pop() || '').replace(/[^']/g, '').length % 2 === 0) return found;
    const trimmed = found.url.slice(0, -1);
    return {...found, url:trimmed, text:found.text.slice(0, -1)};
  }
  const wholeURL = text => { const found = urls(text); return found.length === 1 && found[0].text === text; };
  function match(kind, before, options = {}) {
    if (typeof before !== 'string' || before.length > 1_048_576) return null;
    let result = null;
    if (kind === 'member') {
      before.replace(member, (_all, prefix = '', token, query, index = 0) => {
        if (!badPrefix(prefix) && !/^\s/.test(query) && !(prefix === '/' && wholeURL(before))) result = {match:token,index:index + prefix.length};
      });
      return result && result.match.split(' ').length - 1 <= 8 ? result : null;
    }
    if (kind === 'channel') {
      if (options.suppressChannelCompleterMatch) return null;
      const found = before.match(options.experimentFriendlyChannelNamesEnabled ? friendlyChannel : channel);
      if (!found?.[2] || badPrefix(found[1]) || (found[1] === '/' && wholeURL(before))) return null;
      const index = (found.index || 0) + found[1].length;
      return {match:before.slice(index),index};
    }
    if (kind === 'emoji') {
      before.replace(emoji, (_all,prefix,token,index) => {
        if (prefix === '+' || prefix === '-' || !badPrefix(prefix)) result = {match:token,index:index + prefix.length};
      });
      emoticons.lastIndex = 0;
      if (!result || emoticons.test(result.match)) return null;
      const reaction = result.index === 1 && ['+','-'].includes(before[0]);
      return options.isUserSolicited || result.match.length >= (options.locale === 'ja-JP' || reaction ? 2 : 3) ? result : null;
    }
    return null;
  }
  return Object.freeze({match, wholeURL, hasURL: text => urls(text).length > 0});
})
