const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');const assert=require('node:assert/strict');const fs=require('node:fs');
(async()=>{const browser=await chromium.launch({headless:true,executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'});try{
 const page=await browser.newPage({viewport:{width:390,height:844},userAgent:'Mozilla/5.0 (Linux; Android 16) AppleWebKit/537.36 Chrome/140.0.0.0 Mobile Safari/537.36'});const errors=[],checks=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{
  globalThis.isTauri=true;let id=0;const listeners=new Map(),callbacks=new Map();
  const video={id:'saved:native',name:'native.mp4',path:'/mnt/mmc/Normal/native.mp4',category:'normal',date:'2026-10-28',time:'10:00:00',size:2048,duration:12,savedPath:'content://media/external/video/media/77',storage:'gallery'};
  const m=globalThis.nativePlayerMock={calls:[],fullscreen:false,visible:true,action:null,playing:true,volume:.4,volumeSteps:15,position:6};
  globalThis.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:(_,id)=>listeners.delete(id)};
  globalThis.__TAURI_INTERNALS__={transformCallback:f=>{callbacks.set(++id,f);return id;},unregisterCallback:id=>callbacks.delete(id),invoke:async(command,args={})=>{
   m.calls.push({command,args});if(command==='plugin:event|listen'){listeners.set(++id,args);return id;}if(['plugin:event|unlisten','record_client_event','cancel_recorder_discovery'].includes(command))return;
   if(command==='get_settings')return {saveMode:'gallery',maskByDefault:false,keepScreenOn:false};
   if(command==='list_saved')return [video];if(command==='list_transfers')return [];
   if(command==='discover_recorder'||command==='probe_recorder')return {address:'192.168.43.8',networkId:'car',vehicle:'星越 L',protocol:'A7',firmware:'mock'};
   if(command==='check_recorder_connection')return {connected:true};if(command==='list_videos')return {videos:[],warning:null};
   if(command==='video_metadata')return {size:2048,duration:12,thumbnail:'/fixture-thumbnail.png',warning:null};
   if(command==='open_media')return {url:video.savedPath,nativeUrl:video.savedPath,live:false,size:2048};
   if(command==='share_saved_many')return;
   if(command==='native_media'){
    if(args.operation==='open'){m.fullscreen=false;await new Promise(resolve=>setTimeout(resolve,250));return {volume:m.volume,volumeSteps:m.volumeSteps};}
    if(args.operation==='control'){if(args.payload.action==='volume')m.volume=Math.round(args.payload.value*m.volumeSteps)/m.volumeSteps;if(args.payload.action==='fullscreen')m.fullscreen=args.payload.value>0;if(args.payload.action==='visible')m.visible=args.payload.value>0;if(args.payload.action==='pause')m.playing=false;if(args.payload.action==='play')m.playing=true;if(args.payload.action==='stop'){m.playing=false;m.position=0;}return {};}
    if(args.operation==='status'){const action=m.action;m.action=null;return {fullscreen:m.fullscreen,position:m.position,volume:m.volume,volumeSteps:m.volumeSteps,duration:12,playing:m.playing,buffering:false,error:null,exportProgress:0,action};}
    if(args.operation==='close'){m.fullscreen=false;return {};}
    return {};
   }
   throw Error('Unexpected mock command '+command);
  }};
 });
 await page.route('**/fixture-thumbnail.png',route=>route.fulfill({path:'tools/fixtures/thumbnail.png',contentType:'image/png'}));
 await page.goto('http://127.0.0.1:1420');await page.locator('.glacier-connection-state.online').waitFor();await page.locator('.glacier-connect-button').click();await page.locator('.video-image').click();
 const player=page.getByRole('dialog',{name:'录像播放器',exact:true});await page.waitForFunction(()=>nativePlayerMock.calls.some(c=>c.command==='native_media'&&c.args.operation==='status'));assert.equal(await page.evaluate(()=>nativePlayerMock.fullscreen),false);assert.equal((await player.getAttribute('class')).includes('player-expanded'),false);
 const calls=await page.evaluate(()=>nativePlayerMock.calls);const opened=calls.findIndex(c=>c.command==='native_media'&&c.args.operation==='open');assert.ok(opened>=0);assert.equal(calls[opened].args.payload.source,'content://media/external/video/media/77');assert.equal(calls[opened].args.payload.keepScreenOn,true);assert.equal(calls.some(c=>c.command==='native_media'&&c.args.operation==='control'&&c.args.payload.action==='fullscreen'&&c.args.payload.value===1),false);checks.push('gallery URI opens through native decoder in ordinary view');
 assert.equal(await player.getByRole('button',{name:'停止',exact:true}).count(),0);await player.getByRole('button',{name:'播放速度',exact:true}).click();await player.getByRole('button',{name:'4×',exact:true}).click();assert.ok(await page.evaluate(()=>nativePlayerMock.calls.some(c=>c.command==='native_media'&&c.args.operation==='control'&&c.args.payload.action==='speed'&&c.args.payload.value===4)));checks.push('native player removes stop and sends 4x playback speed');
 assert.equal(calls.some(c=>c.command==='native_media'&&c.args.operation==='control'&&c.args.payload.action==='volume'),false);assert.equal(await page.evaluate(()=>nativePlayerMock.volume),.4);await page.waitForFunction(()=>Math.abs(Number(document.querySelector('.volume-slider')?.value)-.4)<.001);checks.push('opening video reads system volume without changing it');
 const volume=player.locator('.volume-slider');assert.ok(Math.abs(Number(await volume.getAttribute('step'))-1/15)<.001);await page.setViewportSize({width:1000,height:844});await volume.focus();await page.keyboard.press('ArrowRight');await page.waitForFunction(()=>Math.abs(nativePlayerMock.volume-7/15)<.001);await page.setViewportSize({width:390,height:844});checks.push('slider changes system volume in actual hardware steps');
 await page.evaluate(()=>nativePlayerMock.volume=.6);await page.waitForFunction(()=>Math.abs(Number(document.querySelector('.volume-slider')?.value)-.6)<.001);checks.push('hardware volume keys and native gestures synchronize with React controls');
 await player.getByRole('button',{name:'静音',exact:true}).click();await page.waitForFunction(()=>nativePlayerMock.volume===0);await player.getByRole('button',{name:'取消静音',exact:true}).click();await page.waitForFunction(()=>nativePlayerMock.volume===.6);checks.push('unmute restores last audible level without setting maximum volume');
 await player.getByRole('button',{name:'全屏',exact:true}).click();await page.waitForFunction(()=>nativePlayerMock.fullscreen===true);await page.waitForFunction(()=>document.querySelector('.custom-player-dialog')?.classList.contains('player-expanded'));checks.push('only explicit fullscreen button opens native fullscreen');
 assert.equal(await player.getByRole('button',{name:'放大画面',exact:true}).count(),0);checks.push('native player has no zoom button');
 await page.evaluate(()=>nativePlayerMock.action='menu');const drawer=page.getByRole('dialog',{name:'录像文件操作',exact:true});await drawer.waitFor();assert.equal(await page.evaluate(()=>nativePlayerMock.visible),false);assert.equal(await page.evaluate(()=>nativePlayerMock.playing),false);checks.push('native more action pauses and hides video behind file drawer');
 await drawer.getByRole('button',{name:'分享视频',exact:true}).click();assert.ok(await page.evaluate(()=>nativePlayerMock.calls.some(c=>c.command==='share_saved_many'&&c.args.ids[0]==='saved:native')));checks.push('shares gallery file from drawer');
 await drawer.getByRole('button',{name:'关闭录像菜单',exact:true}).click();await page.waitForFunction(()=>nativePlayerMock.visible===true);assert.equal(await player.locator('.custom-player-heading').isVisible(),true);checks.push('closing drawer restores ordinary player');
 await player.getByRole('button',{name:'关闭播放器',exact:true}).click();await page.waitForFunction(()=>nativePlayerMock.calls.some(c=>c.command==='native_media'&&c.args.operation==='close'));checks.push('closing player releases native surface');
 await player.waitFor({state:'hidden'});await page.evaluate(()=>{nativePlayerMock.volume=0;nativePlayerMock.calls=[];});await page.locator('.video-image').click();await player.waitFor();await page.waitForFunction(()=>nativePlayerMock.calls.some(c=>c.command==='native_media'&&c.args.operation==='status'));assert.equal(await page.evaluate(()=>nativePlayerMock.volume),0);assert.equal(await page.evaluate(()=>nativePlayerMock.calls.some(c=>c.command==='native_media'&&c.args.operation==='control'&&c.args.payload.action==='volume')),false);checks.push('opening a video while system is muted preserves mute');
 await player.getByRole('button',{name:'取消静音',exact:true}).click();await page.waitForFunction(()=>nativePlayerMock.volume>0&&nativePlayerMock.volume<1);checks.push('a muted launch uses a moderate fallback when explicitly unmuted');await player.getByRole('button',{name:'关闭播放器',exact:true}).click();assert.deepEqual(errors,[]);
 fs.writeFileSync('analysis/native-player-ui-verification.json',JSON.stringify({mockedNativeDecoder:true,checks,errors},null,2));console.log(JSON.stringify({passed:checks.length,checks}));
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exit(1)});
