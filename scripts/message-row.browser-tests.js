window.runMessageRowTests = async (css) => {
  const assert=(ok,message)=>{if(!ok)throw Error(message);};
  document.head.innerHTML='<style>:root{--msg-avatar:32px;--muted:#9b9b9b;--text:#eee;--accent:#5599ff;--surface-raised:#222;--text-xs:11px}body{font:14px/1.5 sans-serif}</style>';
  const style=document.createElement('style');style.textContent=css;document.head.append(style);
  document.body.innerHTML='<article class="message thread-message compact" tabindex="0" aria-label="Fixture author, 9:47"><div class="compact-gutter">9:47</div><span class="message-tools"><button>Add a reaction</button><button>Save for later</button></span><div class="message-content"><div class="message-body">Synthetic reply<span class="message-edited"> (edited)</span></div></div></article><button id="outside">Outside</button>';
  const row=document.querySelector('article'), tools=row.querySelector('.message-tools'), gutter=row.querySelector('.compact-gutter');
  const height=row.getBoundingClientRect().height;
  row.focus();
  assert(['flex','inline-flex'].includes(getComputedStyle(tools).display),'keyboard focus did not expose actions');
  assert(getComputedStyle(gutter).color!=='rgba(0, 0, 0, 0)','keyboard focus did not reveal timestamp');
  tools.querySelector('button').focus();
  assert(['flex','inline-flex'].includes(getComputedStyle(tools).display),'toolbar disappeared when its button received focus');
  assert(row.getBoundingClientRect().height===height,'focusing actions resized the compact message');
  assert(row.querySelector('.message-edited').getBoundingClientRect().width>0,'edited label disappeared');
  assert(row.getAttribute('aria-label').includes('Fixture author'),'compact row lost accessible author');
  document.getElementById('outside').focus();
  return ['compact keyboard focus reveals timestamp and actions','toolbar focus preserves row height','edited status and accessible author remain available'];
};
