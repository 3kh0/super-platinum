// Emoji selection is admitted by Rust before native insertion. This helper is
// inert until the completion controller constructs one for its actual mount.
(() => {
  class EmojiSelectionGate {
    constructor({field, popup, owner, send, insert, composing}) {
      this.field = field; this.popup = popup; this.owner = owner;
      this.send = send; this.insert = insert; this.composing = composing;
      this.seq = 0; this.serial = 0; this.results = undefined; this.pending = undefined;
      this.scope = field.dataset.draftScope;
      this.mount = field.dataset.projectionVisit;
    }
    snapshot() {
      const field = this.field;
      return {text:field.value, anchor:field.selectionStart, focus:field.selectionEnd,
        direction:field.selectionDirection};
    }
    same(a,b) {
      return a && b && a.text === b.text && a.anchor === b.anchor
        && a.focus === b.focus && a.direction === b.direction;
    }
    current() {
      const field = this.field;
      return field.isConnected && this.popup.isConnected
        && document.getElementById(field.id) === field
        && document.getElementById(this.popup.id) === this.popup
        && field.dataset.draftScope === this.scope
        && field.dataset.projectionVisit === this.mount
        && !field.disabled && !field.readOnly && !this.composing()
        && field.dataset.composing !== 'true' && document.activeElement === field;
    }
    query(seq) {
      this.seq = seq; this.results = undefined; this.pending = undefined;
      this.source = this.snapshot();
    }
    present(result) {
      if (!this.current() || result.seq !== this.seq || !this.same(this.source,this.snapshot())
        || result.owner !== this.owner || typeof result.catalog !== 'string'
        || result.catalog !== this.field.dataset.emojiCatalog
        || result.preferences !== this.field.dataset.emojiPreferences
        || !result.catalog || !Array.isArray(result.choices)) return false;
      // A catalog replacement always retires the displayed/pending choice,
      // including an alias graph change that resolves to the same image URL.
      if (this.results?.catalog !== result.catalog
        || this.pending && !result.choices.some(choice => choice.id === this.pending.choice)) this.pending = undefined;
      this.results = result;
      return true;
    }
    choose(index) {
      if (this.pending || !this.current() || !this.results
        || !this.same(this.source,this.snapshot())) return false;
      const choice = this.results.choices[index];
      if (!choice || typeof choice.id !== 'string' || !choice.id
        || !Number.isSafeInteger(this.serial + 1)) return false;
      const ticket = {serial:++this.serial,seq:this.seq,owner:this.owner,
        catalog:this.results.catalog,choice:choice.id,snapshot:this.snapshot(),
        preferences:this.field.dataset.emojiPreferences};
      this.pending = ticket;
      this.send({kind:'emoji_choose',...ticket});
      return true;
    }
    admit(result) {
      const ticket = this.pending;
      if (!ticket || result.serial !== ticket.serial) return false;
      // Consume matching admissions once, including rejected/stale replies.
      this.pending = undefined;
      if (!result.allowed || !this.current() || result.seq !== this.seq
        || result.owner !== ticket.owner || result.catalog !== ticket.catalog
        || result.choice !== ticket.choice || this.results?.catalog !== ticket.catalog
        || this.field.dataset.emojiCatalog !== ticket.catalog
        || this.field.dataset.emojiPreferences !== ticket.preferences
        || !this.same(ticket.snapshot,this.snapshot())
        || typeof result.value !== 'string' || !result.value
        || !Number.isInteger(result.start) || !Number.isInteger(result.end)
        || result.start < 0 || result.end < result.start
        || result.end !== ticket.snapshot.focus || result.end > ticket.snapshot.text.length
        || typeof result.spacer !== 'string' || !['',' '].includes(result.spacer)) return false;
      this.results = undefined;
      // This callback uses the existing native atomic replacement/undo path;
      // value is plain text. No fabricated reference token or reference label.
      if (this.insert(this.field,result.value,result.value,result.start,result.end,result.spacer,'forward') !== true) return false;
      this.send({kind:'emoji_inserted',...ticket,after:this.snapshot()});
      return true;
    }
    dismiss() { this.pending = undefined; this.results = undefined; }
  }
  window.SuperPlatinumEmojiSelectionGate = EmojiSelectionGate;
})();
