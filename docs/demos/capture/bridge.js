// Documentation-only native-boundary fixtures. Never imported by any shipped app.
(() => {
  const repo = '__PROJECT__';
  window.__demoCalls = [];
  if (repo === 'mimi') {
    localStorage.setItem('mimi-ui-language', 'en');
    return; // Use the actual frontend's existing credential-free browser preview.
  }
  const callbacks = new Map(), listeners = new Map(); let serial = 0;
  const appearance = {colorPreset:'violet',textBackgroundStyle:'transparent',mosaicIntensity:'standard',mosaicStyle:'pixel',penWidth:3,shapeWidth:3,textFontSize:24,mosaicBrushDiameter:20};
  let store = {schema_version:1,books:[],qa:[],settings:{active_profile_id:'model-studio-default',profiles:[{id:'model-studio-default',name:'阿里云百炼',provider:'model_studio',base_url:'https://dashscope.aliyuncs.com/compatible-mode/v1',model_id:'qwen3-vl-plus',api_key_required:true}]},activity:{}};
  const doc = '# A little room for good ideas\n\nA quiet space for notes, plans, and the things worth keeping.\n\n## This weekend\n\n- Read a few pages\n- Make something small\n- Take a long walk\n\n## One useful thought\n\n> Start small. Leave room to play.\n\n```typescript\nconst nextStep = "make something useful";\nconsole.log(nextStep);\n```\n';
  const root = '/demo/Weekend notes';
  const documents = {'Weekend.md':doc,'Ideas.md':'# Little ideas\n\nA useful tool starts with a small, ordinary need.\n\n## Next up\n\n- Keep the interface quiet\n- Keep the files local\n- Make the first step obvious\n'};
  const snapshot = p => ({relativePath:p,name:p.split('/').pop(),content:documents[p] || doc,lineEnding:'lf',revision:{modifiedAtMs:1788652800000,sizeBytes:(documents[p] || doc).length,contentSha256:'1'.repeat(64)}});
  const books = [{aid:'1001',title:'Small moments — original sample',url:'https://wnacg.com/photos-index-aid-1001.html',cover:'https://img.qy0.ru/demo/page-1.png',meta:'3 pages · local demo fixture'}];
  const photos = [1,2,3].map(i => ({id:String(i),url:`https://wnacg.com/photos-view-id-${i}.html`,title:`Original sample — ${i}`}));
  window.__demoEmit = (event,payload) => {for (const {handler} of (listeners.get(event) || [])) callbacks.get(handler)?.({event,id:handler,payload});};
  async function invoke(command,args = {}) {
    window.__demoCalls.push({command,args});
    if (command === 'plugin:event|listen') {const id=++serial; const rows=listeners.get(args.event)||[]; rows.push({id,handler:args.handler}); listeners.set(args.event,rows); return id;}
    if (command === 'plugin:event|unlisten') return null;
    if (command === 'plugin:event|emit' || command === 'plugin:event|emit_to') {window.__demoEmit(args.event,args.payload); return null;}
    if (command === 'plugin:window|get_all_windows') return [{label:'main'}];
    if (command === 'plugin:webview|get_all_webviews') return [{windowLabel:'main',label:'main'}];
    if (command === 'plugin:window|scale_factor') return 1;
    if (command === 'plugin:window|inner_size') return {width:innerWidth,height:innerHeight};
    if (command === 'plugin:window|outer_position' || command === 'plugin:window|inner_position') return {x:0,y:0};
    if (command.startsWith('plugin:window|is_')) return false;
    if (command.startsWith('plugin:window|') || command.startsWith('plugin:webview|') || command.startsWith('plugin:menu|')) return null;
    if (command === 'plugin:app|version') return {kiri:'1.4.9',mimi:'1.3.8',satori:'3.4.4',viva:'2.0.6',tick:'0.1.4',wnacg:'0.1.11'}[repo];
    if (command === 'plugin:app|name') return repo;
    if (command === 'plugin:dialog|open') return repo === 'satori' ? '/demo/sample.pdf' : root;
    if (command.startsWith('plugin:updater|')) throw new Error('Updates are not part of the documentation demo.');
    if (command === 'plugin:opener|open_url') throw new Error('External navigation is disabled in this demo.');
    if (command === 'get_language' || command === 'get_locale') return 'en';
    if (repo === 'kiri') {
      if (command === 'get_annotation_appearance') return appearance;
      if (command === 'set_annotation_appearance') return null;
      if (command === 'get_asset_annotation_project') return {state:'missing',documentJson:null,revisionSha256:'a'.repeat(64)};
      if (command === 'list_pending_recordings' || command === 'list_assets') return [];
      if (command === 'get_library_status') return {availability:'ready',locationLabel:'Demo library',isDefault:true};
      if (command === 'log_frontend_error') {console.warn(args.message); return null;}
    }
    if (repo === 'viva') {
      if (command === 'get_quit_guard_session') return 1;
      if (command === 'set_quit_guard_ready' || command === 'set_has_unsaved_changes') return true;
      if (command === 'is_fresh_window') return false;
      if (command === 'open_workspace') return {rootPath:root,name:'Weekend notes',children:Object.keys(documents).map(n => ({name:n,relativePath:n,kind:'file',children:[]}))};
      if (command === 'read_document') return snapshot(args.request.relativePath);
      if (command === 'write_document') {documents[args.request.relativePath]=args.request.content; return snapshot(args.request.relativePath);}
      if (command === 'list_document_history' || command === 'search_workspace') return [];
      if (command === 'set_menu_language') return null;
    }
    if (repo === 'satori') {
      if (command === 'load_store') return store;
      if (command === 'save_store') {store=args.store; return null;}
      if (command === 'resolve_book_path') return args.book;
      if (command === 'inspect_pdf_file') return {size_bytes:4200,large:false};
      if (command === 'credential_status') return {saved:false};
      if (command === 'load_thumb' || command === 'save_thumb') return null;
      if (command.includes('ask') || command.includes('test_ai') || command === 'extract_outline') throw new Error('AI is not included in this local interface demo.');
    }
    if (repo === 'tick') {
      if (command === 'get_scheduler_capabilities') return {platform:'windows',computerLabel:'电脑',schedulerName:'Windows 任务计划程序',definitionLabel:'任务 XML',defaultInterpreter:'node',scriptPathExample:'C:\\Scripts\\daily.js',workingDirectoryExample:'C:\\Work',homeDirectory:'C:\\Demo',trashLabel:'回收站',minimumIntervalSeconds:60,maximumIntervalSeconds:2678400};
      if (command === 'get_node_runtime_status') return {available:true,version:'Documentation preview',executablePath:'node'};
      if (command === 'list_scheduled_jobs') return [];
      if (command.includes('run_') || command === 'save_scheduled_job') throw new Error('Scheduling and execution are not part of this interface demo.');
    }
    if (repo === 'wnacg') {
      if (command === 'fetch_albums' || command === 'search_albums') return books;
      if (command === 'fetch_album_photos') return {photos,title:books[0].title,tags:[{name:'Original sample',path:'/demo'}],categories:[],author:'Local demo'};
      if (command === 'fetch_photo_image') {const i=String(args.pageUrl||'').match(/id-(\d+)/)?.[1]||'1'; return {url:`https://img.qy0.ru/demo/page-${i}.png`};}
      if (command === 'fetch_image_data_url' || command === 'fetch_image_data_url_progress') {
        const i=String(args.url||'').match(/page-(\d+)/)?.[1]||'1';
        const blob=await (await fetch(`/__demo__/page-${i}.png`)).blob();
        return await new Promise(resolve => {const fr=new FileReader(); fr.onload=()=>resolve({dataUrl:fr.result}); fr.readAsDataURL(blob);});
      }
      if (command === 'ocr_capabilities') return {vision:false,manga:false};
      if (command === 'is_window_fullscreen') return false;
      if (command === 'set_window_title') return null;
      if (command === 'ocr_engine_status' || command === 'translate_engine_status') return 'not_configured';
      if (command.includes('ocr') || command.includes('translate')) throw new Error('OCR and translation are not included in this demo.');
    }
    throw new Error(`Unimplemented documentation boundary: ${command}`);
  }
  window.__TAURI_INTERNALS__ = {
    metadata:{currentWindow:{label:'main'},currentWebview:{label:'main',windowLabel:'main'}},invoke,
    convertFileSrc:(p,protocol) => protocol === 'kiri' ? '/__demo__/source.png' : '/__demo__/sample.pdf',
    transformCallback:(cb,once=false) => {const id=++serial;callbacks.set(id,once?v=>{callbacks.delete(id);cb(v);}:cb);return id;},
    unregisterCallback:id=>callbacks.delete(id),runCallback:(id,data)=>callbacks.get(id)?.(data)
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
})();
