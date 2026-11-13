const fs = require('fs');
const path = require('path');
const net = require('net');
const crypto = require('crypto');
const { spawn, execFileSync } = require('child_process');
const { WebSocketServer, WebSocket } = require('ws');
const { chromium } = require(process.env.WIN3_DESKTOP_DIR + '/node_modules/playwright');

const out = process.env.WIN3_EVIDENCE_DIR;
const temp = process.env.WIN3_TEMP_DIR;
const dataDir = process.env.WIN3_DATA_DIR;
const daemonExe = process.env.WIN3_DAEMON_EXE;
const desktopDir = process.env.WIN3_DESKTOP_DIR;
const nodeRoot = path.dirname(process.execPath);
const npmCli = path.join(nodeRoot, 'node_modules', 'npm', 'bin', 'npm-cli.js');
const browserProfile = path.join(temp, 'edge-profile');
const nonce = crypto.randomBytes(18).toString('hex');
const f = (name) => path.join(out, name);
const pause = (ms) => new Promise((r) => setTimeout(r, ms));
const rows = [];
const calls = [];
let browserVersion = null;
let fatal = null;
function row(id, status, detail, evidence) {
  rows.push({ id: 'WIN-BP-' + id, method: 'browser-proxy', status, detail, evidence: evidence || [] });
  fs.writeFileSync(f('WIN-BP-observations.json'), JSON.stringify({method:'browser-proxy', testedAt:new Date().toISOString(), browser:browserVersion, rows, calls}, null, 2));
}
function frame(obj) {
  const body = Buffer.from(JSON.stringify(obj), 'utf8');
  const head = Buffer.alloc(6); head.writeUInt16BE(1, 0); head.writeUInt32BE(body.length, 2);
  return Buffer.concat([head, body]);
}
function canonicalWindowsPath(p) {
  let value = fs.realpathSync(p).toLowerCase();
  if (!value.startsWith('\\\\?\\')) value = '\\\\?\\' + value;
  return value;
}
function pipeForDataDir() {
  const bytes = Buffer.from(canonicalWindowsPath(dataDir), 'utf8');
  let h = 0xcbf29ce484222325n;
  for (const b of bytes) { h ^= BigInt(b); h = BigInt.asUintN(64, h * 0x100000001b3n); }
  return { path: '\\\\.\\pipe\\agent-orch-' + h.toString(16).padStart(16, '0'), hash: h.toString(16).padStart(16, '0') };
}
const pipe = pipeForDataDir();
function readFrame(state) {
  const chunks = [];
  return new Promise((resolve, reject) => {
    const onData = (d) => {
      chunks.push(d); const b = Buffer.concat(chunks);
      if (b.length < 6) return;
      if (b.readUInt16BE(0) !== 1) { cleanup(); reject(new Error('unsupported frame version')); return; }
      const len = b.readUInt32BE(2);
      if (b.length < 6 + len) return;
      cleanup(); resolve({ value: JSON.parse(b.subarray(6, 6 + len).toString('utf8')), rest: b.subarray(6 + len) });
    };
    const onError = (e) => { cleanup(); reject(e); };
    const onClose = () => { cleanup(); reject(new Error('daemon pipe closed')); };
    const cleanup = () => { state.socket.off('data', onData); state.socket.off('error', onError); state.socket.off('close', onClose); };
    state.socket.on('data', onData); state.socket.once('error', onError); state.socket.once('close', onClose);
  });
}
async function openPipe(onFrame) {
  const socket = net.createConnection(pipe.path);
  const state = { socket, buffer: Buffer.alloc(0), pending: new Map(), seq: 0, onFrame, closed:false };
  socket.on('data', (chunk) => {
    state.buffer = Buffer.concat([state.buffer, chunk]);
    while (state.buffer.length >= 6) {
      if (state.buffer.readUInt16BE(0) !== 1) { socket.destroy(new Error('unsupported frame version')); return; }
      const len = state.buffer.readUInt32BE(2); if (state.buffer.length < 6 + len) break;
      const value = JSON.parse(state.buffer.subarray(6, 6 + len).toString('utf8'));
      state.buffer = state.buffer.subarray(6 + len);
      if (value && value.id !== undefined && state.pending.has(String(value.id))) {
        const p = state.pending.get(String(value.id)); state.pending.delete(String(value.id));
        if (value.error) p.reject(Object.assign(new Error(value.error.message || 'daemon RPC failed'), {rpcCode:value.error.code})); else p.resolve(value.result);
      } else if (state.onFrame) state.onFrame(value);
    }
  });
  socket.on('error', (e) => { for (const p of state.pending.values()) p.reject(e); state.pending.clear(); });
  socket.on('close', () => { state.closed = true; for (const p of state.pending.values()) p.reject(new Error('daemon pipe closed')); state.pending.clear(); });
  await new Promise((resolve, reject) => { socket.once('connect', resolve); socket.once('error', reject); });
  const token = fs.readFileSync(path.join(dataDir, 'daemon.token'), 'utf8').trim();
  const handshake = new Promise((resolve, reject) => state.pending.set('0', {resolve, reject}));
  socket.write(frame({jsonrpc:'2.0', id:0, method:'handshake', params:{token, versions:[1], capabilities:['health','projects','tasks','sessions','events','approvals','workspaces','delivery','agents','coordinator','review','host','queue','workstreams']}}));
  const welcome = await handshake;
  return {state, welcome};
}
function sendRpc(state, method, params) {
  const id = ++state.seq;
  return new Promise((resolve, reject) => {
    state.pending.set(String(id), {resolve, reject});
    state.socket.write(frame({jsonrpc:'2.0', id, method, params}), (e) => { if (e) { state.pending.delete(String(id)); reject(e); } });
  });
}
async function directRpc(method, params) {
  const {state} = await openPipe(null);
  try { return await sendRpc(state, method, params); }
  finally { state.socket.destroy(); }
}
function errText(e) { return (e && e.message ? e.message : String(e)).replace(/[\r\n]+/g,' ').slice(0,240); }
function startDaemon() {
  const logFd = fs.openSync(path.join(out, 'WIN-BP-daemon-proxy.stderr.raw.txt'), 'a');
  const child = spawn(daemonExe, ['--data-dir', dataDir], {detached:true, stdio:['ignore','ignore',logFd], windowsHide:true});
  child.unref();
  fs.writeFileSync(f('WIN-BP-daemon-restart.txt'), 'method=browser-proxy\nrestart_pid=' + child.pid + '\ncommand=orchestrator-daemon.exe --data-dir [ISOLATED-RUN-DIR]\n');
  return child.pid;
}
async function waitForPipe(timeout=15000) {
  const end=Date.now()+timeout;
  while(Date.now()<end){
    try { const c=await openPipe(null); c.state.socket.destroy(); return true; } catch {};
    await pause(200);
  }
  return false;
}

