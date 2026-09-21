/** Structural comparison avoids repeatedly serializing embedded sticker PNGs. */
export function sameVideoProjectValue(a,b) {
  if(a===b)return true;
  if(a===null||b===null||typeof a!=="object"||typeof b!=="object")return false;
  if(Array.isArray(a)!==Array.isArray(b))return false;
  const keys=Object.keys(a);
  return keys.length===Object.keys(b).length&&keys.every(key=>Object.hasOwn(b,key)&&sameVideoProjectValue(a[key],b[key]));
}

/** One write at a time; its revision is used by the next queued snapshot. */
export function createVideoProjectSaveQueue({revision,project=null,save,onState=()=>{},delay=600}) {
  let latest=project,saved=project,timer=null,running=null,disposed=false,attempted=project!==null;
  let status=project?"saved":"idle",error=null;
  const changed=()=>!sameVideoProjectValue(latest,saved);
  const report=(next,reason=null)=>{status=next;error=reason;if(!disposed)onState({status,error});};
  const clear=()=>{if(timer!==null){clearTimeout(timer);timer=null;}};
  async function drain() {
    if(disposed)return false;
    if(running)return running;
    clear();
    running=(async()=>{
      while(!disposed&&latest&&changed()) {
        const snapshot=latest;
        attempted=true;
        report("saving");
        try {
          const result=await save(revision,snapshot);
          if(!result||result.state!=="valid"||typeof result.revision!=="string")throw new Error("VIDEO_PROJECT_INVALID_RESPONSE");
          revision=result.revision;
          saved=snapshot;
        } catch(reason) {
          report("error",String(reason));
          return false;
        }
      }
      report(saved?"saved":"idle");
      return !disposed;
    })();
    try{return await running;}finally{
      running=null;
      // A state subscriber can enqueue immediately when a write settles.
      if(!disposed&&changed()&&status!=="error"){
        report("waiting");timer=setTimeout(()=>{timer=null;void drain();},delay);
      }
    }
  }
  return {
    enqueue(next) {
      if(disposed||sameVideoProjectValue(next,latest))return;
      latest=next;clear();
      if(!changed()){if(!running)report(saved?"saved":"idle");return;}
      if(!running){report("waiting");timer=setTimeout(()=>{timer=null;void drain();},delay);}
    },
    async flush() {
      clear();
      const result=await drain();
      // An enqueue at the completion boundary must be included in a close flush.
      return result&&changed()?this.flush():result;
    },
    discardUnwritten() {
      if(attempted||running)return false;
      clear();latest=saved;report("idle");return true;
    },
    getState(){return{status,error,dirty:changed(),revision};},
    dispose(){disposed=true;clear();},
  };
}
