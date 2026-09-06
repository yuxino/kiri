'use strict';
// Documentation-only window/IPC harness. The production bundles are never edited.
const S={assets:[],record:{isStarting:false,isRecording:false,isPaused:false,isTransitioning:false,isFinalizing:false,elapsed:0,elapsedLabel:'00:00'},session:'',region:null,options:{outputFormat:'mp4',usesCountdown:true,capturesSystemAudio:false,capturesMicrophone:false,showsCursor:true,highlightsClicks:false},appearance:{colorPreset:'cherry',textBackgroundStyle:'transparent',mosaicIntensity:'standard',mosaicStyle:'pixel',penWidth:3,shapeWidth:3,textFontSize:24,mosaicBrushDiameter:20},calls:[],pendingAnnotation:null};
const TOKEN='c61f85ca24b84be18a8ee98c04c1b875';
window.state=S;
window.movePointer=(sender,x,y)=>{
  let offset={left:0,top:0};
  for(const f of document.querySelectorAll('iframe'))if(f.contentWindow===sender)offset=f.getBoundingClientRect();
  document.querySelector('#pointer').style.transform=`translate(${x+offset.left}px,${y+offset.top}px)`;
  document.querySelector('#pointer').style.left='0px';document.querySelector('#pointer').style.top='0px';
};
window.emit=(event,payload)=>{for(const f of document.querySelectorAll('iframe')){try{f.contentWindow.__emit?.(event,structuredClone(payload))}catch{}}};
window.hideWindow=(kind)=>{document.getElementById(kind)?.remove();};
function frame(kind,params={}){
  hideWindow(kind);const f=document.createElement('iframe');f.id=kind;f.title='Kiri '+kind;
  f.src='/app/?'+new URLSearchParams({window:kind,...params});document.body.append(f);return f;
}
window.openLibrary=()=>{hideWindow('viewer');hideWindow('editor');hideWindow('toast');return frame('library');};
window.showSubject=()=>{hideWindow('library');hideWindow('viewer');hideWindow('editor');hideWindow('toast');};
window.launchCapture=async()=>{
  if(S.record.isRecording||S.record.isStarting)return;
  showSubject();await window.__backend('freeze',{});frame('overlay',{captureToken:TOKEN});
};
function status(patch){Object.assign(S.record,patch);emit('recording-state',S.record);}
function newAsset(kind,w,h){return {id:crypto.randomUUID(),kind,createdAt:Date.now(),filename:kind==='image'?'工作笔记.png':'操作演示.mp4',title:kind==='image'?'工作笔记':'操作演示',tags:[],pixelWidth:w,pixelHeight:h,duration:kind==='video'?S.record.elapsed:null,sourceApplication:'工作笔记',isFavorite:false,trashedAt:null,gifEligible:kind==='video'};}
function toast(asset){frame('toast',{mode:'completion',phase:'ready',assetId:asset.id,id:crypto.randomUUID(),kind:asset.kind,title:asset.kind==='image'?'截图已保存':'录屏已保存',detail:asset.kind==='image'?'已复制到剪贴板':'已保存到素材库',copied:asset.kind==='image'?'1':'0',gifEligible:'0'});}
setInterval(()=>{if(S.record.isRecording&&!S.record.isPaused){const e=S.record.elapsed+1;status({elapsed:e,elapsedLabel:`00:${String(e).padStart(2,'0')}`});}},1000);
window.invoke=async(kind,c,a={})=>{
  S.calls.push({c,a:c==='confirm_capture'?'PNG bytes':a,at:performance.now()});
  if(c==='get_language'||c==='get_locale')return 'zh-Hans';
  if(c==='log_frontend_error'){console.error('App error:',a.message);return null;}
  if(c==='plugin:app|version')return '1.4.9';
  if(c==='plugin:app|name')return 'Kiri';
  if(c==='get_annotation_appearance')return S.appearance;
  if(c==='set_annotation_appearance'){S.appearance=a.appearance;return null;}
  if(c==='get_library_status')return {availability:'ready',locationLabel:'本地素材库',isDefault:true};
  if(c==='get_shortcut_status')return {label:'Shift+Ctrl+A',status:'enabled'};
  if(c==='list_pending_recordings')return [];
  if(c==='get_recording_options')return S.options;
  if(c==='set_recording_options'){S.options=a.options;return null;}
  if(c==='mic_supported')return true;
  if(c==='list_assets')return S.assets.filter(x=>Boolean(x.trashedAt)===a.showingTrash&&(!a.query||JSON.stringify(x).includes(a.query)));
  if(c==='get_asset')return S.assets.find(x=>x.id===a.id);
  if(c==='get_asset_availability')return {status:'ready'};
  if(c==='set_favorite'){S.assets.find(x=>x.id===a.id).isFavorite=a.favorite;emit('library-changed',null);return null;}
  if(c==='rename_asset'){S.assets.find(x=>x.id===a.id).title=a.title;emit('library-changed',null);return null;}
  if(c==='set_tags'){S.assets.find(x=>x.id===a.id).tags=a.tags;emit('library-changed',null);return null;}
  if(c==='start_capture')return {displayWidth:1280,displayHeight:720,scale:1.5,pixelWidth:1920,pixelHeight:1080,windowRects:[{x:170,y:76,width:940,height:570}],sourceApplication:'工作笔记'};
  if(c==='cancel_capture'){setTimeout(()=>hideWindow('overlay'),0);return null;}
  if(c==='prepare_capture_annotation'){S.pendingAnnotation=a.request;return 'sample-annotation-once';}
  if(c==='confirm_capture'){
    const selection=S.pendingAnnotation.selection,asset=newAsset('image',Math.round(selection.width*1.5),Math.round(selection.height*1.5));
    const bytes=Array.from(a instanceof ArrayBuffer?new Uint8Array(a):a);
    await window.__backend('save_png',{id:asset.id,bytes});S.assets.unshift(asset);hideWindow('overlay');toast(asset);emit('library-changed',null);return null;
  }
  if(c==='get_ocr_provider_settings')return {schemaVersion:1,activeEngine:{kind:'local'},profiles:[]};
  if(c==='prepare_ocr_request')return {requestId:'original-page-text',engine:{kind:'local'},imageWidth:Math.round(a.selection.width*1.5),imageHeight:Math.round(a.selection.height*1.5),byteLength:2048};
  if(c==='recognize_prepared_ocr_local'){await new Promise(r=>setTimeout(r,650));return '把有用的细节，留在眼前。\n圈出重点，复制文字，再用一段录屏说明过程。';}
  if(c==='cancel_prepared_ocr')return null;
  if(c==='copy_text'){S.copiedText=a.text;hideWindow('overlay');return null;}
  if(c==='start_recording_flow'){
    S.region=a.request.region;S.options=a.request.options;S.session=crypto.randomUUID();status({isStarting:true,elapsed:0,elapsedLabel:'00:00',isRecording:false,isPaused:false});
    hideWindow('overlay');frame('countdown',{session:S.session});return null;
  }
  if(c==='recording_countdown_ready'){
    if(a.sessionId!==S.session)throw Error('Stale countdown');
    await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));return null;
  }
  if(c==='cancel_recording_flow'){
    if(a.sessionId&&a.sessionId!==S.session)return null;S.session='';status({isStarting:false});hideWindow('countdown');hideWindow('control-panel');return null;
  }
  if(c==='get_recording_state')return S.record;
  if(c==='begin_recording'){
    if(a.sessionId!==S.session)throw Error('Stale recording start');
    hideWindow('countdown');status({isStarting:false,isRecording:true,isPaused:false});frame('control-panel');
    await window.__backend('record_start',{region:S.region});return null;
  }
  if(c==='pause_recording'){status({isPaused:true});await window.__backend('record_pause',{});return null;}
  if(c==='resume_recording'){await window.__backend('record_resume',{});status({isPaused:false});return null;}
  if(c==='stop_recording'){
    status({isFinalizing:true});const asset=newAsset('video',Math.round(S.region.width*1.5),Math.round(S.region.height*1.5));
    await window.__backend('record_stop',{id:asset.id});S.assets.unshift(asset);hideWindow('control-panel');status({isRecording:false,isPaused:false,isFinalizing:false});toast(asset);emit('library-changed',null);return null;
  }
  if(c==='open_asset'){hideWindow('toast');frame('viewer',{id:a.id});return null;}
  if(c==='open_editor'){hideWindow('toast');frame('editor',{id:a.id});return null;}
  if(c==='get_asset_annotation_project')return {state:'none',documentJson:null,revisionSha256:'a'.repeat(64)};
  if(c==='copy_asset'){emit('notice',{id:crypto.randomUUID(),title:'已复制',symbol:'checkmark'});return null;}
  throw Error('Unsupported documentation command: '+c);
};
for(const f of document.querySelectorAll('iframe'))f.addEventListener('load',()=>{
  f.contentDocument.addEventListener('mousemove',e=>movePointer(f.contentWindow,e.clientX,e.clientY));
  f.contentDocument.addEventListener('keydown',e=>{if(e.ctrlKey&&e.shiftKey&&e.key.toLowerCase()==='a'){e.preventDefault();void launchCapture();}});
});
openLibrary();
