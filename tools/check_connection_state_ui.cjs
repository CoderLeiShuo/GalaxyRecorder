const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const assert=require('node:assert/strict');const fs=require('node:fs');
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH||'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'});
 try {
 const page=await browser.newPage({viewport:{width:390,height:844},userAgent:'Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/140.0.0.0 Mobile Safari/537.36'});
 const errors=[],checks=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{
  globalThis.isTauri=true;let next=0;const callbacks=new Map(),listeners=new Map();
  const recorder={address:'192.168.43.8',protocol:'A7',firmware:'test',vehicle:'星越 L',networkId:'car-wifi'};
  const video={id:'remote',name:'20261028_080000.mp4',path:'/mnt/mmc/Normal/20261028_080000.mp4',date:'2026-10-28',time:'08:00:00',category:'normal',size:1024,duration:12};
  const m=globalThis.connectionStateMock={calls:[],health:{connected:true},hold:false,finish:null,holdDiscovery:false,finishDiscovery:null,discoveryFails:false,tasks:[],saved:[{...video,id:'saved',savedPath:'/fixture/saved.mp4'}],
   emit:(event,payload)=>{for(const[id,l]of listeners)if(l.event===event)callbacks.get(l.handler)?.({id,event,payload})}};
  globalThis.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:(_,id)=>listeners.delete(id)};
  globalThis.__TAURI_INTERNALS__={transformCallback:fn=>{callbacks.set(++next,fn);return next;},unregisterCallback:id=>callbacks.delete(id),invoke:async(command,args={})=>{
   m.calls.push({command,args});
   if(command==='plugin:event|listen'){listeners.set(++next,args);return next;}
   if(['plugin:event|unlisten','cancel_recorder_discovery','record_client_event'].includes(command))return;
   if(command==='list_saved')return m.saved;
   if(command==='list_transfers')return m.tasks;
   if(command==='get_settings')return {saveMode:'gallery',keepScreenOn:true};
   if(command==='get_recorder_network')return {platform:'android',available:true,bound:true,selected:{id:'car-wifi'}};
   if(command==='probe_recorder'){if(!m.health.connected)throw '未响应';return recorder;}
   if(command==='discover_recorder'){if(m.discoveryFails)throw '未响应';return m.holdDiscovery?new Promise(resolve=>{m.finishDiscovery=resolve;}):recorder;}
   if(command==='check_recorder_connection')return m.hold?new Promise(resolve=>{m.finish=resolve;}):m.health;
   if(command==='list_videos')return {videos:args.category==='normal'?[video]:[],warning:null};
   if(command==='video_metadata')return {duration:12,thumbnail:'/fixture-thumbnail.png',size:1024,warning:null};
   if(command==='release_recorder_network'){m.tasks=m.tasks.map(t=>['waiting','downloading'].includes(t.status)?{...t,status:'failed'}:t);m.emit('transfer-state',m.tasks);return;}
   if(command==='native_media'){
    if(args.operation==='scan'||args.operation==='scanImage')return {text:'WIFI:T:WPA;S:Car;P:12345678;;'};
    if(args.operation==='connectWifi'){m.health={connected:true};return {};}
    return {};
   }
   if(command==='cancel_transfer'){m.tasks=m.tasks.map(t=>t.id===args.taskId?{...t,status:'paused'}:t);m.emit('transfer-state',m.tasks);return;}
   if(command==='retry_transfer'){m.tasks=m.tasks.map(t=>t.id===args.taskId?{...t,status:'waiting'}:t);return m.tasks;}
   if(command==='delete_transfer'){m.tasks=m.tasks.filter(t=>t.id!==args.taskId);return m.tasks;}
   throw Error('Unexpected mock command: '+command);
  }};
 });
 const connected=()=>page.locator('.glacier-connection-state.online').waitFor();
 const disconnected=()=>page.locator('.glacier-connection-state').filter({hasText:'未连接'}).waitFor();
 const count=(command)=>page.evaluate(c=>connectionStateMock.calls.filter(v=>v.command===c).length,command);
 const pull=async({selector='.glacier-home',dx=0,dy=120,cancel=false}={})=>page.evaluate(({selector,dx,dy,cancel})=>{
  const root=document.querySelector(selector);const touch=(x,y)=>new Touch({identifier:1,target:root,clientX:x,clientY:y});
  root.dispatchEvent(new TouchEvent('touchstart',{bubbles:true,cancelable:true,touches:[touch(150,100)]}));
  root.dispatchEvent(new TouchEvent('touchmove',{bubbles:true,cancelable:true,touches:[touch(150+dx,100+dy)]}));
  root.dispatchEvent(new TouchEvent(cancel?'touchcancel':'touchend',{bubbles:true,cancelable:true,touches:[]}));
 },{selector,dx,dy,cancel});
 await page.route('**/fixture-thumbnail.png',route=>route.fulfill({path:'tools/fixtures/thumbnail.png',contentType:'image/png'}));
 await page.goto('http://127.0.0.1:1420');await connected();
 assert.equal(await count('discover_recorder'),1);checks.push('first launch automatically connects once');
 const scans=await count('discover_recorder');
 await page.evaluate(()=>{connectionStateMock.hold=true;connectionStateMock.emit('recorder-network',{kind:'pause'});connectionStateMock.emit('recorder-network',{kind:'resume'});});
 await page.waitForFunction(()=>Boolean(connectionStateMock.finish));assert.equal(await page.locator('.glacier-connection-state.online').count(),1);
 await page.evaluate(()=>{connectionStateMock.hold=false;connectionStateMock.finish({connected:true});});
 assert.equal(await count('discover_recorder'),scans);checks.push('returning to app keeps valid connection while checking without rediscovery');
 await page.reload();await connected();assert.equal(await count('discover_recorder'),0);assert.ok(await count('check_recorder_connection')>=1);checks.push('reopening restores verified session without rediscovery');
 await page.evaluate(()=>connectionStateMock.emit('recorder-network',{kind:'lost',networkId:'old-wifi'}));assert.equal(await page.locator('.glacier-connection-state.online').count(),1);checks.push('ignores delayed loss for an old network');
 await page.evaluate(()=>{connectionStateMock.hold=true;connectionStateMock.emit('recorder-network',{kind:'resume'});});await page.waitForFunction(()=>Boolean(connectionStateMock.finish));
 await page.evaluate(()=>{connectionStateMock.health={connected:false};connectionStateMock.emit('recorder-network',{kind:'lost',networkId:'car-wifi'});});await disconnected();
 await page.evaluate(()=>{connectionStateMock.hold=false;connectionStateMock.finish({connected:true});});await page.waitForTimeout(100);await disconnected();checks.push('loss clears connection and ignores stale successful checks');
 const attempts=await count('probe_recorder')+await count('discover_recorder');
 await pull({dy:30});await pull({dx:70});await pull({cancel:true});await page.waitForTimeout(100);assert.equal(await count('probe_recorder')+await count('discover_recorder'),attempts);checks.push('short horizontal and cancelled drags do not reconnect');
 await page.evaluate(()=>connectionStateMock.health={connected:true});await pull({selector:'.glacier-product-stage img'});await page.locator('.glacier-product-stage img').evaluate(img=>img.click());await connected();assert.equal(await page.getByRole('dialog',{name:'更换记录仪图片',exact:true}).count(),0);checks.push('pulling the device picture reconnects without opening its image editor');
 await page.evaluate(()=>{connectionStateMock.health={connected:false,definitive:true};connectionStateMock.emit('recorder-network',{kind:'resume'});});await disconnected();
 await page.locator('.glacier-connect-button').click();const dialog=page.getByRole('dialog',{name:'连接记录仪',exact:true});
 assert.equal(await dialog.getByText(/IP|192\.168|个地址|自动查找|网段/).count(),0);
 await dialog.getByRole('button',{name:'扫码连接',exact:true}).click();await connected();await dialog.waitFor({state:'detached'});
 assert.ok(await page.evaluate(()=>connectionStateMock.calls.some(c=>c.command==='native_media'&&c.args.operation==='connectWifi')));checks.push('simple connection UI and QR connects hotspot then recorder automatically');
 await page.evaluate(()=>{
  const v=connectionStateMock.saved[0];connectionStateMock.tasks=[{id:'partial',video:{...v,savedPath:undefined},status:'downloading',received:512,total:1024},{id:'done',video:v,status:'done',received:1024,total:1024}];connectionStateMock.emit('transfer-state',connectionStateMock.tasks);
 });
 await page.locator('.nav-item').filter({hasText:'传输任务'}).click();
 await page.evaluate(()=>{const v=connectionStateMock.saved[0];connectionStateMock.tasks=[...connectionStateMock.tasks,{id:'long-failed',video:{...v,id:'long-video',name:'很长的录像任务名称_20261028_跨网络恢复.mp4',time:''},status:'failed',received:512,total:1024,error:'车机热点暂时断开，已保留已下载的数据，重新连接记录仪后可继续下载。'.repeat(2)}];connectionStateMock.emit('transfer-state',connectionStateMock.tasks);});
 await page.waitForFunction(()=>document.querySelectorAll('.transfer-card').length===3);
 for(const width of [320,360,390,414,1280]){
  await page.setViewportSize({width,height:844});
  const rows=await page.locator('.transfer-task-actions').evaluateAll(nodes=>nodes.map(node=>{const r=node.getBoundingClientRect();return {row:{x:r.x,right:r.right},buttons:[...node.querySelectorAll('button')].map(b=>{const x=b.getBoundingClientRect();return {x:x.x,y:x.y,right:x.right,height:x.height,width:x.width};})};}));
  assert.equal(rows.length,3);for(const row of rows){const [action,remove]=row.buttons;assert.equal(row.buttons.length,2);assert.ok(Math.abs(action.y-remove.y)<1);assert.ok(action.right<remove.x);assert.ok(action.x>=row.row.x&&remove.right<=row.row.right+1);assert.ok(action.height>=40&&remove.height>=40);}
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
 }
 checks.push('pause resume view and delete actions stay in one row at 320 360 390 414 and desktop widths');
 await page.setViewportSize({width:390,height:844});await page.evaluate(()=>scrollTo(0,0));await page.waitForTimeout(200);await page.screenshot({path:'analysis/transfers-0.8.5-mobile.png',animations:'disabled'});await page.evaluate(()=>scrollTo(0,document.documentElement.scrollHeight));await page.waitForTimeout(200);await page.locator('.transfer-card').nth(2).screenshot({path:'analysis/transfers-long-message-0.8.5-mobile.png',animations:'disabled'});checks.push('long error messages do not push transfer buttons onto separate lines');
 await page.getByRole('button',{name:'删除任务 很长的录像任务名称_20261028_跨网络恢复.mp4',exact:true}).click();await page.waitForFunction(()=>connectionStateMock.tasks.length===2);checks.push('failed task deletion retains other jobs and saved files');
 await page.getByRole('button',{name:'暂停',exact:true}).click();await page.getByText('已暂停',{exact:true}).waitFor();
 const paused=await page.locator('.transfer-card').first().locator('.transfer-task-actions button').evaluateAll(nodes=>nodes.map(n=>n.getBoundingClientRect().y));assert.ok(Math.abs(paused[0]-paused[1])<1);checks.push('paused task resume and delete actions remain aligned');
 await page.getByRole('button',{name:'继续下载',exact:true}).click();await page.getByText('等待下载',{exact:true}).waitFor();
 assert.equal(await page.evaluate(()=>connectionStateMock.tasks[0].received),512);checks.push('pause and resume preserve displayed byte progress');
 await page.getByRole('button',{name:'删除任务 20261028_080000.mp4',exact:true}).first().click();await page.waitForFunction(()=>connectionStateMock.tasks.length===1);
 await page.getByRole('button',{name:'删除任务 20261028_080000.mp4',exact:true}).click();await page.waitForFunction(()=>connectionStateMock.tasks.length===0);
 assert.equal(await page.evaluate(()=>connectionStateMock.saved.length),1);checks.push('individual task deletion keeps saved recordings');
 await page.locator('.nav-item').filter({hasText:'我的设备'}).click();await page.evaluate(()=>scrollTo(0,0));const healthyAttempts=await count('probe_recorder')+await count('discover_recorder');await pull();await page.waitForTimeout(100);await connected();assert.equal(await count('probe_recorder')+await count('discover_recorder'),healthyAttempts);checks.push('pull on a healthy connection does not probe or rediscover');
 assert.equal(await page.locator('.home-reconnect').count(),0);assert.equal(await page.getByText('下拉刷新，重新连接记录仪',{exact:true}).count(),0);checks.push('obsolete reconnect caption and button are absent');
 await page.locator('.device-manage-connection').click();await page.getByRole('button',{name:'断开连接',exact:true}).click();await disconnected();
 await page.evaluate(()=>{connectionStateMock.health={connected:false};connectionStateMock.holdDiscovery=true;});
 await page.locator('.glacier-connect-button').click();await page.getByRole('dialog',{name:'连接记录仪',exact:true}).getByRole('button',{name:'连接记录仪',exact:true}).click();
 await page.waitForFunction(()=>Boolean(connectionStateMock.finishDiscovery));
 await page.evaluate(()=>connectionStateMock.emit('recorder-network',{kind:'lost',networkId:'car-wifi'}));
 await page.evaluate(()=>connectionStateMock.finishDiscovery({address:'192.168.43.8',networkId:'car-wifi',protocol:'A7',firmware:'test',vehicle:'星越 L'}));
 await page.waitForTimeout(100);await disconnected();checks.push('loss during discovery rejects late success');
 await page.getByRole('button',{name:'关闭连接窗口',exact:true}).click();
 assert.deepEqual(errors,[]);await page.screenshot({path:'analysis/connection-0.8.5-mobile.png',fullPage:true});
 fs.writeFileSync('analysis/connection-state-ui-verification.json',JSON.stringify({mockedRecorder:true,checks,errors},null,2));console.log(JSON.stringify({passed:checks.length,checks}));
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exit(1);});
