// All custom identity is in WeakMaps, never reconstructed from editable HTML.
// This constructor is private to Comments; the application's RichEditor is not
// registered with Quill or modified by these formats.
function listCommentDeltaBlots(Quill) {
  const Inline = Quill.import('blots/inline'), Embed = Quill.import('blots/embed');
  const values = new WeakMap(), tokenValues = new WeakMap(), images = new WeakMap();
  const clone = value => JSON.parse(JSON.stringify(value));
  function mediaUri(value) {
    // MediaUri::presentation_uri emits these exact synthetic origins, not a
    // wildcard subdomain. MediaAssetId::uri_at uses a canonical u64 generation.
    if (typeof value !== 'string') return false;
    const match=/^(?:super-platinum-media:\/\/|http:\/\/super-platinum-media\.)(?:avatar|emoji|attachment|background)\/[0-9a-f]{32}(?:\?v=(0|[1-9][0-9]{0,19}))?$/.exec(value);
    return !!match && match[0]===value
      && (match[1]===undefined || BigInt(match[1])<=18446744073709551615n);
  }
  function mediaAt(uri, epoch) {
    if (epoch === null) return uri;
    const [base, query] = uri.split('?v=');
    const current = query === undefined ? 0n : BigInt(query);
    return `${base}?v=${current > epoch ? current : epoch}`;
  }
  // Non-plain class instances survive Quill Delta's extend(true) by identity.
  // Enumerable unique id also prevents its deep-equal from merging two tokens.
  // A JSON lookalike has no WeakMap entry and cannot import opaque identity.
  class Token { constructor() { this.id = globalThis.crypto.randomUUID(); } }
  function semantic(name, tag, className) {
    return class extends Inline {
      static blotName = name;
      static tagName = tag;
      static className = className;
      static create(value) {
        const node = super.create();
        if (!value || typeof value !== 'object' || typeof value.id !== 'string' || typeof value.label !== 'string')
          throw Error('Invalid Comments semantic text');
        values.set(node, clone(value));
        node.setAttribute('spellcheck', 'false');
        node.setAttribute('dir', 'ltr');
        return node;
      }
      static formats(node) {
        const value=values.get(node);
        if(!value)throw Error('Foreign Comments semantic node');
        return clone(value);
      }
      clone() {
        // Parchment's ShadowBlot.clone bypasses static create(value): it shallow
        // clones the DOM then calls Registry.create(node). Inline split reaches
        // that path. Transfer authority only from this original owned node,
        // before the real registry constructs its replacement blot.
        const value=values.get(this.domNode);
        if(!value)throw Error('Foreign Comments semantic clone');
        const node=this.domNode.cloneNode(false);
        values.set(node,clone(value));
        return Quill.import('parchment').create(node);
      }
    };
  }
  const Mention = semantic('slackmention', 'TS-MENTION', 'comment-delta-mention');
  const Slug = semantic('slackslug', 'TS-SLUG', 'comment-delta-slug');
  // O+Q+ getSlackUnlinkedFormat: this semantic marker suppresses linkification.
  class Unlinked extends Inline {
    static blotName='unlink';
    static tagName=['TS-UNLINKED'];
    static formats(node) { return node.tagName==='TS-UNLINKED'; }
  }
  class Emoji extends Embed {
    static blotName = 'slackemoji';
    static tagName = 'SPAN';
    static className = 'comment-delta-embed';
    static create(token) {
      const node = super.create();
      const held = tokenValues.get(token);
      if (held?.kind !== 'slackemoji') throw Error('Foreign Comments emoji identity');
      const {value, appearance} = held;
      if (!appearance) throw Error('Emoji appearance unavailable');
      values.set(node, token);
      node.setAttribute('aria-label', value.text);
      if (appearance.media) {
        const image = node.ownerDocument.createElement('img');
        const src = mediaAt(appearance.media, held.mediaEpoch.value);
        image.setAttribute('src', src);
        image.setAttribute('alt', appearance.text);
        images.set(image, {node, token, src});
        node.appendChild(image);
      } else node.textContent = appearance.text;
      return node;
    }
    static value(node) {
      const value = values.get(node);
      if (!value) throw Error('Foreign Comments emoji node');
      return value;
    }
  }
  class WorkflowToken extends Embed {
    static blotName = 'workflowtoken';
    static tagName = 'BUTTON';
    static className = 'comment-delta-workflow';
    static create(token) {
      const node = super.create();
      const held = tokenValues.get(token);
      if (held?.kind !== 'workflowtoken') throw Error('Foreign Comments workflow identity');
      const {value} = held;
      values.set(node, token);
      node.setAttribute('type', 'button');
      node.setAttribute('tabindex', '-1');
      // PlainSeed emits unavailable tokens with empty labels. FrTg renders that
      // state as Missing Data. An available picker/edit action is not invented.
      if (value.isAvailable !== false) throw Error('Available workflow-token editor is not implemented');
      node.textContent = value.label || 'Missing Data';
      return node;
    }
    static value(node) {
      const value = values.get(node);
      if (!value) throw Error('Foreign Comments workflow node');
      return value;
    }
  }
  Quill.register({'formats/slackmention':Mention,'formats/slackslug':Slug,'formats/unlink':Unlinked,
    'formats/slackemoji':Emoji,'formats/workflowtoken':WorkflowToken}, true);
  return Object.freeze({
    codec(rows) {
      if (!Array.isArray(rows)) throw Error('Emoji appearance observations unavailable');
      const appearances = new Map(), owner = Symbol('Comments Delta owner');
      const mediaEpoch = {value:null};
      for (const row of rows) {
        if (!row || typeof row.name !== 'string' || typeof row.text !== 'string' || !row.text
            || row.media !== undefined && !mediaUri(row.media))
          throw Error('Invalid opaque emoji appearance');
        // The parent supplies opaque MediaUri, never source Slack/download URLs.
        if (appearances.has(row.name)) throw Error('Duplicate emoji appearance');
        appearances.set(row.name, clone(row));
      }
      const issued = new Map();let appearanceRevision=0;
      function prepare(ops, extra = []) {
        if(!Array.isArray(extra))throw Error('Emoji appearance observations unavailable');
        const revision=appearanceRevision,next=new Map(appearances),staged=new Map();
        const seen=new Set();
        for(const row of extra) {
          if(!row||typeof row.name!=='string'||typeof row.text!=='string'||!row.text
              ||row.media!==undefined&&!mediaUri(row.media)||seen.has(row.name))
            throw Error('Invalid opaque emoji appearance');
          seen.add(row.name);
          if(next.has(row.name)&&JSON.stringify(next.get(row.name))!==JSON.stringify(row))
            throw Error('Original emoji appearance changed');
          next.set(row.name,clone(row));
        }
        const encoded=ops.map(op=>{
          if(!Object.prototype.hasOwnProperty.call(op,'insert')||typeof op.insert==='string')return clone(op);
          const [kind]=Object.keys(op.insert),value=op.insert[kind],appearance=next.get(value.text);
          if(kind==='slackemoji'&&!appearance)throw Error('Emoji appearance unavailable');
          if(kind==='workflowtoken'&&value.isAvailable!==false)throw Error('Available workflow-token editor is not implemented');
          if(!['slackemoji','workflowtoken'].includes(kind))throw Error('Unsupported opaque kind');
          const key=JSON.stringify([kind,value]);let token=issued.get(key)||staged.get(key);
          if(!token){token=new Token();staged.set(key,token);
            tokenValues.set(token,{owner,kind,value:clone(value),appearance,mediaEpoch});}
          return {...clone(op),insert:{[kind]:token}};
        });
        let committed=false;
        return Object.freeze({ops:encoded,commit(){
          if(committed)throw Error('Opaque insertion plan already committed');
          if(revision!==appearanceRevision)throw Error('Opaque insertion plan retired');
          committed=true;
          appearanceRevision++;
          for(const[key,value]of next)appearances.set(key,value);
          for(const[key,value]of staged)issued.set(key,value);
        }});
      }
      return Object.freeze({
        prepare,
        emojiAt(node) {
          const token = values.get(node), held = tokenValues.get(token);
          return held?.owner === owner && held.kind === 'slackemoji' ? clone(held.value) : null;
        },
        refreshMedia(root, epoch) {
          // Presentation only: never re-encode a Delta or change an issued
          // token. Quill history can recreate a blot using the same token.
          if (typeof epoch !== 'string' || !/^(0|[1-9][0-9]{0,19})$/.test(epoch)) return false;
          const next = BigInt(epoch);
          if (next > 18446744073709551615n || mediaEpoch.value !== null && next < mediaEpoch.value)
            return false;
          mediaEpoch.value = next;
          for (const image of root.querySelectorAll('img')) {
            const issuedImage = images.get(image);
            if (!issuedImage) continue;
            const {node, token} = issuedImage, held = tokenValues.get(token);
            if (held?.owner !== owner || values.get(node) !== token
                || !root.contains(node) || !node.contains(image)
                || image.getAttribute('src') !== issuedImage.src) continue;
            const src = mediaAt(held.appearance.media, next);
            if (src !== issuedImage.src) {
              image.setAttribute('src', src);
              issuedImage.src = src;
            }
          }
          return true;
        },
        encode(ops) {
          const plan=prepare(ops);plan.commit();return plan.ops;
        },
        decode(ops) {
          return ops.map(op => {
            if (typeof op.insert === 'string') return clone(op);
            const [kind] = Object.keys(op.insert), token = op.insert[kind], held = tokenValues.get(token);
            if (!held || held.owner !== owner || held.kind !== kind) throw Error('Foreign Comments embed capture');
            return {...clone({...op,insert:null}),insert:{[kind]:clone(held.value)}};
          });
        },
      });
    },
  });
}
