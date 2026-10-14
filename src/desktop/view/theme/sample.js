(() => {
  const samples = [], host = document.createElement('div');
  host.dataset.contrastProbe = 'temporary-css-only';
  host.style.cssText = 'position:fixed;left:-20000px;top:0;width:400px;background:var(--sidebar);pointer-events:none;';
  const rgba = text => {
    if (text === 'transparent') return [0,0,0,0];
    const srgb = text.startsWith('color(srgb ');
    if (!srgb && !/^rgba?\(/.test(text)) throw Error('Unsupported computed color: '+text);
    const values = text.replace(srgb ? 'color(srgb' : /^rgba?/, '').match(/[-+]?(?:\d*\.)?\d+(?:e[-+]?\d+)?/gi).map(Number);
    return [values[0]/(srgb?1:255),values[1]/(srgb?1:255),values[2]/(srgb?1:255),values[3]??1];
  };
  const over = (a,b) => a.slice(0,3).map((v,i)=>v*a[3]+b[i]*(1-a[3]));
  const luminance = rgb => rgb.map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4).reduce((n,v,i)=>n+v*[.2126,.7152,.0722][i],0);
  const contrast = (a,b) => { const x=luminance(a),y=luminance(b); return (Math.max(x,y)+.05)/(Math.min(x,y)+.05); };
  function measure(element,name,kind,threshold=4.5,pseudo=null) {
    const style=getComputedStyle(element,pseudo), chain=[];
    // UA placeholder opacity is separate from the input's opacity. Refuse an
    // opaque-text contrast claim if the pseudo-element adds group opacity.
    if (pseudo && Number(style.opacity)!==1) throw Error('Unmodeled pseudo opacity: '+name);
    for(let node=element;node;node=node.parentElement) chain.push(node);
    let background=[1,1,1];
    for(const node of chain.reverse()) {
      const css=getComputedStyle(node);
      if (Number(css.opacity)!==1) throw Error('Unmodeled group opacity: '+name);
      if(css.backgroundImage!=='none') throw Error('Unmodeled image background: '+name);
      background=over(rgba(css.backgroundColor),background);
    }
    const foreground=over(rgba(style.color),background), ratio=contrast(foreground,background);
    samples.push({name,kind,color:style.color,background:style.backgroundColor,opacity:style.opacity,compositedBackground:background,compositedForeground:foreground,ratio,threshold,ok:ratio>=threshold,transitionProperty:style.transitionProperty});
  }
  const settings=!!document.querySelector('.settings-grid');
  const selectors=settings?['.settings-grid > button']:['.rail-badge','.ping-badge','.jump-to .hint'];
  for(const selector of selectors) {
    const elements=[...document.querySelectorAll(selector)].filter(e=>e.getBoundingClientRect().width>0&&!e.disabled);
    if(!elements.length && selector!=='.ping-badge') throw Error('Missing actual control: '+selector);
    for(const [i,element] of elements.entries()) measure(element,selector+'['+i+']','actual-mounted-ui');
  }
  document.body.append(host);
  const node = (tag,classes,parent=host) => {const e=document.createElement(tag);e.className=classes;e.textContent='Contrast sample';parent.append(e);return e;};
  try {
    const dm=node('button','dm-row active'); measure(node('span','dm-time',dm),'selected DM timestamp','css-probe');
    const input=node('input','list-find'); input.placeholder='Find'; measure(input,'list search placeholder','css-probe',4.5,'::placeholder');
    measure(node('span','mpdm-count'),'group DM count','css-probe');
    measure(node('button','block-button primary'),'primary fill','css-probe');
    measure(node('button','block-button danger'),'danger fill','css-probe');
    measure(node('button','huddle-control leave'),'huddle leave icon foreground','css-probe',3);
    const modal=node('div','button-confirmation'), footer=node('footer','',modal);
    measure(node('button','button-confirmation-submit danger',footer),'destructive confirmation','css-probe');
    // Explicit endpoint probes: these are not claims of real pointer hover.
    const primary=node('button','block-button primary');
    primary.style.background='color-mix(in srgb, var(--accent) 82%, white)'; primary.style.color='var(--on-accent-hover)';
    measure(primary,'primary hover endpoint','hover-token-probe');
    const leave=node('button','huddle-control leave');
    leave.style.background='color-mix(in srgb, var(--danger) 85%, white)'; leave.style.color='var(--on-danger-hover)';
    measure(leave,'leave hover endpoint','hover-token-probe',3);
    // #707070 crosses the adaptive foreground boundary on hover. Both fill
    // and foreground must change instantly, not interpolate through equality.
    host.style.setProperty('--accent','#707070'); host.style.setProperty('--on-accent','#FFFFFF'); host.style.setProperty('--on-accent-hover','#000000');
    host.style.setProperty('--danger','#707070'); host.style.setProperty('--on-danger','#FFFFFF'); host.style.setProperty('--on-danger-hover','#000000');
    for(const classes of ['block-button primary','huddle-control leave']) {
      const e=node('button',classes), props=getComputedStyle(e).transitionProperty.split(',').map(x=>x.trim());
      if(props.some(p=>['all','color','background','background-color'].includes(p))) throw Error('Adaptive foreground transition: '+classes);
      measure(e,classes+' custom crossover base','custom-fill-css-probe');
      e.style.background=classes.includes('primary')?'color-mix(in srgb, var(--accent) 82%, white)':'color-mix(in srgb, var(--danger) 85%, white)';
      e.style.color=classes.includes('primary')?'var(--on-accent-hover)':'var(--on-danger-hover)';
      measure(e,classes+' custom crossover hover endpoint','custom-fill-css-probe');
    }
  } finally { host.remove(); }
  return {ok:samples.every(s=>s.ok),samples,probesRemoved:!host.isConnected,viewport:[innerWidth,innerHeight],scope:'Selected active text and filled-control pairs; no full-theme certification; hover endpoints are probes.'};
})()
