import {useEffect,useRef,useState} from "react";
import {ArrowUpRight,ChevronDown,X} from "lucide-react";
import {t} from "../i18n";
import {VideoExportSettings} from "./VideoExportSettings";
import type {VideoExportProgress} from "./video-project";
import "./VideoExportPanel.css";

type Props={preset:"original"|"share"|"small";onPreset(value:Props["preset"]):void;sourceSize:{width:number;height:number};duration:number;valid:boolean;busy:boolean;error:boolean;saved:boolean;progress:VideoExportProgress|null;cancelling:boolean;cancelled:boolean;cancelFailed:boolean;onCancel():void;onSave():void;onOpen():void};
export function VideoExportPanel(props:Props){
  const [open,setOpen]=useState(false),root=useRef<HTMLDivElement>(null),trigger=useRef<HTMLButtonElement>(null),panel=useRef<HTMLDivElement>(null);
  const progress=props.progress?.progress;
  const percent=typeof progress==="number"?Math.floor(progress*100):null;
  const phase=props.progress?.phase??"preparing";
  const busyLabel=t(props.cancelling?"Cancelling export…":phase==="saving"?"Saving to library…":phase==="preparing"?"Preparing video…":"Exporting video…");
  useEffect(()=>{
    if(!open)return;
    panel.current?.focus();
    const outside=(event:globalThis.PointerEvent)=>{const target=event.target as HTMLElement;if(!root.current?.contains(target)&&!target.closest(".kiri-choice-menu"))setOpen(false);};
    const key=(event:KeyboardEvent)=>{if(event.key==="Escape"&&!document.querySelector(".kiri-choice-menu")){event.preventDefault();event.stopImmediatePropagation();setOpen(false);trigger.current?.focus();}};
    document.addEventListener("pointerdown",outside,true);window.addEventListener("keydown",key,true);
    return()=>{document.removeEventListener("pointerdown",outside,true);window.removeEventListener("keydown",key,true);};
  },[open]);
  return <div className="kiri-video-export" ref={root}>
    <button ref={trigger} type="button" className="kiri-button kiri-button--primary" aria-label={t("Export video")} aria-expanded={open} aria-haspopup="dialog" onClick={()=>setOpen(value=>!value)}><ArrowUpRight size={15}/>{props.busy?<>{t(props.cancelling?"Cancelling…":"Exporting…")}{!props.cancelling&&percent!==null?` ${percent}%`:null}</>:t("Export")}<ChevronDown size={12}/></button>
    {open&&<div ref={panel} role="dialog" tabIndex={-1} aria-label={t("Export video")} className="kiri-video-export-panel">
      <div className="kiri-video-export-panel-title"><strong>{t("Export video")}</strong><button type="button" className="kiri-icon-button" aria-label={t("Close")} onClick={()=>{setOpen(false);trigger.current?.focus();}}><X size={14}/></button></div>
      <VideoExportSettings preset={props.preset} onChange={props.onPreset} sourceSize={props.sourceSize} outputDuration={props.duration} disabled={props.busy}/>
      <p>{t("Your original recording stays unchanged.")}</p>
      <div className="kiri-video-export-status" role={props.error||props.cancelFailed?"alert":"status"}>{props.busy?busyLabel:props.error?t("Couldn't export the video. Check library access and free disk space, then retry."):props.saved?t("Copy saved to library"):props.cancelled?t("Export cancelled. Your edit is still here."):null}{props.busy&&percent!==null&&!props.cancelling&&<span>{percent}%</span>}</div>
      {props.busy&&<div className="kiri-video-export-progress" role="progressbar" aria-label={busyLabel} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent??undefined} data-indeterminate={percent===null}><i style={{width:percent===null?"30%":`${percent}%`}}/></div>}
      {props.busy&&props.cancelFailed&&<p role="alert">{t("Couldn't cancel the export. Try again or wait for it to finish.")}</p>}
      <div className="kiri-video-export-panel-actions">{props.busy?<button type="button" className="kiri-button kiri-button--secondary" disabled={props.cancelling||phase==="saving"} onClick={props.onCancel}>{t(props.cancelling?"Cancelling…":phase==="saving"?"Finishing…":"Cancel export")}</button>:<>
        {props.saved&&<button type="button" className="kiri-button kiri-button--secondary" onClick={props.onOpen}>{t("Open")}</button>}<button type="button" className="kiri-button kiri-button--primary" disabled={!props.valid} onClick={props.onSave}>{t("Save a Copy")}</button>
      </>}</div>
    </div>}
  </div>;
}
