const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const assert=require('node:assert/strict');const fs=require('node:fs');
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'});
 try{
  const page=await browser.newPage({viewport:{width:390,height:844}});const checks=[],errors=[];
  page.on('pageerror',error=>errors.push(error.message));
  await page.addInitScript(()=>{
   globalThis.isTauri=true;let id=0;
   const make=(id,date,time,category='normal')=>({id,name:id+'.mp4',path:'/mnt/mmc/'+id+'.mp4',date,time,category,size:1024,duration:12});
   const remote=Array.from({length:64},(_,i)=>make('normal-'+i,'2026-10-28',`${String(8+Math.floor(i/60)).padStart(2,'0')}:${String(i%60).padStart(2,'0')}:00`));
   remote.push(make('event-new','2026-10-28','10:00:00','event'),make('old-one','2026-10-27','12:00:00'),make('find-me','2026-10-27','13:00:00'),make('event-old','2026-10-27','11:00:00','event'),make('unknown','','00:00:00'));
   const saved=[{...make('saved-new','2026-10-28','11:00:00'),savedPath:'/fixture/new.mp4'},{...make('saved-old','2026-10-26','09:00:00'),savedPath:'/fixture/old.mp4'}];
   const calls=globalThis.groupingCalls=[];
   globalThis.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
   globalThis.__TAURI_INTERNALS__={transformCallback:()=>++id,unregisterCallback:()=>{},invoke:async(command,args={})=>{
    calls.push({command,args});
    if(command==='plugin:event|listen')return ++id;
    if(['plugin:event|unlisten','record_client_event','cancel_recorder_discovery'].includes(command))return;
    if(command==='get_settings')return {saveMode:'gallery',keepScreenOn:true};
    if(command==='list_saved')return saved;if(command==='list_transfers')return [];
    if(command==='discover_recorder')return {address:'192.168.43.8',networkId:'car',protocol:'A7',firmware:'test',vehicle:'星越L'};
    if(command==='check_recorder_connection')return {connected:true};
    if(command==='list_videos')return {videos:remote.filter(v=>v.category===args.category),warning:null};
    if(command==='video_metadata')return {size:1024,duration:12,thumbnail:'/fixture-thumbnail.png',warning:null};
    if(command==='recorder_status')return {fields:{},warnings:[]};
    throw Error('Unexpected grouping mock command '+command);
   }};
  });
  await page.route('**/fixture-thumbnail.png',route=>route.fulfill({path:'tools/fixtures/thumbnail.png',contentType:'image/png'}));
  await page.goto('http://127.0.0.1:1420');await page.locator('.glacier-connection-state.online').waitFor();
  assert.equal(await page.locator('.topbar img,.sidebar .brand img').count(),0);checks.push('top-left interface logo is removed');
  const spacing=await page.evaluate(()=>({header:document.querySelector('.topbar-device').getBoundingClientRect().height,gap:parseFloat(getComputedStyle(document.querySelector('.glacier-home-heading')).marginTop)}));
  assert.ok(spacing.header>=64&&spacing.gap>=16);checks.push('device header and title have added top spacing');
  assert.equal(await page.getByText('下拉刷新，重新连接记录仪',{exact:true}).count(),0);checks.push('device footer reconnect caption is removed');
  await page.locator('.glacier-connect-button').click();await page.locator('.video-card').first().waitFor();
  const groups=page.locator('.recording-date-group');const dates=()=>groups.evaluateAll(nodes=>nodes.map(n=>n.dataset.date));
  assert.equal(await page.locator('.video-card').count(),60);assert.deepEqual(await dates(),['2026-10-28']);assert.equal(await groups.first().locator('.date-heading span').textContent(),'66 段录像');checks.push('first page contains 60 clips and shows the full date-group count');
  await page.getByRole('button',{name:/加载更多录像/}).click();assert.equal(await page.locator('.video-card').count(),71);assert.deepEqual(await dates(),['2026-10-28','2026-10-27','2026-10-26','']);assert.ok((await groups.last().locator('h2').innerText()).includes('日期未知'));assert.equal(await groups.last().locator('.date-heading span').textContent(),'1 段录像');checks.push('pagination preserves newest-first dates and places undated clips in their own group');
  const times=await groups.first().locator('.video-title').allTextContents();assert.equal(times[0],'11:00:00');assert.equal(times[1],'10:00:00');assert.equal(times[2],'09:03:00');checks.push('clips within each date remain newest-first');
  await page.locator('.filter-tabs button').filter({hasText:'紧急录像'}).click();assert.deepEqual(await dates(),['2026-10-28','2026-10-27']);assert.equal(await page.locator('.video-card').count(),2);checks.push('emergency filter keeps separate date groups');
  await page.getByRole('button',{name:'搜索与筛选',exact:true}).click();await page.getByRole('button',{name:'列表视图',exact:true}).click();assert.equal(await page.locator('.video-collection.list').count(),2);checks.push('list view keeps date-group boundaries');
  await page.getByRole('button',{name:'网格视图',exact:true}).click();await page.getByRole('button',{name:'批量选择',exact:true}).first().click();await groups.first().locator('.video-image').click();await groups.last().locator('.video-image').click();assert.equal(await page.locator('.video-card.is-selected').count(),2);await page.getByText('已选 2 段录像',{exact:false}).waitFor();checks.push('batch selection works across dates');
  await page.locator('.library-selection-toolbar').getByRole('button',{name:'清空选择',exact:true}).click();assert.equal(await page.locator('.video-card.is-selected').count(),0);checks.push('global selection can clear clips in every group');
  await page.locator('.filter-tabs button').filter({hasText:'已保存'}).click();assert.deepEqual(await dates(),['2026-10-28','2026-10-26']);assert.equal(await page.locator('.video-card').count(),2);checks.push('saved clips are grouped by recording date');
  await page.locator('.filter-tabs button').filter({hasText:'全部录像'}).click();await page.getByLabel('按日期筛选',{exact:true}).fill('2026-10-27');assert.deepEqual(await dates(),['2026-10-27']);assert.equal(await page.locator('.video-card').count(),3);checks.push('date filter shows only its matching group');
  await page.getByLabel('搜索录像',{exact:true}).fill('find-me');assert.equal(await page.locator('.video-card').count(),1);assert.deepEqual(await dates(),['2026-10-27']);checks.push('search combines with date filter without empty groups');
  await page.getByLabel('搜索录像',{exact:true}).fill('no-such-recording');await page.getByText('没有符合条件的录像',{exact:true}).waitFor();assert.equal(await groups.count(),0);checks.push('empty search removes all date headings');
  await page.getByRole('button',{name:'清除筛选条件',exact:true}).click();await page.locator('.filter-tabs button').filter({hasText:'紧急录像'}).click();
  await page.screenshot({path:'analysis/recordings-grouped-0.8.5-mobile.png',fullPage:true});
  for(const width of [360,414,1280]){await page.setViewportSize({width,height:844});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);}checks.push('date groups fit mobile and desktop widths');
  await page.setViewportSize({width:390,height:844});await page.locator('.nav-item').filter({hasText:'设置与帮助'}).click();assert.equal(await page.locator('.topbar-settings .connection-button').count(),0);await page.getByRole('button',{name:'管理连接',exact:true}).waitFor();checks.push('settings removes the Wi-Fi header button and retains connection management');
  assert.deepEqual(errors,[]);fs.writeFileSync('analysis/recording-groups-ui-verification.json',JSON.stringify({mockedRecorder:true,checks,errors},null,2));console.log(JSON.stringify({passed:checks.length,checks}));
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exit(1);});
