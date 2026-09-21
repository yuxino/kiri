import {useCallback,useEffect,useRef,useState} from "react";
import {api} from "../lib/ipc";
import {createVideoProjectSaveQueue,type VideoProjectSaveQueue,type VideoProjectSaveState} from "./video-project-save.js";
import {hasVideoEdits,type VideoProject} from "./video-project";

type ProjectState=VideoProjectSaveState|{status:"loading"|"blocked";error:string|null};
type Options={id:string;enabled:boolean;duration:number;getProject():VideoProject;restore(project:VideoProject,isCurrent:()=>boolean):Promise<void>};

/** Loading is read-only. A corrupt/missing baseline can never be autosaved over. */
export function useVideoProject(options:Options) {
  const latest=useRef(options);latest.current=options;
  const queue=useRef<VideoProjectSaveQueue|null>(null);
  const [state,setState]=useState<ProjectState>({status:"loading",error:null});
  const [retryLoad,setRetryLoad]=useState(0);
  const [ready,setReady]=useState(false);
  const hasProject=useRef(false);
  useEffect(()=>{
    let cancelled=false;
    queue.current?.dispose();queue.current=null;hasProject.current=false;setReady(false);
    if(!options.enabled){setState({status:"idle",error:null});return;}
    setState({status:"loading",error:null});
    if(options.duration<=0)return;
    void (async()=>{
      try {
        const loaded=await api.loadVideoProject(options.id);
        if(cancelled)return;
        if(loaded.state==="invalid"){
          setState({status:"blocked",error:loaded.reason==="sourceChanged"?"VIDEO_PROJECT_SOURCE_CHANGED":"VIDEO_PROJECT_INVALID"});return;
        }
        if(loaded.state==="valid"){
          if(!loaded.project)throw new Error("VIDEO_PROJECT_INVALID");
          await latest.current.restore(loaded.project,()=>!cancelled);
          if(cancelled)return;
          hasProject.current=true;
        }
        const controller=createVideoProjectSaveQueue({revision:loaded.revision,project:loaded.project,
          save:(revision,project)=>api.saveVideoProject(options.id,revision,project),
          onState:next=>{if(!cancelled){if(next.status==="saved")hasProject.current=true;setState(next);}},
        });
        queue.current=controller;setReady(true);
        setState({status:loaded.project?"saved":"idle",error:null});
      } catch(error) {
        if(!cancelled)setState({status:"blocked",error:String(error)});
      }
    })();
    return()=>{cancelled=true;queue.current?.dispose();queue.current=null;};
  },[options.id,options.enabled,options.duration,retryLoad]);

  const schedule=useCallback(()=>{
    const controller=queue.current;if(!controller)return;
    const project=latest.current.getProject();
    // Opening or playing an untouched recording should not create an empty project.
    if(!hasProject.current&&!hasVideoEdits(project.edit,project.sourceDuration)&&controller.discardUnwritten())return;
    controller.enqueue(project);
  },[]);
  const flush=useCallback(async()=>{
    schedule();
    return queue.current?queue.current.flush():true;
  },[schedule]);
  const retry=useCallback(()=>{
    if(queue.current)void flush();else setRetryLoad(value=>value+1);
  },[flush]);
  return{state,ready,schedule,flush,retry};
}
