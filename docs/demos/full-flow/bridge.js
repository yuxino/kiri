(() => {
  if (window === window.parent || !location.pathname.startsWith('/app/')) return;
  const p=new URLSearchParams(location.search),kind=p.get('window')||'library';
  const label=kind==='viewer'||kind==='editor'?`${kind}-${p.get('id')}`:kind;
  const callbacks=new Map(),listeners=new Map();let serial=0;
  window.__emit=(event,payload)=>{for(const [id,handler] of listeners.get(event)||[])callbacks.get(handler)?.({event,id,payload});};
  window.__TAURI_INTERNALS__={metadata:{currentWindow:{label},currentWebview:{label,windowLabel:label}},
    transformCallback:(fn,once=false)=>{const id=++serial;callbacks.set(id,once?(v)=>{callbacks.delete(id);fn(v)}:fn);return id;},
    unregisterCallback:id=>callbacks.delete(id),runCallback:(id,data)=>callbacks.get(id)?.(data),
    convertFileSrc:(path)=>'/__media__/'+path,
    invoke:async(command,args={})=>{
      if(command==='plugin:event|listen'){const id=++serial;const list=listeners.get(args.event)||[];list.push([id,args.handler]);listeners.set(args.event,list);return id;}
      if(command==='plugin:event|unlisten'){listeners.set(args.event,(listeners.get(args.event)||[]).filter(x=>x[0]!==args.eventId));return null;}
      if(command==='plugin:event|emit'||command==='plugin:event|emit_to'){parent.emit(args.event,args.payload);return null;}
      if(command.startsWith('plugin:window|')){
        const op=command.split('|')[1];
        if(op==='close'||op==='hide'){parent.hideWindow(kind);return null;}
        if(op==='scale_factor')return 1.5;
        if(op==='inner_size')return {width:innerWidth*1.5,height:innerHeight*1.5};
        if(op==='outer_position'||op==='inner_position')return {x:0,y:0};
        if(op.startsWith('is_'))return false;
        if(op==='get_all_windows')return [{label:'library'}];
        return null;
      }
      if(command.startsWith('plugin:webview|'))return null;
      return parent.invoke(kind,command,args);
    }
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
  document.addEventListener('keydown',e=>{
    if(e.ctrlKey&&e.shiftKey&&e.key.toLowerCase()==='a'){e.preventDefault();void parent.launchCapture();}
  });
  document.addEventListener('mousemove',e=>parent.movePointer(window,e.clientX,e.clientY));
})();
