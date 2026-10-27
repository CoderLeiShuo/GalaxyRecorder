mod protocol;

use futures_util::{stream, StreamExt};
use protocol::{authority, media_path, parse_listing_page, Video};
use reqwest::{Client, Url};
use serde::Serialize;
use std::{collections::{HashMap, HashSet}, path::PathBuf, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};
use tauri::{Emitter, Manager, State};
use tokio::{io::AsyncWriteExt, sync::Mutex};

#[derive(Default)]
struct Transfers(Mutex<HashMap<String, Arc<AtomicBool>>>);

#[derive(Clone, Serialize)]
struct Recorder { address: String, protocol: String, firmware: String, vehicle: String }

#[derive(Serialize)]
struct Listing { videos: Vec<Video>, warning: Option<String> }

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress { task_id: String, received: u64, total: Option<u64> }

fn client() -> Result<Client, String> {
    Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(4)).read_timeout(Duration::from_secs(15))
        .user_agent("GalaxyRecorder/0.1").build().map_err(|e|e.to_string())
}

fn endpoint(address: &str, action: &str, property: &str) -> Result<Url,String> {
    let mut url=Url::parse(&format!("http://{}/cgi-bin/Config.cgi",authority(address)?)).map_err(|e|e.to_string())?;
    url.query_pairs_mut().append_pair("action", action).append_pair("property",property);
    Ok(url)
}

async fn text(client: &Client, url: Url, seconds: u64) -> Result<String,String> {
    let mut response=client.get(url).timeout(Duration::from_secs(seconds)).send().await.map_err(|e|format!("设备未响应，请检查热点和地址：{e}"))?.error_for_status().map_err(|e|format!("设备返回错误：{e}"))?;
    let mut bytes=Vec::new();
    while let Some(chunk)=response.chunk().await.map_err(|e|e.to_string())? {
        if bytes.len()+chunk.len()>8*1024*1024 { return Err("设备响应超过读取上限".into()); }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_|"设备返回的文本编码暂不支持".into())
}

async fn probe(address: String, seconds: u64) -> Result<Recorder,String> {
    let address=authority(&address)?;
    let http=client()?;
    for property in ["DVR", "Camera.Menu.FWversion"] {
        if let Ok(body)=text(&http,endpoint(&address,"get",property)?,seconds).await {
            let lower=body.to_lowercase();
            if !lower.contains("<html") && body.contains("OK") && (body.contains("DVR") || body.contains("Camera.")) {
                let firmware=body.lines().find(|line|line.contains("FWversion") && line.contains('=')).and_then(|line|line.split_once('=').map(|(_,value)|value.trim().to_string())).unwrap_or_else(||"待进一步读取".into());
                return Ok(Recorder{address,protocol:"A7 / Config.cgi（待实车确认）".into(),firmware,vehicle:"2026 款星越 L".into()});
            }
        }
    }
    Err("尚未识别到兼容记录仪。请确认已连接车机热点，并打开一次车机记录仪应用。".into())
}

#[tauri::command]
async fn probe_recorder(address: String) -> Result<Recorder,String> { probe(address,5).await }

#[tauri::command]
async fn discover_recorder(subnet: String) -> Result<Recorder,String> {
    let test=authority(&format!("{subnet}.1"))?;
    if test.split('.').count()!=4 || test.contains(':') { return Err("请输入前三段局域网地址，例如 192.168.43".into()); }
    let work=async {
        let mut candidates=stream::iter(1..=254).map(|n|probe(format!("{subnet}.{n}"),1)).buffer_unordered(24);
        while let Some(result)=candidates.next().await { if result.is_ok(){return result;} }
        Err("该网段暂未找到记录仪，可核对地址后重试".into())
    };
    tokio::time::timeout(Duration::from_secs(30),work).await.map_err(|_|"搜索超时，可输入设备地址直接连接".to_string())?
}