const callbacks = new Map();
let nextCallback=1;
let bridgeSeq=0;
const bridgePending=new Map();
let wss, vite, browser, context, page, wsUrl, viteUrl;
const pipes=new Map();
let n08Ready=false;
async function main() {
  fs.copyFileSync(__filename, f('WIN-BP-playwright-proxy.cjs'));
  fs.writeFileSync(f('WIN-BP-method.txt'), 'method=browser-proxy\nbrowser=Microsoft Edge via Playwright channel msedge\ntransport=real isolated daemon over Windows named pipe\nscript=WIN-BP-playwright-proxy.cjs\n');
  const server = net.createServer();
  await new Promise((resolve,reject)=>server.listen(0,'127.0.0.1',resolve).once('error',reject));
  const port=server.address().port; server.close();
  wss = new WebSocketServer({host:'127.0.0.1',port,verifyClient:(info)=>info.origin===viteUrl && info.req.url==='/bridge/'+nonce});
  await new Promise((resolve)=>wss.once('listening',resolve));
  wsUrl='ws://127.0.0.1:'+port+'/bridge/'+nonce;
  wss.on('connection',(ws)=>{
    ws.on('message',async raw=>{
      let msg; try{msg=JSON.parse(raw.toString())}catch{return}
      const reply=(value)=>{if(ws.readyState===WebSocket.OPEN)ws.send(JSON.stringify({requestId:msg.requestId,ok:true,value}))};
      const fail=(e)=>{if(ws.readyState===WebSocket.OPEN)ws.send(JSON.stringify({requestId:msg.requestId,ok:false,error:{code:'io',message:errText(e)}}))};
      try {
        if(msg.op==='invoke' && msg.command==='daemon_open'){
          const cbId=msg.callbackId;
          const opened=await openPipe((value)=>{if(cbId!==null&&cbId!==undefined&&ws.readyState===WebSocket.OPEN)ws.send(JSON.stringify({type:'frame',callbackId:cbId,index:openedIndex++,payload:{kind:'message',message:value}}))});
          let openedIndex=0;
          const id=1; pipes.set(id,{...opened,ws,cbId,idx:0});
          reply({connectionId:id,welcome:opened.welcome});
        } else if(msg.op==='invoke' && msg.command==='daemon_send') {
          const c=pipes.get(msg.connectionId); if(!c||c.state.closed)throw new Error('unknown or closed connection');
          c.state.socket.write(frame(msg.message),(e)=>e?fail(e):reply(null));
        } else if(msg.op==='invoke' && msg.command==='daemon_close') {
          const c=pipes.get(msg.connectionId); if(c){pipes.delete(msg.connectionId);c.state.socket.end();} reply(null);
        } else if(msg.op==='status') { reply({pipeHash:pipe.hash,activePipes:pipes.size}); }
        else throw new Error('unsupported Tauri invoke '+msg.command);
      } catch(e){fail(e)}
    });
    ws.on('close',()=>{for(const [id,c] of pipes){if(c.ws===ws){pipes.delete(id);c.state.socket.destroy()}}});
  });

  const npm=spawn(process.execPath,[npmCli,'run','dev','--','--host','127.0.0.1','--port','5179','--strictPort'],{cwd:desktopDir,windowsHide:true,stdio:['ignore','pipe','pipe']});
  let viteText='';
  npm.stdout.on('data',d=>viteText+=d.toString()); npm.stderr.on('data',d=>viteText+=d.toString());
  vite=npm;
  viteUrl='http://127.0.0.1:5179';
  let ready=false;
  for(let i=0;i<100;i++){try{const res=await fetch(viteUrl);if(res.ok){ready=true;break}}catch{};await pause(250)}
  if(!ready)throw new Error('Vite did not become ready: '+viteText.slice(-500));
  const projectDir=path.join(temp,'isolated test project'); fs.mkdirSync(projectDir,{recursive:true});
  execFileSync('git',['init','--quiet',projectDir],{windowsHide:true});
  const project=await directRpc('projects.register',{path:projectDir});
  const stream=await directRpc('workstreams.create',{title:'Browser proxy verification',summary:'Ephemeral test workstream',idempotency_key:crypto.randomUUID()});
  calls.push({method:'projects.register',result:'created-isolated-project'});
  calls.push({method:'workstreams.create',result:'created-isolated-workstream'});
  const app = await chromium.launch({channel:'msedge',headless:true,args:['--no-first-run','--disable-features=msEdgeSidebarV2'],executablePath:undefined});
  browser=app; browserVersion=await browser.version();
  context=await browser.newContext({viewport:{width:1280,height:900},reducedMotion:'no-preference',colorScheme:'light'});
  page=await context.newPage();
  const consoleErrors=[]; page.on('console',m=>{if(m.type()==='error')consoleErrors.push(m.text().slice(0,160))}); page.on('pageerror',e=>consoleErrors.push('pageerror:'+errText(e)));
  await page.addInitScript((url)=>{
    const callbacks=new Map();let cb=0;let seq=0;const pending=new Map();let ws;
    const ensure=()=>new Promise((resolve,reject)=>{
      if(ws&&ws.readyState===WebSocket.OPEN){resolve();return}
      if(ws&&ws.readyState===WebSocket.CONNECTING){ws.addEventListener('open',()=>resolve(),{once:true});ws.addEventListener('error',()=>reject(new Error('proxy websocket failed')),{once:true});return}
      ws=new WebSocket(url);ws.addEventListener('open',()=>resolve(),{once:true});ws.addEventListener('error',()=>reject(new Error('proxy websocket failed')),{once:true});
      ws.addEventListener('message',ev=>{const m=JSON.parse(ev.data);if(m.type==='frame'){const f=callbacks.get(m.callbackId);if(f)f({index:m.index,message:m.payload});return}const p=pending.get(m.requestId);if(!p)return;pending.delete(m.requestId);m.ok?p.resolve(m.value):p.reject(m.error)});
    });
    const call=(body)=>ensure().then(()=>new Promise((resolve,reject)=>{const requestId=++seq;pending.set(requestId,{resolve,reject});ws.send(JSON.stringify({...body,requestId}))}));
    window.__TAURI_INTERNALS__={transformCallback(fn){const id=++cb;callbacks.set(id,fn);return id},unregisterCallback(id){callbacks.delete(id)},invoke(command,args={}){
      if(command==='daemon_open')return call({op:'invoke',command,requested:args.requested,callbackId:args.onFrame?.id??null});
      if(command==='daemon_send')return call({op:'invoke',command,connectionId:args.connectionId,message:args.message});
      if(command==='daemon_close')return call({op:'invoke',command,connectionId:args.connectionId});
      return Promise.reject({code:'io',message:'unsupported proxy command'});
    }};
  },wsUrl);
  await page.goto(viteUrl+'/#/workspace',{waitUntil:'domcontentloaded',timeout:30000});
  await page.waitForFunction(()=>document.body.innerText.includes('Connected'),null,{timeout:20000});

  if(process.env.WIN3_N08_ONLY!=='1'){
  const n02=[];
  await page.emulateMedia({colorScheme:'dark'}); await page.evaluate(()=>localStorage.removeItem('agent-orch.theme')); await page.reload(); await page.waitForTimeout(800);
  n02.push({case:'system-dark',theme:await page.locator('html').getAttribute('data-theme'),scheme:await page.evaluate(()=>matchMedia('(prefers-color-scheme: dark)').matches)});
  await page.screenshot({path:f('WIN-BP-N02-system-dark-rerun.png'),fullPage:true});
  await page.getByRole('button',{name:/Dark mode/}).click(); await page.waitForTimeout(200); await page.getByRole('button',{name:/Dark mode/}).click(); await page.waitForTimeout(300);
  n02.push({case:'explicit-dark',pressed:await page.getByRole('button',{name:/Dark mode/}).getAttribute('aria-pressed'),saved:await page.evaluate(()=>localStorage.getItem('agent-orch.theme'))});
  await page.reload(); await page.waitForTimeout(500); n02.push({case:'saved-dark',saved:await page.evaluate(()=>localStorage.getItem('agent-orch.theme')),pressed:await page.getByRole('button',{name:/Dark mode/}).getAttribute('aria-pressed')});
  await page.screenshot({path:f('WIN-BP-N02-explicit-saved-dark-rerun.png'),fullPage:true});
  await page.emulateMedia({colorScheme:'light'}); await page.evaluate(()=>localStorage.removeItem('agent-orch.theme')); await page.reload(); await page.waitForTimeout(700);
  n02.push({case:'system-light',theme:await page.locator('html').getAttribute('data-theme'),scheme:await page.evaluate(()=>matchMedia('(prefers-color-scheme: dark)').matches)});
  await page.screenshot({path:f('WIN-BP-N02-system-light-rerun.png'),fullPage:true});
  const n02pass=n02.some(x=>x.case==='system-dark'&&x.scheme&&x.theme===null)&&n02.some(x=>x.case==='system-light'&&!x.scheme&&x.theme===null)&&n02.some(x=>x.case==='explicit-dark'&&x.pressed==='true'&&x.saved==='dark')&&n02.some(x=>x.case==='saved-dark'&&x.saved==='dark'&&x.pressed==='true');
  calls.push({row:'WIN-BP-N02',observations:n02}); row('N02',n02pass?'PASS':'FAIL','browser-proxy theme cases '+(n02pass?'passed':'did not all pass'),['WIN-BP-N02-system-dark-rerun.png','WIN-BP-N02-explicit-saved-dark-rerun.png','WIN-BP-N02-system-light-rerun.png']);

  if(process.env.WIN3_RERUN_FAILED_ONLY!=='1'){
  const sizes=[];
  for(const width of [1280,1150,960,760,480])for(const height of [700,900]){
    await page.setViewportSize({width,height}); await page.locator('.shell-skip').focus(); await page.keyboard.press('Enter'); await page.waitForTimeout(120);
    const obs=await page.evaluate(()=>{
      const root=document.documentElement,main=document.querySelector('#shell-main');
      const a=document.activeElement, r=a?.getBoundingClientRect();const s=document.querySelector('.conn-strip');const sr=s?.getBoundingClientRect();
      let obscured=false;if(r&&sr){const x=Math.max(r.left,sr.left),y=Math.max(r.top,sr.top),rr=Math.min(r.right,sr.right),bb=Math.min(r.bottom,sr.bottom);const hit=document.elementFromPoint((x+rr)/2,(y+bb)/2);obscured=rr>x&&bb>y&&!!hit&&(hit===s||s.contains(hit));}
      return {innerWidth,innerHeight,scrollWidth:root.scrollWidth,overflow:root.scrollWidth>innerWidth,activeId:a?.id||a?.tagName,focusVisible:!!a?.matches(':focus-visible'),connectionStripFound:!!s,focusObscured:obscured};
    });
    sizes.push(obs); await page.screenshot({path:f('WIN-BP-N03-'+width+'x'+height+'.png'),fullPage:true});
  }
  const n03pass=sizes.length===10&&sizes.every(x=>!x.overflow&&!x.focusObscured&&x.focusVisible);
  calls.push({row:'WIN-BP-N03',viewports:sizes}); row('N03',n03pass?'PASS':'FAIL','browser-proxy 10 viewport cases; '+sizes.filter(x=>x.overflow).length+' overflow; '+sizes.filter(x=>x.focusObscured).length+' focus-obscured; '+sizes.filter(x=>!x.focusVisible).length+' focus-not-visible',['WIN-BP-N03-*.png']);
  }

  const keyObs={};
  await page.setViewportSize({width:1280,height:900}); await page.goto(viteUrl+'/#/workspace',{waitUntil:'domcontentloaded'}); await page.waitForTimeout(700);
  await page.locator('.shell-skip').focus(); keyObs.skipLink=await page.evaluate(()=>({text:document.activeElement?.textContent?.trim(),href:document.activeElement?.getAttribute('href')}));
  await page.keyboard.press('Enter'); keyObs.skipMain=await page.evaluate(()=>({id:document.activeElement?.id,tag:document.activeElement?.tagName})); const nav=page.getByRole('link',{name:'Agents & Sessions'}); await nav.focus(); await page.keyboard.press('Enter'); keyObs.navRoute=await page.evaluate(()=>location.hash);
  await page.goto(viteUrl+'/#/workstreams',{waitUntil:'domcontentloaded'}); await page.waitForTimeout(900);
  const tree=page.getByRole('treeitem'); keyObs.treeCount=await tree.count();
  if(keyObs.treeCount){await tree.first().focus();await page.keyboard.press('ArrowDown');keyObs.treeDown=await page.evaluate(()=>({role:document.activeElement?.getAttribute('role'),text:document.activeElement?.getAttribute('aria-label')}));}
  await page.goto(viteUrl+'/#/workspace',{waitUntil:'domcontentloaded'}); await page.waitForTimeout(700);
  const target=page.getByRole('button',{name:/Choose conversation target/}); keyObs.targetButton=await target.count();
  if(keyObs.targetButton){await target.focus();await page.keyboard.press('Enter');await page.waitForTimeout(150);keyObs.targetOpened=await page.getByRole('radiogroup',{name:'Conversation targets'}).count();await page.keyboard.press('Escape');keyObs.targetRestored=await target.evaluate(el=>el===document.activeElement);}
  await page.goto(viteUrl+'/#/review',{waitUntil:'domcontentloaded'}); await page.waitForTimeout(700);
  const tabs=page.getByRole('tablist',{name:'Decision filters'}); keyObs.reviewTabs=await tabs.count();
  if(keyObs.reviewTabs){const first=tabs.getByRole('tab').first();await first.focus();await page.keyboard.press('ArrowRight');keyObs.reviewActive=await page.evaluate(()=>({role:document.activeElement?.getAttribute('role'),selected:document.activeElement?.getAttribute('aria-selected'),text:document.activeElement?.textContent?.trim()}));await page.screenshot({path:f('WIN-BP-N04-keyboard-rerun.png'),fullPage:true});}
  const n04pass=keyObs.skipLink?.href==='#shell-main'&&keyObs.skipMain?.id==='shell-main'&&keyObs.navRoute==='#/agents'&&keyObs.treeCount>0&&keyObs.targetButton===1&&keyObs.targetOpened>0&&keyObs.targetRestored&&keyObs.reviewTabs===1&&keyObs.reviewActive?.role==='tab';
  calls.push({row:'WIN-BP-N04',keyboard:keyObs}); row('N04',n04pass?'PASS':'FAIL','browser-proxy keyboard traversal '+(n04pass?'passed':'had missing or incorrect controls'),['WIN-BP-N04-keyboard-rerun.png']);

  await page.emulateMedia({reducedMotion:'reduce'}); await page.goto(viteUrl+'/#/workspace',{waitUntil:'domcontentloaded'}); await page.waitForTimeout(700);
  if(process.env.WIN3_RERUN_FAILED_ONLY!=='1'){
  const bg=page.getByRole('button',{name:/Background/}); if(await bg.count())await bg.click(); await page.waitForTimeout(800);
  const n06=await page.evaluate(()=>({matches:matchMedia('(prefers-reduced-motion: reduce)').matches,ambient:!!document.querySelector('.ambient-background'),engine:document.querySelector('[data-testid="ambient-background"]')?.getAttribute('data-engine')||null,animated:!!document.querySelector('.ambient-background *')&&[...document.querySelectorAll('.ambient-background *')].some(e=>getComputedStyle(e).animationDuration!=='0s'&&getComputedStyle(e).animationName!=='none')}));
  await page.screenshot({path:f('WIN-BP-N06-reduced-motion.png'),fullPage:true}); const n06pass=n06.matches&&n06.ambient&&n06.engine==='static';
  calls.push({row:'WIN-BP-N06',observations:n06}); row('N06',n06pass?'PASS':'FAIL','browser-proxy reduced motion '+(n06pass?'enabled and rendered':'not confirmed'),['WIN-BP-N06-reduced-motion.png']);
  }

  if(process.env.WIN3_RERUN_FAILED_ONLY!=='1'){
  const routeChecks=[];
  const routes=['#/workspace','#/workstreams','#/agents','#/review','#/projects','#/project/'+encodeURIComponent(project.id),'#/project/'+encodeURIComponent(project.id)+'/tasks/all','#/project/'+encodeURIComponent(project.id)+'/coordinator'];
  for(const hash of routes){await page.goto(viteUrl+'/'+hash,{waitUntil:'domcontentloaded'});await page.waitForTimeout(280);const actual=await page.evaluate(()=>location.hash);const safeRoute=hash.includes(encodeURIComponent(project.id))?hash.replaceAll(encodeURIComponent(project.id),'<ISOLATED_PROJECT>'):hash;routeChecks.push({route:safeRoute,matched:actual===hash,main:await page.locator('#shell-main').count()});}
  const unknown='#/nope'; await page.goto(viteUrl+'/'+unknown,{waitUntil:'domcontentloaded'});await page.waitForTimeout(250); const unknownActual=await page.evaluate(()=>location.hash);
  await page.goto(viteUrl+'/#/workspace',{waitUntil:'domcontentloaded'});await page.goto(viteUrl+'/#/agents',{waitUntil:'domcontentloaded'});await page.goBack();const back=await page.evaluate(()=>location.hash);await page.goForward();const forward=await page.evaluate(()=>location.hash);
  const taskRoutes='BLOCKED: no real task ID existed in the isolated daemon; tasks.submit wakes executor and is intentionally not used because live-agent checks require the already-signed-in pinned CLI.';
  await page.goto(viteUrl+'/#/review',{waitUntil:'domcontentloaded'});await page.waitForTimeout(500);await page.screenshot({path:f('WIN-BP-N07-routes.png'),fullPage:true});
  const n07pass=routeChecks.every(x=>x.matched&&x.main===1)&&unknownActual==='#/workspace'&&back==='#/workspace'&&forward==='#/agents';
  calls.push({row:'WIN-BP-N07',routes:routeChecks,unknownActual,back,forward,taskRoutes}); row('N07',n07pass?'BLOCKED':'FAIL','browser-proxy globals/available legacy routes, unknown fallback and back/forward '+(n07pass?'passed; task and task-review legacy routes blocked because no actual isolated task ID was available':'failed')+'; '+taskRoutes,['WIN-BP-N07-routes.png']);
  }

  }
  if(process.env.WIN3_N11_ONLY!=='1'){
  await page.goto(viteUrl+'/#/workspace',{waitUntil:'domcontentloaded'});await page.waitForTimeout(900);
  const beforeStop=await page.evaluate(()=>({connected:document.body.innerText.includes('Connected'),body:document.body.innerText.slice(0,1400),placeholders:/PLACEHOLDER|fixture:/i.test(document.body.innerText)}));
  await page.screenshot({path:f('WIN-BP-N08-connected-before-stop-rerun2.png'),fullPage:true});
  fs.writeFileSync(f('WIN-BP-N08-ready-rerun2.txt'),'method=browser-proxy\napplication_connected='+beforeStop.connected+'\npipe_hash='+pipe.hash+'\noperator_action=stop only the verified initial daemon PID from the isolated run, then create WIN-BP-N08-stop-verified-rerun2.txt\n'); n08Ready=true;
  const readyPath=f('WIN-BP-N08-stop-verified-rerun2.txt'); const waitUntil=Date.now()+90000;
  while(!fs.existsSync(readyPath)&&Date.now()<waitUntil)await pause(200);
  if(!fs.existsSync(readyPath))throw new Error('timed out waiting for exact isolated daemon stop verification');
  await page.waitForTimeout(1800); const stale=await page.evaluate(()=>({body:document.body.innerText.slice(0,1600),placeholders:/PLACEHOLDER|fixture:/i.test(document.body.innerText),connected:document.body.innerText.includes('Connected'),reconnecting:/Reconnecting|Disconnected|stale/i.test(document.body.innerText)}));
  await page.screenshot({path:f('WIN-BP-N08-stale-after-stop-rerun2.png'),fullPage:true});
  const restartPid=startDaemon(); const restartReady=await waitForPipe(15000); await page.waitForFunction(()=>document.body.innerText.includes('Connected'),null,{timeout:25000});await page.waitForTimeout(700);
  const recovered=await page.evaluate(()=>({connected:document.body.innerText.includes('Connected'),body:document.body.innerText.slice(0,1600),placeholders:/PLACEHOLDER|fixture:/i.test(document.body.innerText)}));
  await page.screenshot({path:f('WIN-BP-N08-reconnected-after-restart-rerun2.png'),fullPage:true});
  const n08pass=beforeStop.connected&&restartReady&&recovered.connected&&!stale.placeholders&&!recovered.placeholders;
  calls.push({row:'WIN-BP-N08',initial:beforeStop,afterStop:stale,recovered,restartPid,pipeHash:pipe.hash}); row('N08',n08pass?'PASS':'FAIL','browser-proxy named-pipe stop/restart '+(n08pass?'recovered without fixture fallback':'failed or fixture fallback observed'),['WIN-BP-N08-connected-before-stop-rerun2.png','WIN-BP-N08-stale-after-stop-rerun2.png','WIN-BP-N08-reconnected-after-restart-rerun2.png']);

  }
  await page.goto(viteUrl+'/#/workspace',{waitUntil:'domcontentloaded'});await page.waitForTimeout(700);
  const n11=await page.evaluate(()=>{const t=document.body.innerText;const resourceLine=t.split(/\r?\n/).find(x=>/^CPU\s/.test(x))||null;return {connected:t.includes('Connected'),usageUnavailable:/not available/i.test(t),placeholderBadge:/PLACEHOLDER DATA/i.test(t),fabricatedDollar:/\$26\.00|\$0(?:\.00)?/.test(t),fabricatedCounts:/20\/24|CPU 41\/44/i.test(t),resourceLine}});
  const n11DaemonMetrics=await directRpc('host.metrics',null);const byMeter=Object.fromEntries(n11DaemonMetrics.meters.map(m=>[m.meter,m]));const pct=m=>m&&m.state==='ok'?(m.kind==='capacity'&&m.total?m.value/m.total*100:m.value):null;
  const ui={};for(const [key,re] of Object.entries({cpu:/CPU\s+([\d.]+)%/i,memory:/Memory\s+([\d.]+)%/i,disk:/Disk\s+([\d.]+)%/i})){const m=n11.resourceLine?.match(re);ui[key]=m?Number(m[1]):null}const gpuUnsupported=/GPU\s+unsupported/i.test(n11.resourceLine||'');
  const daemon={cpu:pct(byMeter.cpu),memory:pct(byMeter.memory),disk:pct(byMeter.disk),gpuState:byMeter.gpu?.state||null,gpuReason:byMeter.gpu?.reason||null};const compare={ui,daemon,delta:{cpu:ui.cpu===null||daemon.cpu===null?null:Math.abs(ui.cpu-daemon.cpu),memory:ui.memory===null||daemon.memory===null?null:Math.abs(ui.memory-daemon.memory),disk:ui.disk===null||daemon.disk===null?null:Math.abs(ui.disk-daemon.disk)},gpuUnsupported,pass:null};compare.pass=compare.delta.cpu!==null&&compare.delta.cpu<=15&&compare.delta.memory!==null&&compare.delta.memory<=5&&compare.delta.disk!==null&&compare.delta.disk<=1&&gpuUnsupported&&daemon.gpuState==='unsupported';
  fs.writeFileSync(f('WIN-BP-N11-host-metrics-response.json'),JSON.stringify({method:'browser-proxy',rpc:'host.metrics',pipe_hash:pipe.hash,queriedAt:new Date().toISOString(),result:n11DaemonMetrics},null,2)+'\n');fs.writeFileSync(f('WIN-BP-N11-comparison.json'),JSON.stringify({method:'browser-proxy',observedAt:new Date().toISOString(),...compare},null,2)+'\n');
  await page.screenshot({path:f('WIN-BP-N11-live-data-honesty-rerun.png'),fullPage:true});
  const n11pass=n11.connected&&n11.usageUnavailable&&!n11.placeholderBadge&&!n11.fabricatedDollar&&!n11.fabricatedCounts&&compare.pass;
  calls.push({row:'WIN-BP-N11',observations:n11,comparison:compare}); row('N11',n11pass?'PASS':'FAIL','browser-proxy live-data honesty '+(n11pass?'metrics matched without a placeholder badge':'placeholder badge or another required assertion failed'),['WIN-BP-N11-live-data-honesty-rerun.png','WIN-BP-N11-comparison.json','WIN-BP-N11-host-metrics-response.json']);
  calls.push({consoleErrorCount:consoleErrors.length,consoleErrors:consoleErrors.slice(0,20)});
  if(process.env.WIN3_RERUN_FAILED_ONLY!=='1')fs.writeFileSync(f('WIN-BP-route-and-keyboard.txt'),JSON.stringify({method:'browser-proxy',routes:routeChecks,unknownActual,back,forward,taskRoutes,keyboard:keyObs},null,2));
  fs.writeFileSync(f('WIN-BP-proxy-log.txt'),'method=browser-proxy\nbrowser=Microsoft Edge '+browserVersion+'\npipe_hash='+pipe.hash+'\nproject=created in isolated run\nworkstream=created in isolated run\nrows='+rows.map(r=>r.id+':'+r.status).join(', ')+'\nconsole_errors='+consoleErrors.length+'\n');
  fs.writeFileSync(f('WIN-BP-screenshots-index.txt'),rows.map(r=>r.id+' | browser-proxy | '+r.evidence.join(', ')).join('\n')+'\n');
  await browser.close(); browser=null;
  if(vite){vite.kill();vite=null;}
  for(const c of pipes.values())c.state.socket.destroy(); pipes.clear();
  wss.close();
  fs.writeFileSync(f('WIN-BP-script-exit.txt'),'exit=0\n');
}
main().catch(async e=>{
  fatal=errText(e); fs.writeFileSync(f('WIN-BP-script-error.txt'),'method=browser-proxy\nerror='+fatal+'\n');
  if(!n08Ready){try{for(const id of ['N02','N03','N04','N06','N07','N08','N11'])if(!rows.some(r=>r.id==='WIN-BP-'+id))row(id,'FAIL','browser-proxy harness failed: '+fatal,[])}catch{}}
  process.exitCode=1;
}).finally(async()=>{try{if(browser)await browser.close()}catch{};try{if(vite)vite.kill()}catch{};try{if(wss)wss.close()}catch{};for(const c of pipes.values())try{c.state.socket.destroy()}catch{}});
