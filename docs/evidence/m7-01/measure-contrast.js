() => {
  // Mirror only the primitive hover selectors with a fixture-only class; the declarations
  // are taken from the loaded stylesheets, and computed styles are read from real elements.
  const hoverRules=[];
  function visit(rules){for(const r of rules){if(r.selectorText && r.selectorText.includes(':hover') && /ui-(btn|disclosure|theme-toggle)/.test(r.selectorText))hoverRules.push(r.cssText.replaceAll(':hover','.audit-hover')); if(r.cssRules)visit(r.cssRules);}}
  for(const sheet of document.styleSheets){try{visit(sheet.cssRules)}catch{}}
  const hoverSheet=document.createElement('style'); hoverSheet.textContent=hoverRules.join('\n');document.head.append(hoverSheet);
  const fixture = document.createElement('div'); fixture.id = 'contrast-audit-fixture'; document.body.append(fixture);
  const cases = [];
  const add=(name,html,selector,fg,bg,kind='text',property='color',surfaces=['paper']) => cases.push({name,html,selector,fg,bg,kind,property,surfaces});
  const surfaces=['paper','panel','soft','soft-strong','control-surface'];
  add('Disclosure closed','<details class="ui-disclosure"><summary>Title <span class="ui-disclosure-count">2</span></summary></details>','summary','ink','surface','text','color',surfaces);
  add('Disclosure open','<details class="ui-disclosure" open><summary>Title <span class="ui-disclosure-count">2</span></summary></details>','summary','ink','soft-strong');
  add('Disclosure arrow closed','<details class="ui-disclosure"><summary>Title</summary></details>','summary','blue','surface','ui','before-color',surfaces);
  add('Disclosure arrow open','<details class="ui-disclosure" open><summary>Title</summary></details>','summary','blue','soft-strong','ui','before-color');
  for(const [tone,fg,bg] of [['neutral','ink','soft'],['info','blue','blue-bg'],['ok','success','success-bg'],['warn','amber','amber-bg'],['placeholder','amber','amber-bg'],['bad','red','red-bg']]) for(const cls of ['ui-pill','ui-badge']) add(`${cls} ${tone}`,`<span class="${cls} ui-tone-${tone}">State</span>`,'span',fg,bg);
  for(const [cls,fg,bg] of [['ui-btn','ink','panel'],['ui-btn ui-btn-primary','on-accent','accent'],['ui-btn ui-btn-danger','red','red-bg']]) add(`Button ${cls}`,`<button class="${cls}">Button</button>`,'button',fg,bg);
  for(const s of surfaces){add('Button quiet',`<button class="ui-btn ui-btn-quiet">Quiet</button>`,'button','blue','surface','text','color',[s]);add('Button disabled',`<button disabled class="ui-btn">Disabled</button>`,'button','subtle','surface','text','color',[s]);}
  add('Panel title','<section class="ui-panel"><h3 class="ui-panel-title">Title</h3></section>','h3','ink','panel');
  for(const [cls,bg] of [['ui-bubble','soft'],['ui-bubble ui-bubble-human','panel']]){add(`Bubble ${cls}`,`<div class="${cls}"><div class="ui-bubble-meta"><b>Author</b> time</div><div class="ui-bubble-body">Body</div></div>`,'.ui-bubble-body','ink',bg);add(`Bubble metadata ${cls}`,`<div class="${cls}"><div class="ui-bubble-meta">time</div></div>`,'.ui-bubble-meta','subtle',bg);}
  add('EmptyState title','<div class="ui-empty"><p class="ui-empty-title">Empty</p><p class="ui-empty-description">Description</p></div>','.ui-empty-title','ink','surface','text','color',surfaces);
  add('EmptyState description','<div class="ui-empty"><p class="ui-empty-description">Description</p></div>','.ui-empty-description','subtle','surface','text','color',surfaces);
  add('Meter label','<div class="ui-meter"><span>CPU</span><span class="ui-meter-track" style="--level:50%"></span><span class="ui-meter-value">50%</span></div>','span','ink','surface','text','color',surfaces);
  add('Meter value','<div class="ui-meter"><span>CPU</span><span class="ui-meter-track" style="--level:50%"></span><span class="ui-meter-value">50%</span></div>','.ui-meter-value','ink','surface','text','color',surfaces);
  add('Theme toggle','<button class="ui-theme-toggle">Theme <span class="ui-theme-state">On</span></button>','button','ink','panel');
  add('Theme toggle state','<button class="ui-theme-toggle">Theme <span class="ui-theme-state">On</span></button>','.ui-theme-state','subtle','panel');
  for(const [cls,fg,bg] of [['pill','ink','soft'],['pill pill-connected','success','success-bg'],['pill pill-reconnecting','amber','amber-bg'],['pill pill-disconnected','red','red-bg'],['notice','ink','amber-bg'],['notice notice-error','ink','red-bg'],['placeholder','ink','amber-bg'],['badge','amber','amber-bg'],['placeholder-tag','amber','amber-bg']]) add(`app.css ${cls}`,`<span class="${cls}">Text</span>`,'span',fg,bg);
  add('app.css label','<label>Label</label>','label','component-muted','surface','text','color',surfaces);
  add('app.css muted','<span class="muted">Muted</span>','span','component-muted','surface','text','color',surfaces);
  add('app.css input text','<input value="text">','input','ink','panel');
  add('app.css button','<button>Button</button>','button','ink','panel');
  add('app.css disabled button','<button disabled>Button</button>','button','subtle','panel');
  add('app.css pre','<pre>Text</pre>','pre','ink','soft');
  add('app.css strip','<div class="strip">Text</div>','div','ink','soft');
  add('Button boundary','<button class="ui-btn">Button</button>','button','control-border','surface','ui','border',surfaces);
  add('EmptyState boundary','<div class="ui-empty">Empty</div>','div','control-border','surface','ui','border',surfaces);
  add('Meter boundary','<div class="ui-meter"><span class="ui-meter-track" style="--level:50%"></span></div>','.ui-meter-track','control-border','surface','ui','outline',surfaces);
  add('input boundary','<input value="Text">','input','control-border','surface','ui','border',surfaces);
  add('focus ring','<button class="ui-btn">Button</button>','button','focus','surface','ui','focus',surfaces);
  add('Button primary boundary','<button class="ui-btn ui-btn-primary">Button</button>','button','accent','surface','ui','border',surfaces);
  add('Button danger boundary','<button class="ui-btn ui-btn-danger">Button</button>','button','red','surface','ui','border',surfaces);
  for(const [tone,fg] of [['info','blue'],['ok','success'],['warn','amber'],['bad','red']]) add(`Pill ${tone} boundary`,`<span class="ui-pill ui-tone-${tone}">State</span>`,'span',fg,'surface','ui','border',surfaces);
  add('app.css notice warning stripe','<div class="notice">Text</div>','div','amber','surface','ui','border-left',surfaces);
  add('app.css notice error stripe','<div class="notice notice-error">Text</div>','div','red','surface','ui','border-left',surfaces);
  add('app.css placeholder boundary','<div class="placeholder">Text</div>','div','amber','surface','ui','border',surfaces);
  for(const [fg,bg] of [['meter-marker','meter-marker-ring'],['meter-marker','meter-track'], ...[1,2,3,4,5].map(i=>['meter-fill-pattern',`meter-stop-${i}`])]) add(`Meter ${fg} on ${bg}`,`<span style="color:var(--${fg});background:var(--${bg})">■</span>`,'span',fg,bg,'ui');
  for(const fg of ['meter-marker','meter-marker-ring']) for(let i=1;i<=5;i++) { const bg=`meter-stop-${i}`; add(`Meter ${fg} on ${bg}`,`<span style="color:var(--${fg});background:var(--${bg})">■</span>`,'span',fg,bg,'ui'); }
  add('Disclosure hover','<details class="ui-disclosure"><summary class="audit-hover">Title</summary></details>','summary','ink','soft-strong');
  add('Disclosure arrow hover','<details class="ui-disclosure"><summary class="audit-hover">Title</summary></details>','summary','blue','soft-strong','ui','before-color');
  for(const [cls,fg,bg] of [['ui-btn','ink','soft-strong'],['ui-btn ui-btn-primary','on-accent','accent'],['ui-btn ui-btn-danger','red','soft-strong'],['ui-btn ui-btn-quiet','blue','soft-strong']]) add(`Button hover ${cls}`,`<button class="${cls} audit-hover">Button</button>`,'button',fg,bg);
  add('Theme toggle hover','<button class="ui-theme-toggle audit-hover">Theme</button>','button','ink','soft');
  // Decorative separators and panel boundaries are included so low ratios are visible.
  add('Disclosure decorative separator','<details class="ui-disclosure"><summary>Title</summary></details>','details','line-soft','surface','ui','border-bottom',surfaces);
  add('Panel decorative border','<section class="ui-panel">Panel</section>','section','line','surface','ui','border',surfaces);
  add('Bubble decorative border','<div class="ui-bubble ui-bubble-human">Body</div>','div','line','surface','ui','border',surfaces);
  add('app.css pre decorative border','<pre>Text</pre>','pre','line','surface','ui','border',surfaces);
  add('app.css strip decorative separator','<div class="strip">Text</div>','div','line','surface','ui','border-bottom',surfaces);
  add('app.css table decorative separator','<table><tbody><tr><td>Cell</td></tr></tbody></table>','td','line','surface','ui','border-bottom',surfaces);
  function parse(s){let m=s.match(/rgba?\(([^)]+)\)/); if(!m) throw Error('not RGB: '+s);let a=m[1].split(/[,\s/]+/).filter(Boolean).map(Number); return [a[0],a[1],a[2],a[3]??1];}
  function blend(a,b){return [0,1,2].map(i=>a[i]*a[3]+b[i]*(1-a[3])).concat(1)}
  function ratio(a,b){function l(c){return c.slice(0,3).map(x=>x/255).map(x=>x<=.04045?x/12.92:((x+.055)/1.055)**2.4).reduce((v,x,i)=>v+x*[.2126,.7152,.0722][i],0)}let x=l(a),y=l(b);return (Math.max(x,y)+.05)/(Math.min(x,y)+.05)}
  function bg(el){for(let n=el;n;n=n.parentElement){let v=parse(getComputedStyle(n).backgroundColor);if(v[3]>.999)return v;if(v[3]>0)return blend(v,bg(n.parentElement));}return [255,255,255,1]}
  const results=[];
  for(const theme of ['light','dark']) {document.documentElement.dataset.theme=theme; for(const c of cases) for(const surface of c.surfaces){ fixture.innerHTML=`<div style="background:var(--${surface});color:var(--ink);padding:8px">${c.html}</div>`;const wrapper=fixture.firstElementChild, el=wrapper.querySelector(c.selector);if(c.property==='focus')el.focus();const st=getComputedStyle(el);const value=c.property==='border-bottom'?st.borderBottomColor:c.property==='border'?st.borderTopColor:c.property==='border-left'?st.borderLeftColor:c.property==='outline'||c.property==='focus'?st.outlineColor:c.property==='before-color'?getComputedStyle(el,'::before').color:st.color; const fg=parse(value), back= c.property==='border'||c.property==='border-bottom'||c.property==='border-left'||c.property==='outline'||c.property==='focus'?bg(wrapper):bg(el); let expected=c.bg==='surface'?surface:c.bg; let threshold=c.kind==='text'?4.5:3; results.push({theme,name:c.name,fg:c.fg,bg:expected,resolvedFg:value,resolvedBg:`rgb(${back.slice(0,3).join(', ')})`,ratio:ratio(fg,back),threshold,pass:ratio(fg,back)>=threshold,focusVisible:c.property==='focus'?el.matches(':focus-visible'):undefined}); }}
  fixture.remove(); hoverSheet.remove(); return results;
}