#[tauri::command]
async fn list_videos(address: String, category: String) -> Result<Listing,String> {
    let property=match category.as_str(){"normal"=>"MOVIE","event"=>"SOS",_=>return Err("不支持的录像分类".into())};
    let http=client()?;
    let mut videos=Vec::new();let mut seen=HashSet::new();let mut offset=0;let mut repeated=0;
    for _ in 0..200 {
        let mut page=None;let mut failure="录像列表读取失败".to_string();
        for count in [50,20,1] {
            let mut url=endpoint(&address,"dir",property)?;
            url.query_pairs_mut().append_pair("format","all").append_pair("count",&count.to_string()).append_pair("from",&offset.to_string());
            match text(&http,url,10).await.and_then(|xml|parse_listing_page(&xml,&category)) {
                Ok((items,entries))=>{page=Some((items,entries,count));break;},Err(error)=>failure=error,
            }
        }
        let Some((items,entries,count))=page else {
            return if videos.is_empty(){Err(failure)}else{Ok(Listing{videos,warning:Some(format!("列表尚未完整读取：{failure}"))})};
        };
        let before=videos.len();
        for video in items {if seen.insert(video.id.clone()){videos.push(video);}}
        if entries==0 || entries<count {return Ok(Listing{videos,warning:None});}
        repeated=if videos.len()==before{repeated+1}else{0};
        if repeated>=3{return Ok(Listing{videos,warning:Some("设备连续返回重复页，已保留读取到的录像，请刷新重试".into())});}
        offset+=entries;
    }
    Ok(Listing{videos,warning:Some("已达到单次读取上限，列表可能不完整".into())})
}

fn media_directory(app: &tauri::AppHandle) -> Result<PathBuf,String> {
    app.path().app_data_dir().map(|path|path.join("media")).map_err(|e|e.to_string())
}

#[tauri::command]
async fn saved_directory(app: tauri::AppHandle) -> Result<String,String> {
    let path=media_directory(&app)?;tokio::fs::create_dir_all(&path).await.map_err(|e|e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
async fn cancel_download(task_id: String, state: State<'_,Transfers>) -> Result<(),String> {
    if let Some(cancel)=state.0.lock().await.get(&task_id){cancel.store(true,Ordering::Relaxed);}
    Ok(())
}

#[tauri::command]
async fn download_video(app: tauri::AppHandle, state: State<'_,Transfers>, address: String, video: Video, task_id: String) -> Result<String,String> {
    if task_id.is_empty() || task_id.len()>64 || !task_id.chars().all(|c|c.is_ascii_alphanumeric()||c=='-'){return Err("下载任务标识不正确".into());}
    let address=authority(&address)?;let path=media_path(&video.path)?;
    let name=path.rsplit('/').next().ok_or("缺少录像文件名")?;
    let mut url=Url::parse(&format!("http://{address}")).map_err(|e|e.to_string())?;url.set_path(&path);
    let dir=media_directory(&app)?;
    let cancel=Arc::new(AtomicBool::new(false));
    {let mut jobs=state.0.lock().await;if jobs.contains_key(&task_id){return Err("任务已在下载".into());}jobs.insert(task_id.clone(),cancel.clone());}
    let safe_name:String=name.chars().map(|c|if c.is_ascii_alphanumeric()||c=='.'||c=='_'||c=='-'{c}else{'_'}).take(180).collect();
    let final_path=dir.join(format!("{task_id}_{safe_name}"));let partial=final_path.with_extension("part");
    let result=async {
        tokio::fs::create_dir_all(&dir).await.map_err(|e|e.to_string())?;
        let mut response=client()?.get(url).timeout(Duration::from_secs(1800)).header("Accept-Encoding","identity").send().await.map_err(|e|e.to_string())?.error_for_status().map_err(|e|e.to_string())?;
        let total=response.content_length();
        if response.headers().get("content-type").and_then(|h|h.to_str().ok()).is_some_and(|value|value.contains("text/")||value.contains("json")){return Err("设备返回了文本内容，未保存为录像".to_string());}
        let mut file=tokio::fs::OpenOptions::new().write(true).create_new(true).open(&partial).await.map_err(|e|e.to_string())?;
        let mut received=0u64;let mut first=Vec::new();let mut last_event=std::time::Instant::now();
        loop {
            let next=tokio::select! {
                value=response.chunk()=>value.map_err(|e|e.to_string())?,
                _=async {while !cancel.load(Ordering::Relaxed){tokio::time::sleep(Duration::from_millis(100)).await;}}=>return Err("下载已取消".into()),
            };
            let Some(chunk)=next else{break;};
            if first.len()<512{first.extend_from_slice(&chunk[..chunk.len().min(512-first.len())]);}
            received=received.checked_add(chunk.len() as u64).ok_or("录像文件过大")?;
            file.write_all(&chunk).await.map_err(|e|e.to_string())?;
            if last_event.elapsed()>Duration::from_millis(150){let _=app.emit("download-progress",Progress{task_id:task_id.clone(),received,total});last_event=std::time::Instant::now();}
        }
        if cancel.load(Ordering::Relaxed){return Err("下载已取消".into());}
        if received==0||total.is_some_and(|length|length!=received){return Err("录像下载不完整，请重试".into());}
        // Reject error pages even when the device labels them as binary data.
        let first_text=String::from_utf8_lossy(&first).trim_start().to_lowercase();
        if first_text.starts_with("<html")||first_text.starts_with("<!doctype")||first_text.starts_with("<?xml"){return Err("下载响应不是录像，未发布文件".into());}
        file.flush().await.map_err(|e|e.to_string())?;file.sync_all().await.map_err(|e|e.to_string())?;drop(file);
        tokio::fs::rename(&partial,&final_path).await.map_err(|e|e.to_string())?;
        let saved_path=final_path.to_string_lossy().to_string();let mut saved=video;saved.size=received;saved.saved_path=Some(saved_path.clone());
        let metadata=final_path.with_extension("json");let metadata_partial=final_path.with_extension("json.part");
        tokio::fs::write(&metadata_partial,serde_json::to_vec(&saved).map_err(|e|e.to_string())?).await.map_err(|e|e.to_string())?;
        tokio::fs::rename(metadata_partial,metadata).await.map_err(|e|e.to_string())?;
        let _=app.emit("download-progress",Progress{task_id:task_id.clone(),received,total:Some(received)});
        Ok(saved_path)
    }.await;
    state.0.lock().await.remove(&task_id);
    if result.is_err(){let _=tokio::fs::remove_file(&partial).await;let _=tokio::fs::remove_file(&final_path).await;let _=tokio::fs::remove_file(final_path.with_extension("json.part")).await;}
    result
}

#[tauri::command]
async fn list_saved(app: tauri::AppHandle) -> Result<Vec<Video>,String> {
    let directory=media_directory(&app)?;
    if !directory.exists(){return Ok(Vec::new());}
    let mut entries=tokio::fs::read_dir(&directory).await.map_err(|e|e.to_string())?;let mut videos=Vec::new();
    while let Some(entry)=entries.next_entry().await.map_err(|e|e.to_string())? {
        if entry.path().extension().is_some_and(|ext|ext=="json"){
            let bytes=tokio::fs::read(entry.path()).await.map_err(|e|e.to_string())?;
            if let Ok(video)=serde_json::from_slice::<Video>(&bytes){if video.saved_path.as_ref().is_some_and(|path|{let p=PathBuf::from(path);p.parent()==Some(directory.as_path())&&p.is_file()}){videos.push(video);}}
        }
    }
    Ok(videos)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default().plugin(tauri_plugin_opener::init()).manage(Transfers::default())
        .invoke_handler(tauri::generate_handler![probe_recorder,discover_recorder,list_videos,download_video,cancel_download,list_saved,saved_directory])
        .run(tauri::generate_context!()).expect("银河记录仪启动失败");
}
