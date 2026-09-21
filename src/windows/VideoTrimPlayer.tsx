import { useCallback, useEffect, useLayoutEffect, useRef, useState, type PointerEvent } from "react";
import { Scissors, Trash2, Undo2, Redo2, Play, Pause, RotateCcw, X, ImagePlus, SlidersHorizontal, ChevronDown } from "lucide-react";
import { api } from "../lib/ipc";
import { currentMonitor, getCurrentWindow, PhysicalPosition, PhysicalSize } from "@tauri-apps/api/window";
import { fmt, t } from "../i18n";
import { segmentSpeed, timelineSegments, sourceAtOutput, moveSegment, outputTime, splitSegment, timelineDuration, trimSegment, validSegments, videoTimeLabel, type VideoSegment } from "./video-trim.js";
import { useVideoThumbnails } from "./useVideoThumbnails";
import { VideoEffectsControls, VideoEffectsOverlay } from "./VideoEffects";
import {defaultOverlayRange, effectLabels, moveVideoEffect, videoPreviewTransform, type VideoEffect} from "./video-effects";
import {paintVideoEffect,paintVideoEffects} from "./video-effect-render";
import {isVideoAdjustment,nextVideoLayer,orderedVideoLayers,videoLayerPreviewTime} from "./video-layers";
import "./video-trim.css";
import {VideoOutputEffectTracks} from "./VideoOutputEffectTracks";
import {ChoiceSelect} from "../components/ChoiceSelect";
import {VideoPlaybackControls} from "./VideoPlaybackControls";
import {VideoTimeInput} from "./VideoTimeInput";
import {VideoExportPanel} from "./VideoExportPanel";
import {VideoCloseGuard} from "./VideoCloseGuard";
import {VideoVisibleTime} from "./VideoVisibleTime";
import {useVideoProject} from "./useVideoProject";
import {sameVideoProjectValue} from "./video-project-save.js";
import {hasVideoEdits,type VideoEdit,type VideoProject,type VideoExportProgress} from "./video-project";
import {installVideoProjectShortcuts} from "./video-project-shortcuts.js";
import "./VideoProjectStatus.css";

import {importVideoSticker,rasterizeVideoStickers,type VideoSticker} from "./video-stickers";
import {markIndexAt,selectionBounds,translateMark,type AnnotationMark,type Tool} from "../annotation/model";
import {hitTestHandle} from "../annotation/geom";
import {VideoAnnotationsEditor} from "./VideoAnnotationsEditor";
import {paintVideoAnnotation,rasterizeVideoAnnotations} from "./video-annotation-render";
type EditDocument = VideoEdit;
const annotationLabel=(mark:AnnotationMark)=>(mark.kind==="text"&&mark.text.trim()?`${t("Text")} · ${mark.text.trim().replace(/\s+/g," ").slice(0,16)}`:t(({pen:"Pen",rectangle:"Rectangle",line:"Line",arrow:"Arrow",text:"Text",mosaic:"Mosaic"} as const)[mark.kind]));
const unchanged = sameVideoProjectValue;

async function makeRoomForVideoEditor() {
  const window = getCurrentWindow();
  if (await window.isMaximized() || await window.isFullscreen()) return;
  const [monitor, inner, outer, position] = await Promise.all([
    currentMonitor(), window.innerSize(), window.outerSize(), window.outerPosition(),
  ]);
  if (!monitor) return;
  const area = monitor.workArea, scale = monitor.scaleFactor;
  const border = {width: outer.width - inner.width, height: outer.height - inner.height};
  const width = Math.max(inner.width, Math.min(1060 * scale, area.size.width - border.width - 32 * scale));
  const height = Math.max(inner.height, Math.min(720 * scale, area.size.height - border.height - 32 * scale));
  if (width === inner.width && height === inner.height) return;
  const x = Math.max(area.position.x, Math.min(position.x - (width - inner.width) / 2, area.position.x + area.size.width - width - border.width));
  const y = Math.max(area.position.y, Math.min(position.y - (height - inner.height) / 2, area.position.y + area.size.height - height - border.height));
  await window.setSize(new PhysicalSize(Math.round(width), Math.round(height)));
  await window.setPosition(new PhysicalPosition(Math.round(x), Math.round(y)));
}

export function VideoTrimPlayer(props: { id: string; src: string; editable: boolean; onClose(): void; onError(): void }) {
  const video = useRef<HTMLVideoElement>(null);
  const container=useRef<HTMLDivElement>(null);
  const playbackIndex=useRef(0);
  const [timelineHeight,setTimelineHeight]=useState(230);
  const [draggedClip,setDraggedClip]=useState<number|null>(null);
  const [dropIndex,setDropIndex]=useState<number|null>(null);
  const [dragOffset,setDragOffset]=useState(0);
  const suppressClick=useRef(false);
  const isWindows=/Windows/i.test(navigator.userAgent);
  const canvas = useRef<HTMLCanvasElement>(null);
  const liveAnnotation=useRef<{marks:AnnotationMark[];draft:AnnotationMark|null;editingId:number|null}|null>(null),redrawComposite=useRef<(()=>void)|null>(null);
  const receiveLiveMarks=useCallback((marks:AnnotationMark[],draft:AnnotationMark|null,editingId:number|null)=>{liveAnnotation.current={marks,draft,editingId};redrawComposite.current?.();},[]);
  const stage = useRef<HTMLDivElement>(null);
  const track = useRef<HTMLDivElement>(null);
  const saving = useRef(false);
  const stickerInput=useRef<HTMLInputElement>(null);
  const stickerImages=useRef(new Map<string,HTMLImageElement>());
  const alive=useRef(true);useEffect(()=>{alive.current=true;return()=>{alive.current=false;};},[]);
  const [importing,setImporting]=useState(false);
  const [stickerError,setStickerError]=useState(false);
  const commitAnnotation = useRef<(() => void) | null>(null);
  const textDraft=useRef<{mark:AnnotationMark|null;previousId:number|null;start:number;end:number;layer:number}|null>(null);
  const scheduleProject=useRef<(()=>void)|null>(null);
  const registerAnnotationCommit = useCallback((commit:(()=>void)|null)=>{commitAnnotation.current=commit;},[]);
  const previewing = useRef(false);
  const dragBase = useRef<EditDocument | null>(null);
  const history = useRef<{ past: EditDocument[]; future: EditDocument[] }>({past:[],future:[]});
  const [doc, setDoc] = useState<EditDocument>({segments:[],effects:[],annotations:[],stickers:[]});
  const docRef = useRef(doc); docRef.current=doc;
  const [editing, setEditing] = useState(false);
  useEffect(() => {
    if (editing) void makeRoomForVideoEditor().catch(() => {});
  }, [editing]);
  const [duration, setDuration] = useState(0);
  const [sourceSize, setSourceSize] = useState({width:16,height:9});
  const [fitted, setFitted] = useState({width:1,height:1});
  const [time, setTime] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [selected, setSelected] = useState(0);
  const [annotating,setAnnotating]=useState(false);
  const [annotationTool,setAnnotationTool]=useState<Tool>("select");
  const cancelCanvasDrag=useRef<(()=>boolean)|null>(null);
  const [annotationId,setAnnotationId]=useState<string|null>(null);
  const [annotationRevision,setAnnotationRevision]=useState(0);
  const [appearanceHost,setAppearanceHost]=useState<HTMLDivElement|null>(null);
  const [annotationToolbar,setAnnotationToolbar]=useState<HTMLDivElement|null>(null);
  const [sourceImage,setSourceImage]=useState<HTMLImageElement|null>(null);
  const [effectId, setEffectId] = useState<string | null>(null);
  const [preset, setPreset] = useState<"original" | "share" | "small">("original");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  const [savedId, setSavedId] = useState<string | null>(null);
  const [exportProgress,setExportProgress]=useState<VideoExportProgress|null>(null);
  const [cancelling,setCancelling]=useState(false);
  const [exportCancelled,setExportCancelled]=useState(false);
  const [cancelFailed,setCancelFailed]=useState(false);
  const exportRequest=useRef<string|null>(null);
  const exportStarted=useRef(false);
  const cancelRequested=useRef(false);
  const {frames,failed:thumbnailFailed} = useVideoThumbnails(props.src,duration,editing);
  const {segments,effects,annotations,stickers} = doc;
  const selectedSticker=stickers.find(item=>item.id===effectId);
  const selectedAnnotation=annotations.find(item=>item.id===annotationId);
  const visibleAnnotations=orderedVideoLayers(annotations.filter(item=>(time>=item.start&&time<item.end)||item.id===annotationId));
  const timelineItems=[...effects,...stickers.map(item=>({...item,kind:"mask" as const})),...annotations.map(item=>({id:item.id,kind:"mask" as const,start:item.start,end:item.end,x:0,y:0,width:1,height:1,layer:item.layer}))];
  const trackLabels=Object.fromEntries([...annotations.map(item=>[item.id,annotationLabel(item.mark)]),...stickers.map((item,index)=>[item.id,`${t("Sticker")} ${index+1}`])]);
  const selectedTrack=timelineItems.find(item=>item.id===(annotating?annotationId:effectId));
  const selectedClip=segments[selected];
  const total=timelineDuration(segments);
  const timeline=timelineSegments(segments);
  const clockTime=outputTime(segments,time,playbackIndex.current);
  const previewTransform=videoPreviewTransform(effects,time);
  const {x:tx,y:ty,sx,sy,clip:contentRect}=previewTransform;
  const annotationClip=`inset(${(contentRect.y-ty)/sy*100}% ${(1-(contentRect.x+contentRect.width-tx)/sx)*100}% ${(1-(contentRect.y+contentRect.height-ty)/sy)*100}% ${(contentRect.x-tx)/sx*100}%)`;
  const valid=validSegments(segments,duration);
  const canSplit=splitSegment(segments,time)!==segments;
  const projectContext=useRef({duration,sourceSize,preset,time});projectContext.current={duration,sourceSize,preset,time};
  const receiveTextDraft=useCallback((mark:AnnotationMark|null,previousId:number|null,active:boolean)=>{
    const previous=textDraft.current;
    if(active){
      const context=projectContext.current,current=docRef.current;
      const existing=previousId===null?null:current.annotations.find(item=>item.mark.id===previousId);
      const range=existing??previous??defaultOverlayRange(context.time,context.duration);
      textDraft.current={mark,previousId,start:range.start,end:range.end,layer:existing?.layer??previous?.layer??nextVideoLayer([...current.effects,...current.annotations,...current.stickers])};
    }else textDraft.current=null;
    scheduleProject.current?.();
  },[]);
  function projectSnapshot():VideoProject {
    const current=docRef.current,context=projectContext.current,draft=textDraft.current;
    let edit=current;
    if(draft){
      const annotations=current.annotations.flatMap(item=>item.mark.id!==draft.previousId?[item]:draft.mark?[{...item,mark:draft.mark}]:[]);
      if(draft.previousId===null&&draft.mark&&!annotations.some(item=>item.mark.id===draft.mark!.id))annotations.push({id:`annotation-${draft.mark.id}`,mark:draft.mark,start:draft.start,end:draft.end,layer:draft.layer});
      edit={...current,annotations};
    }
    return{schemaVersion:1,sourceSize:context.sourceSize,sourceDuration:context.duration,edit,preset:context.preset,playhead:Math.max(0,Math.min(context.duration,video.current?.currentTime??context.time))};
  }
  async function restoreProject(project:VideoProject,isCurrent:()=>boolean){
    const context=projectContext.current;
    if(project.sourceSize.width!==context.sourceSize.width||project.sourceSize.height!==context.sourceSize.height||Math.abs(project.sourceDuration-context.duration)>.1)throw new Error("VIDEO_PROJECT_SOURCE_CHANGED");
    const images=await Promise.all(project.edit.stickers.map(async sticker=>{const image=new Image();image.src=sticker.dataUrl;await image.decode();return[sticker.id,image] as const;}));
    if(!alive.current||!isCurrent())return;
    stickerImages.current=new Map(images);
    textDraft.current=null;liveAnnotation.current=null;
    docRef.current=project.edit;setDoc(project.edit);setPreset(project.preset);projectContext.current.preset=project.preset;
    history.current={past:[],future:[]};setAnnotationRevision(value=>value+1);
    let index=project.edit.segments.findIndex(clip=>project.playhead>=clip.start&&project.playhead<clip.end);
    if(index<0)index=project.edit.segments.findIndex(clip=>Math.abs(project.playhead-clip.end)<.001);
    playbackIndex.current=Math.max(0,index);setSelected(Math.max(0,index));setTime(project.playhead);
    previewing.current=false;video.current?.pause();if(video.current)video.current.currentTime=project.playhead;
    if(hasVideoEdits(project.edit,project.sourceDuration)||project.preset!=="original")setEditing(true);
  }
  const project=useVideoProject({id:props.id,enabled:props.editable,duration,getProject:projectSnapshot,restore:restoreProject});
  scheduleProject.current=project.schedule;
  useEffect(()=>{if(project.ready)project.schedule();},[doc,preset,project.ready,project.schedule]);
  async function flushProject(){
    commitAnnotation.current?.();textDraft.current=null;
    return project.flush();
  }

  useEffect(()=>{
    const id=annotating?annotationId:effectId;if(!id)return;
    const viewport=container.current?.querySelector<HTMLElement>(".kiri-video-lanes-scroll"),row=viewport?.querySelector<HTMLElement>(`[data-effect-id="${CSS.escape(id)}"]`);
    if(!viewport||!row)return;
    const bounds=viewport.getBoundingClientRect(),rect=row.getBoundingClientRect();
    if(rect.bottom>bounds.bottom)viewport.scrollTop+=rect.bottom-bounds.bottom+4;
    else if(rect.top<bounds.top+28)viewport.scrollTop-=bounds.top+28-rect.top;
  },[effectId,annotationId,annotating,timelineItems.length]);

  const seek = useCallback((value:number) => {
    const player=video.current;
    if(!player || !Number.isFinite(value)) return;
    commitAnnotation.current?.();
    previewing.current=false; player.pause();
    const next=Math.max(0,Math.min(duration,value));
    const index=docRef.current.segments.findIndex(segment=>next>=segment.start&&next<segment.end);
    if(index>=0)playbackIndex.current=index;
    player.currentTime=next; setTime(next);scheduleProject.current?.();
  },[duration]);

  function seekOutput(value:number){const target=sourceAtOutput(docRef.current.segments,value);if(!target)return;seek(target.time);playbackIndex.current=target.index;setSelected(target.index);}

  function apply(next: EditDocument, transient=false) {
    const previous=docRef.current;
    if(transient) { dragBase.current ??= previous; }
    else {
      const baseline=dragBase.current ?? previous;
      dragBase.current=null;
      if(!unchanged(baseline,next)) {
        history.current.past=[...history.current.past.slice(-99),baseline];
        history.current.future=[];
      }
    }
    docRef.current=next; setDoc(next);
    setSavedId(null); setError(false); previewing.current=false; video.current?.pause();
  }
  function undo(redo=false) {
    if(busy || dragBase.current) return;
    commitAnnotation.current?.();
    const from=redo ? history.current.future : history.current.past;
    const next=from.pop(); if(!next) return;
    (redo ? history.current.past : history.current.future).push(docRef.current);
    docRef.current=next; setDoc(next); setEffectId(null);setAnnotationRevision(value=>value+1);
    setSelected(index=>Math.min(index,Math.max(0,next.segments.length-1)));
    setSavedId(null); setError(false); previewing.current=false; video.current?.pause();
  }
  function split() {
    if(busy) return;
    commitAnnotation.current?.();
    const next=splitSegment(docRef.current.segments,time);
    if(next===docRef.current.segments) return;
    apply({...docRef.current,segments:next});
    setSelected(Math.max(0,next.findIndex(s=>Math.abs(s.start-time)<0.001))); setEffectId(null);setAnnotationId(null);setAnnotating(false);
  }
  function removeClip() {
    if(busy || !selectedClip) return;
    commitAnnotation.current?.();
    const next=docRef.current.segments.filter((_,index)=>index!==selected);
    apply({...docRef.current,segments:next});
    setSelected(Math.max(0,Math.min(selected,next.length-1)));
    setEffectId(null);setAnnotationId(null);setAnnotating(false);
    const target=next[Math.min(selected,next.length-1)];if(target)seek(target.start);
  }
  function removeSelection() {
    if(busy)return;
    commitAnnotation.current?.();
    const id=annotating?annotationId:effectId;
    if(!id){removeClip();return;}
    const current=docRef.current;
    apply({...current,effects:current.effects.filter(item=>item.id!==id),annotations:current.annotations.filter(item=>item.id!==id),stickers:current.stickers.filter(item=>item.id!==id)});
    setEffectId(null);setAnnotationId(null);setAnnotationRevision(value=>value+1);
  }
  function setClipRate(player:HTMLVideoElement,segment:VideoSegment){player.playbackRate=segmentSpeed(segment);player.preservesPitch=!isWindows;}
  function playEdit(){
    const player=video.current;if(!player||!valid||busy)return;

    if(!player.paused){previewing.current=false;player.pause();return;}
    commitAnnotation.current?.();setAnnotating(false);
    const clips=docRef.current.segments;
    let index=clips.findIndex((clip,i)=>i===playbackIndex.current&&player.currentTime>=clip.start&&player.currentTime<clip.end-.001);
    if(index<0)index=clips.findIndex(clip=>player.currentTime>=clip.start&&player.currentTime<clip.end-.001);
    if(index<0){index=playbackIndex.current>=clips.length-1?0:Math.max(0,playbackIndex.current);player.currentTime=clips[index].start;}
    playbackIndex.current=index;setSelected(index);setClipRate(player,clips[index]);previewing.current=true;
    void player.play().catch(()=>{previewing.current=false;});
  }
  function updatePlayback(){
    const player=video.current;if(!player)return;
    if(previewing.current&&!player.seeking){
      const clips=docRef.current.segments,current=clips[playbackIndex.current];
      if(current&&player.currentTime>=current.end-.001){
        const next=clips[playbackIndex.current+1];
        if(next){playbackIndex.current++;setSelected(playbackIndex.current);setClipRate(player,next);player.currentTime=next.start;if(player.paused)void player.play().catch(()=>{previewing.current=false;});}
        else{previewing.current=false;player.pause();player.currentTime=current.end;}
      }
    }
    setTime(player.currentTime);
  }

  useEffect(()=>{
    if(!editing || !playing) return;
    let frame=0;
    const tick=()=>{updatePlayback();frame=requestAnimationFrame(tick);};
    frame=requestAnimationFrame(tick);
    return()=>cancelAnimationFrame(frame);
  },[editing,playing]);

  useLayoutEffect(()=>{
    const element=stage.current; if(!element) return;
    const fit=()=>{
      const rect=element.getBoundingClientRect();
      const availableHeight=Math.max(1,rect.height);
      const scale=Math.min(rect.width/sourceSize.width,availableHeight/sourceSize.height);
      setFitted({width:Math.max(1,sourceSize.width*scale),height:Math.max(1,sourceSize.height*scale)});
    };
    fit();const observer=new ResizeObserver(fit);
    observer.observe(element); return()=>observer.disconnect();
  },[sourceSize,editing]);

  // Every edit, including a live annotation draft, uses the export composition order.
  useEffect(()=>{
    const player=video.current,output=canvas.current;
    if(!editing || !player || !output) return;
    const buffer=document.createElement("canvas");
    const scale=Math.min(1,1280/Math.max(sourceSize.width,sourceSize.height));
    buffer.width=Math.max(1,Math.round(sourceSize.width*scale));
    buffer.height=Math.max(1,Math.round(sourceSize.height*scale));
    // Preserve the last presented frame while a paused WebKit seek is decoding.
    if(output.width!==buffer.width)output.width=buffer.width;
    if(output.height!==buffer.height)output.height=buffer.height;
    const maskScratch=document.createElement("canvas");
    const sourceFrame=document.createElement("canvas");sourceFrame.width=sourceSize.width;sourceFrame.height=sourceSize.height;
    const frameCtx=sourceFrame.getContext("2d");
    const sourceCtx=buffer.getContext("2d"),ctx=output.getContext("2d");
    if(!sourceCtx || !ctx || !frameCtx) return;
    let frame=0,repaintAttempts=3,decodedFrame=0;
    const draw=()=>{
      cancelAnimationFrame(frame);
      const clip=docRef.current.segments[playbackIndex.current];
      const inDeletedGap=previewing.current&&(!clip||player.currentTime<clip.start||player.currentTime>=clip.end);
      if(player.readyState>=2 && !player.seeking && !inDeletedGap) {
        const active=effects.filter(e=>player.currentTime>=e.start && player.currentTime<e.end);
        frameCtx.drawImage(player,0,0,sourceSize.width,sourceSize.height);

        sourceCtx.drawImage(sourceFrame,0,0,buffer.width,buffer.height);
        const live=annotating?liveAnnotation.current:null;
        const marks=new Map(live?.marks.map(mark=>[mark.id,mark]));
        const activeAnnotations=annotations.filter(item=>player.currentTime>=item.start&&player.currentTime<item.end).flatMap(item=>{
          const mark=live?marks.get(item.mark.id):item.mark;
          return mark&&mark.id!==live?.editingId?[{...item,mark}]:[];
        });
        const draft=live?.draft?[{id:"draft",mark:live.draft,start:0,end:duration,layer:nextVideoLayer([...effects,...annotations,...stickers])}]:[];
        const layers=orderedVideoLayers([...active.filter(item=>!isVideoAdjustment(item)),...stickers.filter(item=>player.currentTime>=item.start&&player.currentTime<item.end),...activeAnnotations,...draft]);
        for(const item of layers){
          if("mark" in item){sourceCtx.save();sourceCtx.scale(buffer.width/sourceSize.width,buffer.height/sourceSize.height);paintVideoAnnotation(sourceCtx,item.mark,sourceSize,sourceFrame);sourceCtx.restore();}
          else if("dataUrl" in item){const image=stickerImages.current.get(item.id);if(image)sourceCtx.drawImage(image,item.x*buffer.width,item.y*buffer.height,item.width*buffer.width,item.height*buffer.height);}
          else{
            paintVideoEffect(sourceCtx,item,player.currentTime,maskScratch);
          }
        }
        paintVideoEffects(sourceCtx,active.filter(isVideoAdjustment),player.currentTime,maskScratch);
        ctx.clearRect(0,0,output.width,output.height);ctx.drawImage(buffer,0,0);
      }
      if(!player.paused||repaintAttempts-->0) frame=requestAnimationFrame(draw);
    };
    const redraw=()=>{
      repaintAttempts=3;draw();
      // seeked can precede presentation of the decoded frame in WKWebView.
      if(player.requestVideoFrameCallback){
        if(decodedFrame)player.cancelVideoFrameCallback(decodedFrame);
        decodedFrame=player.requestVideoFrameCallback(()=>{decodedFrame=0;draw();});
      }
    };
    const events=["seeked","loadeddata","play","pause"];
    redrawComposite.current=draw;events.forEach(event=>player.addEventListener(event,redraw)); redraw();
    return()=>{redrawComposite.current=null;cancelAnimationFrame(frame);if(decodedFrame)player.cancelVideoFrameCallback(decodedFrame);events.forEach(event=>player.removeEventListener(event,redraw));};
  },[editing,effectId,annotating,effects,annotations,stickers,sourceSize]);

  useEffect(()=>{
    if(!editing) return;
    const onKey=(event:KeyboardEvent)=>{
      const target=event.target as HTMLElement;
      if(event.defaultPrevented||target.closest("input,select,textarea,[contenteditable=true],[role=listbox]")) return;
      if((event.metaKey||event.ctrlKey)&&event.key.toLowerCase()==="z") {event.preventDefault();undo(event.shiftKey);}
      else if(event.code==="Space" && !target.closest("[role=dialog],summary")) {event.preventDefault();playEdit();}
      else if(event.key.toLowerCase()==="s" && !event.metaKey && !event.ctrlKey) {event.preventDefault();split();}
      else if(event.key==="Delete"||event.key==="Backspace") {event.preventDefault();removeSelection();}
    };
    const stopProjectShortcut=installVideoProjectShortcuts(window,{save:()=>{if(!busy)void flushProject();}});
    window.addEventListener("keydown",onKey); return()=>{stopProjectShortcut();window.removeEventListener("keydown",onKey);};
  });

  function dragHandle(event:PointerEvent<HTMLDivElement>,index:number,edge:"start"|"end") {
    if(busy || !track.current) return;
    event.preventDefault();event.stopPropagation();
    const target=event.currentTarget,rect=track.current.getBoundingClientRect(),origin=event.clientX;
    commitAnnotation.current?.();
    const baseline=docRef.current;
    setSelected(index);setEffectId(null);setAnnotating(false);dragBase.current=baseline;
    target.setPointerCapture(event.pointerId);
    const move=(e:globalThis.PointerEvent)=>{
      const value=baseline.segments[index][edge]+(e.clientX-origin)/rect.width*timelineDuration(baseline.segments)*segmentSpeed(baseline.segments[index]);
      const next=trimSegment(baseline.segments,index,edge,value,duration);
      apply({...baseline,segments:next},true);seek(next[index][edge]);playbackIndex.current=index;
    };
    const finish=(e:globalThis.PointerEvent)=>{
      target.removeEventListener("pointermove",move);target.removeEventListener("pointerup",finish);target.removeEventListener("pointercancel",cancel);window.removeEventListener("keydown",key,true);
      if(target.hasPointerCapture(e.pointerId)) target.releasePointerCapture(e.pointerId);
      apply(docRef.current);
    };
    const cancel=(e:globalThis.PointerEvent)=>{docRef.current=baseline;finish(e);setDoc(baseline);};
    const key=(e:globalThis.KeyboardEvent)=>{if(e.key==="Escape"){e.preventDefault();e.stopImmediatePropagation();cancel(new globalThis.PointerEvent("pointercancel",{pointerId:event.pointerId}));}};
    target.addEventListener("pointermove",move);target.addEventListener("pointerup",finish);target.addEventListener("pointercancel",cancel);window.addEventListener("keydown",key,true);
  }

  function reorderClip(from:number,to:number){
    commitAnnotation.current?.();const current=docRef.current,next=moveSegment(current.segments,from,to);if(next===current.segments)return;
    apply({...current,segments:next});setSelected(to);playbackIndex.current=to;setEffectId(null);setAnnotating(false);seek(next[to].start);
  }
  function dragClip(event:PointerEvent<HTMLButtonElement>,index:number){
    if(busy||event.button!==0||!track.current)return;
    if(docRef.current.segments.length===1){scrubTimeline(event);return;}
    event.preventDefault();event.stopPropagation();commitAnnotation.current?.();video.current?.pause();previewing.current=false;
    const target=event.currentTarget,rect=track.current.getBoundingClientRect(),origin=event.clientX;
    const entries=timelineSegments(docRef.current.segments),length=entries.length,extent=timelineDuration(docRef.current.segments);
    let moved=false,slot=index;
    setSelected(index);target.setPointerCapture(event.pointerId);
    const move=(e:globalThis.PointerEvent)=>{
      if(!moved&&Math.abs(e.clientX-origin)<5)return;
      moved=true;setDraggedClip(index);setDragOffset(e.clientX-origin);
      const point=(e.clientX-rect.left)/rect.width*extent;
      slot=entries.findIndex(entry=>point<(entry.start+entry.end)/2);if(slot<0)slot=length;
      setDropIndex(slot);
    };
    const cleanup=()=>{if(target.hasPointerCapture(event.pointerId))target.releasePointerCapture(event.pointerId);target.removeEventListener("pointermove",move);target.removeEventListener("pointerup",finish);target.removeEventListener("pointercancel",cancel);window.removeEventListener("keydown",key,true);setDraggedClip(null);setDropIndex(null);setDragOffset(0);};
    const finish=()=>{cleanup();if(moved){suppressClick.current=true;reorderClip(index,Math.max(0,Math.min(length-1,slot>index?slot-1:slot)));}};
    const cancel=()=>{cleanup();suppressClick.current=moved;};
    const key=(e:globalThis.KeyboardEvent)=>{if(e.key==="Escape"){e.preventDefault();e.stopImmediatePropagation();cancel();}};
    target.addEventListener("pointermove",move);target.addEventListener("pointerup",finish);target.addEventListener("pointercancel",cancel);window.addEventListener("keydown",key,true);
  }
  function scrubTimeline(event:PointerEvent<HTMLElement>){
    if(busy||event.button!==0||!track.current||!total)return;
    event.preventDefault();event.stopPropagation();commitAnnotation.current?.();setAnnotating(false);setEffectId(null);setAnnotationId(null);
    const target=event.currentTarget,rect=track.current.getBoundingClientRect();target.focus();target.setPointerCapture(event.pointerId);
    const move=(e:globalThis.PointerEvent)=>seekOutput(Math.max(0,Math.min(total,(e.clientX-rect.left)/rect.width*total)));
    const finish=()=>{target.removeEventListener("pointermove",move);target.removeEventListener("pointerup",finish);target.removeEventListener("pointercancel",finish);if(target.hasPointerCapture(event.pointerId))target.releasePointerCapture(event.pointerId);};
    move(event.nativeEvent);target.addEventListener("pointermove",move);target.addEventListener("pointerup",finish);target.addEventListener("pointercancel",finish);
  }
  function resizeTimeline(event:PointerEvent<HTMLDivElement>){
    if(event.button!==0)return;event.preventDefault();const target=event.currentTarget,origin=event.clientY,initial=timelineHeight;target.setPointerCapture(event.pointerId);
    const move=(e:globalThis.PointerEvent)=>{const maximum=Math.min(360,(container.current?.clientHeight??700)*.45);setTimelineHeight(Math.max(160,Math.min(maximum,initial+origin-e.clientY)));};
    const finish=()=>{target.removeEventListener("pointermove",move);target.removeEventListener("pointerup",finish);target.removeEventListener("pointercancel",finish);};
    target.addEventListener("pointermove",move);target.addEventListener("pointerup",finish);target.addEventListener("pointercancel",finish);
  }

  async function addSticker(file:File){
    if(importing||busy)return;setImporting(true);setStickerError(false);commitAnnotation.current?.();
    try{
      const {dataUrl,image}=await importVideoSticker(file);if(!alive.current)return;
      const current=docRef.current;
      if([...stickerImages.current.values()].reduce((sum,image)=>sum+image.naturalWidth*image.naturalHeight*4,0)+image.naturalWidth*image.naturalHeight*4>128*1024*1024)throw Error("Sticker memory limit");
      if(current.annotations.length+current.stickers.length>=128||current.stickers.reduce((sum,item)=>sum+item.dataUrl.length,0)+dataUrl.length>32*1024*1024)throw Error("Sticker limit");
      const ratio=image.naturalWidth/image.naturalHeight,sourceRatio=sourceSize.width/sourceSize.height;
      const width=Math.min(.3,.3*ratio/sourceRatio),height=width*sourceRatio/ratio;
      const {start,end}=defaultOverlayRange(video.current?.currentTime??time,duration);
      const item:VideoSticker={layer:nextVideoLayer([...current.effects,...current.annotations,...current.stickers]),id:`sticker-${crypto.randomUUID()}`,start,end,x:(1-width)/2,y:(1-height)/2,width,height,dataUrl};
      stickerImages.current.set(item.id,image);apply({...current,stickers:[...current.stickers,item]});setAnnotating(false);setAnnotationId(null);setEffectId(item.id);seek(videoLayerPreviewTime(start,end));
    }catch{if(alive.current)setStickerError(true);}finally{if(alive.current)setImporting(false);}
  }

  async function saveCopy() {
    if(saving.current||!valid||project.state.error?.includes("SOURCE_CHANGED")) return;
    commitAnnotation.current?.();
    textDraft.current=null;project.schedule();
    const snapshot=docRef.current;
    saving.current=true;setBusy(true);setError(false);setSavedId(null);setAnnotating(false);
    const requestId=crypto.randomUUID();exportRequest.current=requestId;exportStarted.current=false;cancelRequested.current=false;
    setExportProgress({requestId,phase:"preparing",progress:null});setCancelling(false);setExportCancelled(false);setCancelFailed(false);
    previewing.current=false;video.current?.pause();setEffectId(null);
    let stop:(()=>void)|undefined;
    try {
      // Register before invoking so even a small, fast export reports its state.
      stop=await getCurrentWindow().listen<VideoExportProgress>("video-export-progress",event=>{
        if(event.payload.requestId!==exportRequest.current)return;
        const progress=event.payload.progress;
        setExportProgress({...event.payload,progress:progress===null||!Number.isFinite(progress)?null:Math.max(0,Math.min(1,progress))});
      });
      // Export must start even when WebKit suspends animation frames in a covered window.
      await new Promise<void>(resolve=>setTimeout(resolve,0));
      if(cancelRequested.current){setExportCancelled(true);return;}
      const rasterized=[...rasterizeVideoAnnotations(snapshot.annotations,sourceSize),...rasterizeVideoStickers(snapshot.stickers)];
      exportStarted.current=true;
      setSavedId(await api.exportVideoCopy(props.id,snapshot.segments,snapshot.effects,rasterized,preset,requestId));
    }
    catch(reason) {if(String(reason)==="VIDEO_EXPORT_CANCELLED")setExportCancelled(true);else setError(true);}
    finally {stop?.();exportRequest.current=null;exportStarted.current=false;cancelRequested.current=false;saving.current=false;setBusy(false);setCancelling(false);setExportProgress(null);}
  }
  async function cancelExport(){
    const requestId=exportRequest.current;if(!requestId||cancelling||exportProgress?.phase==="saving")return;
    cancelRequested.current=true;setCancelling(true);setCancelFailed(false);
    if(!exportStarted.current)return;
    try{
      const accepted=await api.cancelVideoExport(requestId);
      if(exportRequest.current!==requestId)return;
      if(!accepted){cancelRequested.current=false;setCancelling(false);setExportProgress({requestId,phase:"saving",progress:null});}
    }catch{
      if(exportRequest.current===requestId){cancelRequested.current=false;setCancelling(false);setCancelFailed(true);}
    }
  }

  useEffect(()=>{
    if(!annotating)return;
    setAnnotationRevision(value=>value+1);
    const player=video.current;if(!player)return;
    let disposed=false;
    const capture=()=>{
      if(player.readyState<2||player.seeking)return;
      const bitmap=document.createElement("canvas");bitmap.width=sourceSize.width;bitmap.height=sourceSize.height;
      const ctx=bitmap.getContext("2d");if(!ctx)return;ctx.drawImage(player,0,0);
      const image=new Image();image.onload=()=>{if(!disposed)setSourceImage(image);};image.src=bitmap.toDataURL("image/png");
    };
    capture();player.addEventListener("seeked",capture);
    return()=>{disposed=true;player.removeEventListener("seeked",capture);};
  },[annotating,time,sourceSize,stickers,effectId]);
  function changeAnnotationMarks(marks:AnnotationMark[]){
    const current=docRef.current;
    const visibleIds=new Set(visibleAnnotations.map(item=>item.mark.id));
    const incoming=new Map(marks.map(mark=>[mark.id,mark]));
    const retained=current.annotations.flatMap(item=>{
      if(!visibleIds.has(item.mark.id))return [item];
      const mark=incoming.get(item.mark.id);return mark?[{...item,mark}]:[];
    });
    const existing=new Set(current.annotations.map(item=>item.mark.id));
    const {start,end}=defaultOverlayRange(time,duration);
    const added=marks.filter(mark=>!existing.has(mark.id)).map((mark,index)=>({id:`annotation-${mark.id}`,mark,start,end,layer:nextVideoLayer([...current.effects,...current.annotations,...current.stickers])+index}));
    if(retained.length+added.length>128){setAnnotationRevision(value=>value+1);setError(true);return;}
    apply({...current,annotations:[...retained,...added]});
    if(added.length){
      setAnnotationId(added[added.length-1].id);
      const player=video.current;
      if(player&&(player.currentTime<start||player.currentTime>=end)){
        const next=videoLayerPreviewTime(start,end);player.currentTime=next;setTime(next);
      }
    }
  }
  function selectTrack(id:string){

    commitAnnotation.current?.();
    setAnnotationRevision(value=>value+1);
    if(id.startsWith("annotation-")){liveAnnotation.current=null;setAnnotationId(id);setEffectId(null);setAnnotating(true);}
    else{setAnnotating(false);setAnnotationId(null);setEffectId(id);}
  }
  function changeEffects(next:VideoEffect[],transient=false){
    const current=docRef.current,rank=nextVideoLayer([...current.effects,...current.annotations,...current.stickers]);
    apply({...current,effects:next.map((item,index)=>item.layer===undefined?{...item,layer:rank+index}:item)},transient);
  }
  function changeTracks(next:VideoEffect[],transient=false){
    if(!dragBase.current)commitAnnotation.current?.();
    if(!transient)setAnnotationRevision(value=>value+1);
    const byId=new Map(next.map(item=>[item.id,item]));
    apply({...docRef.current,effects:next.filter(item=>!item.id.startsWith("annotation-")&&!item.id.startsWith("sticker-")),stickers:docRef.current.stickers.map(item=>{const timing=byId.get(item.id);return timing?{...item,start:timing.start,end:timing.end,layer:timing.layer}:item;}),annotations:docRef.current.annotations.map(item=>{
      const timing=byId.get(item.id);return timing?{...item,start:timing.start,end:timing.end,layer:timing.layer}:item;
    })},transient);
  }

  // Selecting a sticker or effect must not make the ink on the picture unclickable.
  // A stable surface owns cross-tool drags while the corresponding inspector mounts.
  function selectPictureObject(event:PointerEvent<HTMLDivElement>){
    if(!editing||playing||busy||event.button!==0||(annotating&&annotationTool!=="select"))return;
    // A selected camera tool owns the picture: dragging an existing annotation
    // underneath it must adjust the framing, not unexpectedly switch tools.
    if(!annotating&&docRef.current.effects.some(item=>item.id===effectId&&(item.kind==="zoom"||item.kind==="frame")))return;
    if(event.target instanceof HTMLElement&&event.target.closest("textarea,.kiri-video-effect-resize"))return;
    const rect=event.currentTarget.getBoundingClientRect(),transform=previewTransform;
    const x=((event.clientX-rect.left)/rect.width-transform.x)/transform.sx;
    const y=((event.clientY-rect.top)/rect.height-transform.y)/transform.sy;
    const current=docRef.current;
    if(annotating&&selectedAnnotation){
      const mark=selectedAnnotation.mark,p={x:x*sourceSize.width,y:y*sourceSize.height};
      const radius=10*sourceSize.width/(rect.width*transform.sx);
      if(mark.kind==="line"||mark.kind==="arrow"){
        if(Math.hypot(p.x-mark.start.x,p.y-mark.start.y)<=radius||Math.hypot(p.x-mark.end.x,p.y-mark.end.y)<=radius)return;
      }else if(hitTestHandle(p,selectionBounds(mark),radius))return;
    }
    const choices=orderedVideoLayers([
      ...current.annotations.map(item=>({...item,type:"annotation" as const})),
      ...current.stickers.map(item=>({...item,type:"sticker" as const})),
      ...current.effects.filter(item=>!isVideoAdjustment(item)).map(item=>({...item,type:"effect" as const})),
    ]).reverse();
    const hit=choices.find(item=>time>=item.start&&time<item.end&&(item.type==="annotation"?
      markIndexAt([item.mark],{x:x*sourceSize.width,y:y*sourceSize.height},{x:sourceSize.width/(rect.width*transform.sx),y:sourceSize.height/(rect.height*transform.sy),radial:sourceSize.width/(rect.width*transform.sx)})!==null:
      x>=item.x&&x<=item.x+item.width&&y>=item.y&&y<=item.y+item.height));
    if(!hit)return;
    if(hit.type==="annotation"&&annotating)return; // Shared canvas owns ink and its handles.
    if(hit.type!=="annotation"&&!annotating&&effectId===hit.id)return;
    event.preventDefault();event.stopPropagation();commitAnnotation.current?.();selectTrack(hit.id);
    const target=event.currentTarget,origin={x:event.clientX,y:event.clientY};target.setPointerCapture(event.pointerId);
    let latest=current,moved=false;
    const move=(e:globalThis.PointerEvent)=>{
      if(!moved&&Math.hypot(e.clientX-origin.x,e.clientY-origin.y)<2)return;
      moved=true;
      const dx=(e.clientX-origin.x)/(rect.width*transform.sx),dy=(e.clientY-origin.y)/(rect.height*transform.sy);
      if(hit.type==="annotation")latest={...current,annotations:current.annotations.map(item=>item.id===hit.id?{...item,mark:translateMark(item.mark,{x:dx*sourceSize.width,y:dy*sourceSize.height},{x:0,y:0,...sourceSize})}:item)};
      else if(hit.type==="sticker"){
        const next=moveVideoEffect({...hit,kind:"mask"},dx,dy);
        latest={...current,stickers:current.stickers.map(item=>item.id===hit.id?{...item,x:next.x,y:next.y}:item)};
      }else latest={...current,effects:current.effects.map(item=>item.id===hit.id?moveVideoEffect(item,dx,dy):item)};
      liveAnnotation.current=null;apply(latest,true);if(hit.type==="annotation")setAnnotationRevision(value=>value+1);
    };
    const cleanup=()=>{target.removeEventListener("pointermove",move);target.removeEventListener("pointerup",finish);target.removeEventListener("pointercancel",cancel);window.removeEventListener("keydown",key,true);cancelCanvasDrag.current=null;if(target.hasPointerCapture(event.pointerId))target.releasePointerCapture(event.pointerId);};
    const finish=()=>{cleanup();if(moved){apply(latest);setAnnotationRevision(value=>value+1);}};
    const cancel=()=>{cleanup();if(moved){apply(current);setAnnotationRevision(value=>value+1);}return true;};
    const key=(e:globalThis.KeyboardEvent)=>{if(e.key==="Escape"){e.preventDefault();e.stopImmediatePropagation();cancel();}};
    cancelCanvasDrag.current=cancel;
    target.addEventListener("pointermove",move);target.addEventListener("pointerup",finish);target.addEventListener("pointercancel",cancel);window.addEventListener("keydown",key,true);
  }

  return <div ref={container} className={`kiri-video-player ${editing ? "kiri-video-player--editing" : ""}`}>
    <VideoCloseGuard busy={busy} prepareClose={flushProject}/>
    <header className="kiri-video-editor-heading">
      <div><strong>{t(editing ? "Video editor" : "Video")}</strong><span>{t(editing ? "Your original recording stays unchanged." : "Esc to close")}</span></div>
      <div className="kiri-video-header-actions">
        {editing&&<div className="kiri-video-project-status" role="status" aria-live="polite">{project.state.status==="error"?<button type="button" className="kiri-video-save-retry" onClick={project.retry}>{t("Not saved · Retry")}</button>:t(project.state.status==="saved"?"Edit saved":project.state.status==="saving"||project.state.status==="waiting"?"Saving edit…":"Edits save automatically")}</div>}
        {editing&&<VideoExportPanel preset={preset} onPreset={next=>{setPreset(next);projectContext.current.preset=next;project.schedule();setSavedId(null);}} sourceSize={sourceSize} duration={total} valid={valid&&!project.state.error?.includes("SOURCE_CHANGED")} busy={busy} error={error} saved={!!savedId} progress={exportProgress} cancelling={cancelling} cancelled={exportCancelled} cancelFailed={cancelFailed} onCancel={()=>void cancelExport()} onSave={()=>void saveCopy()} onOpen={()=>{if(savedId)void api.openAsset(savedId).catch(()=>setError(true));}}/>}


        {editing ? <button type="button" className="kiri-button kiri-button--secondary" disabled={busy} onClick={()=>{commitAnnotation.current?.();previewing.current=false;video.current?.pause();setEditing(false);setAnnotating(false);}}>{t("Close editor")}</button>
          : props.editable && <button type="button" className="kiri-button kiri-button--primary" disabled={duration<=0||!project.ready} onClick={()=>{video.current?.pause();if((video.current?.currentTime??0)>=duration-.001)seek(0);setEditing(true);}}><Scissors size={14}/>{t(project.state.status==="loading"?"Loading edit…":"Trim & Export")}</button>}
        <button type="button" className="kiri-icon-button" aria-label={t("Close · Esc")} title={t("Close · Esc")} onClick={props.onClose}><X size={16}/></button>
      </div>
    </header>
    {(project.state.status==="blocked"||project.state.status==="error")&&<div className="kiri-video-project-notice" role="alert"><p>{t(project.state.error?.includes("SOURCE_CHANGED")?"The original video changed. The saved edit has been kept and cannot be applied to this file.":project.state.status==="blocked"
      ?"Couldn't open the saved edit. It has been kept unchanged; you can still watch the original video."
      :project.state.error?.includes("CONFLICT")?"This edit was saved elsewhere. Your changes are still here; exporting a copy will keep the finished video."
      :"Couldn't save this edit. Your latest changes are still here. Retry before closing.")}</p><button type="button" className="kiri-button kiri-button--secondary" onClick={project.retry}>{t("Retry")}</button></div>}
    {stickerError&&<p role="alert" className="kiri-video-sticker-error">{t("Choose a PNG, JPEG or WebP image up to 10 MB and 16 megapixels.")}</p>}
    {editing&&<div ref={setAnnotationToolbar} className="kiri-video-annotation-toolbar-host"/>}
    <div className="kiri-video-workspace">
      <div ref={stage} className="kiri-video-stage">
        <div className="kiri-video-surface" onPointerDownCapture={selectPictureObject} style={{width:fitted.width,height:fitted.height}}>
          <video ref={video} src={props.src} crossOrigin="anonymous" controls={false} playsInline autoPlay preload="metadata"
            // Keep the decoder's presentation surface live for paused WebKit seeks;
            // the opaque edited canvas above it owns the visible picture.
            style={{pointerEvents:editing?"none":"auto"}}
            onLoadedMetadata={event=>{
              const player=event.currentTarget,value=player.duration;
              const d=Number.isFinite(value)?value:0;setDuration(d);
              setSourceSize({width:player.videoWidth||16,height:player.videoHeight||9});
              const initial={segments:d>0?[{start:0,end:d}]:[],effects:[],annotations:[],stickers:[]};docRef.current=initial;setDoc(initial);
            }}
            onTimeUpdate={updatePlayback} onSeeked={updatePlayback}
            onPlay={()=>setPlaying(true)} onPause={()=>{setPlaying(false);scheduleProject.current?.();}} onEnded={()=>{updatePlayback();if(!previewing.current)setPlaying(false);scheduleProject.current?.();}}
            onError={props.onError} />
          {editing && <canvas ref={canvas} className="kiri-video-effect-preview" aria-label={t("Edited video preview")} />}
          {editing&&<div className="kiri-video-annotation-editor" style={{pointerEvents:annotating?"auto":"none",clipPath:annotationClip,transformOrigin:"0 0",transform:`translate(${previewTransform.x*fitted.width}px, ${previewTransform.y*fitted.height}px) scale(${previewTransform.sx}, ${previewTransform.sy})`}}><VideoAnnotationsEditor onTextDraftChange={receiveTextDraft} onToolChange={setAnnotationTool} onCancelGesture={()=>cancelCanvasDrag.current?.()??false} onLiveMarks={receiveLiveMarks} active={annotating} onActivate={()=>{video.current?.pause();previewing.current=false;setEffectId(null);if(!annotating)liveAnnotation.current=null;setAnnotating(true);}} extraTools={<><button type="button" className="kiri-video-annotation-tool" disabled={busy||importing||annotations.length+stickers.length>=128} title={t("Add image sticker")} aria-label={t("Add image sticker")} onClick={()=>stickerInput.current?.click()}><ImagePlus size={17}/></button><button type="button" className="kiri-video-effect-tool" disabled={busy} onClick={()=>{commitAnnotation.current?.();setAnnotating(false);setAnnotationId(null);setEffectId(null);}}><SlidersHorizontal size={15}/>{t("Add effect")}</button><input ref={stickerInput} type="file" accept="image/png,image/jpeg,image/webp" hidden onChange={event=>{const file=event.target.files?.[0];event.target.value="";if(file)void addSticker(file);}}/></>} onCommitReady={registerAnnotationCommit} toolbarHost={annotationToolbar} appearanceHost={appearanceHost} image={sourceImage} sourceSize={sourceSize} viewSize={fitted} marks={visibleAnnotations.map(item=>item.mark)} revision={annotationRevision} selectedMarkId={selectedAnnotation?.mark.id??null} onSelectionChange={markId=>setAnnotationId(markId===null?null:docRef.current.annotations.find(item=>item.mark.id===markId)?.id??null)} disabled={busy} onChange={changeAnnotationMarks} onUndo={()=>undo()} onRedo={()=>undo(true)} canUndo={!!history.current.past.length} canRedo={!!history.current.future.length} onClose={()=>{setAnnotating(false);setEffectId(null);setAnnotationId(null);}}/></div>}

          {editing && !playing && effectId && !selectedSticker && <VideoEffectsOverlay transform={previewTransform} sourceSize={sourceSize} effects={effects} onChange={changeEffects} selectedId={effectId} onSelect={setEffectId} time={time} duration={duration} disabled={busy} />}
          {editing&&!playing&&!annotating&&<VideoEffectsOverlay transform={previewTransform} regionLabel={t("Sticker")} effects={stickers.map(item=>({...item,kind:"mask"}))} selectedId={effectId} onSelect={id=>{video.current?.pause();previewing.current=false;setEffectId(id);setAnnotationId(null);}} time={time} duration={duration} disabled={busy} onChange={(next,transient)=>{const rects=new Map(next.map(item=>[item.id,item]));apply({...docRef.current,stickers:docRef.current.stickers.map(item=>{const rect=rects.get(item.id);return rect?{...item,x:rect.x,y:rect.y,width:rect.width,height:rect.height}:item;})},transient);}}/>}
        </div>

      </div>
      {editing && <aside className="kiri-video-inspector">{selectedSticker?<section className="kiri-video-annotation-inspector"><strong>{t("Sticker")}</strong><p>{t("Drag to move. Resize with the handles; set timing on its track.")}</p><button type="button" className="kiri-button kiri-button--secondary" disabled={busy} onClick={()=>{apply({...docRef.current,stickers:docRef.current.stickers.filter(item=>item.id!==selectedSticker.id)});setEffectId(null);}}><Trash2 size={14}/>{t("Delete sticker")}</button></section>:annotating?<section className="kiri-video-annotation-inspector"><div ref={setAppearanceHost}/>
        {selectedAnnotation&&<><details className="kiri-effect-timing" key={selectedAnnotation.id}><summary><span>{t("Visible during")}</span><VideoVisibleTime segments={segments} start={selectedAnnotation.start} end={selectedAnnotation.end}/><ChevronDown size={12}/></summary><p>{t("Times refer to the original video. You can also drag the track edges below.")}</p>{(["start","end"] as const).map(edge=><label key={edge}>{t(edge==="start"?"Effect start":"Effect end")}<VideoTimeInput value={selectedAnnotation[edge]} min={edge==="start"?0:selectedAnnotation.start+.05} max={edge==="start"?selectedAnnotation.end-.05:duration} step={.1} disabled={busy} onCommit={value=>{
          commitAnnotation.current?.();
          const current=docRef.current,selected=current.annotations.find(item=>item.id===selectedAnnotation.id);if(!selected)return;
          const next={...selected,[edge]:value};if(!Number.isFinite(value)||next.start<0||next.end>duration||next.end-next.start<.05-1e-9)return;apply({...current,annotations:current.annotations.map(item=>item.id===next.id?next:item)});setAnnotationRevision(v=>v+1);
        }}/></label>)}</details><button type="button" className="kiri-button kiri-button--secondary" disabled={busy} onClick={()=>{commitAnnotation.current?.();apply({...docRef.current,annotations:docRef.current.annotations.filter(item=>item.id!==selectedAnnotation.id)});setAnnotationId(null);setAnnotationRevision(v=>v+1);}}><Trash2 size={14}/>{t("Delete annotation")}</button></>}
        </section>:<VideoEffectsControls segments={segments} sourceSize={sourceSize} effects={effects} onChange={changeEffects} selectedId={effectId} onSelect={id=>{video.current?.pause();previewing.current=false;setEffectId(id);}} time={time} duration={duration} disabled={busy} onSeek={seek} />}</aside>}
    </div>
    {!editing&&<VideoPlaybackControls video={video}/>}
    {editing&&<><div className="kiri-video-timeline-divider" role="separator" aria-label={t("Timeline height")} aria-orientation="horizontal" aria-valuemin={160} aria-valuemax={360} aria-valuenow={timelineHeight} tabIndex={0} onPointerDown={resizeTimeline} onDoubleClick={()=>setTimelineHeight(230)} onKeyDown={event=>{if(["ArrowUp","ArrowDown"].includes(event.key)){event.preventDefault();setTimelineHeight(value=>Math.max(160,Math.min(360,(container.current?.clientHeight??700)*.45,value+(event.key==="ArrowUp"?20:-20))));}}}/><section className="kiri-video-timeline" aria-label={t("Video timeline")} style={{height:timelineHeight}}>
      <div className="kiri-video-edit-tools">
        <button type="button" className="kiri-button kiri-button--secondary" disabled={!valid||busy} onClick={playEdit} title={t("Play edited video · Space")}>{playing?<Pause size={14}/>:<Play size={14}/>} {t(playing?"Pause":"Play")}</button>
        <span className="kiri-video-clock">{videoTimeLabel(clockTime)}<span> / {videoTimeLabel(total)}</span></span>
        <div className="kiri-video-tool-divider"/>
        <button type="button" className="kiri-button kiri-button--secondary" disabled={!canSplit||busy} onClick={split} title={t("Split at playhead · S")}><Scissors size={14}/>{t("Split")}</button>
        <button type="button" className="kiri-button kiri-button--secondary" disabled={(!selectedTrack&&(!selectedClip||annotating))||busy} onClick={removeSelection}><Trash2 size={14}/>{t(selectedTrack?(selectedAnnotation?"Delete annotation":selectedSticker?"Delete sticker":"Delete effect"):annotating?"Delete annotation":"Delete segment")}</button>
        <div className="kiri-video-history">
          <button type="button" className="kiri-button kiri-button--secondary" disabled={!history.current.past.length||busy} onClick={()=>undo()} title={t("Undo")} aria-label={t("Undo")}><Undo2 size={14}/></button>
          <button type="button" className="kiri-button kiri-button--secondary" disabled={!history.current.future.length||busy} onClick={()=>undo(true)} title={t("Redo")} aria-label={t("Redo")}><Redo2 size={14}/></button>
          <button type="button" className="kiri-button kiri-button--secondary" disabled={busy} onClick={()=>{commitAnnotation.current?.();apply({segments:[{start:0,end:duration}],effects:[],annotations:[],stickers:[]});setAnnotationRevision(v=>v+1);setSelected(0);setEffectId(null);seek(0);}} title={t("Reset edit")} aria-label={t("Reset edit")}><RotateCcw size={14}/></button>
        </div>
      </div>
      <div className="kiri-video-lanes-scroll"><div className="kiri-video-lanes">
      <div className="kiri-video-ruler">
        {Array.from({length:6},(_,i)=><span key={i}>{videoTimeLabel(total*i/5)}</span>)}
        <input type="range" min={0} max={total} step="any" value={Math.min(total,clockTime)} disabled={busy||!total} aria-label={t("Playhead")} aria-valuetext={videoTimeLabel(clockTime)} onPointerDown={scrubTimeline} onChange={event=>seekOutput(Number(event.target.value))}/>
      </div>
      <div ref={track} className="kiri-video-track" onClick={event=>{if(busy||!track.current)return;const rect=track.current.getBoundingClientRect();seekOutput((event.clientX-rect.left)/rect.width*total);}}>
        <span className="kiri-video-lane-label">{t("Video")}</span>
        {timeline.map(({segment:clip,index,start,end})=><div key={index} className="kiri-video-clip" data-selected={index===selected&&!selectedTrack&&!annotating} data-dragging={index===draggedClip} style={{left:`${total?start/total*100:0}%`,width:`${total?(end-start)/total*100:0}%`,transform:index===draggedClip?`translateX(${dragOffset}px)`:undefined}}>
          <div className="kiri-video-filmstrip" aria-hidden="true">{Array.from({length:Math.max(1,Math.min(8,Math.ceil((end-start)/Math.max(total,.1)*12)))},(_,i)=>{const source=clip.start+(clip.end-clip.start)*i/Math.max(1,Math.ceil((end-start)/Math.max(total,.1)*12));const frame=frames[Math.min(frames.length-1,Math.floor(source/Math.max(duration,.1)*frames.length))];return <div key={i}>{frame&&<img src={frame} draggable={false} alt=""/>}</div>;})}</div>
          <button type="button" className="kiri-video-clip-select" disabled={busy} aria-pressed={index===selected} aria-label={fmt("Segment %d: %@ to %@",index+1,videoTimeLabel(clip.start),videoTimeLabel(clip.end))} data-reorderable={segments.length>1} title={t(segments.length>1?"Drag to reorder. Alt + arrow keys also moves the clip.":"Drag to scrub")} onPointerDown={event=>dragClip(event,index)} onKeyDown={event=>{if(event.altKey&&["ArrowLeft","ArrowRight"].includes(event.key)){event.preventDefault();reorderClip(index,Math.max(0,Math.min(segments.length-1,index+(event.key==="ArrowLeft"?-1:1))));}}} onClick={event=>{event.stopPropagation();if(suppressClick.current){suppressClick.current=false;return;}commitAnnotation.current?.();setSelected(index);setEffectId(null);setAnnotating(false);const rect=track.current!.getBoundingClientRect();seekOutput(event.detail===0?start:Math.min(end,Math.max(start,(event.clientX-rect.left)/rect.width*total)));}}><span>{String(index+1).padStart(2,"0")}</span>{segmentSpeed(clip)!==1&&<span className="kiri-video-clip-rate">{segmentSpeed(clip)}×</span>}</button>
          {(["start","end"] as const).map(edge=><div key={edge} className={`kiri-video-trim-handle kiri-video-trim-handle--${edge}`} role="slider" tabIndex={busy?-1:0} aria-label={fmt(edge==="start"?"Segment %d start":"Segment %d end",index+1)} aria-valuemin={0} aria-valuemax={duration} aria-valuenow={clip[edge]} aria-valuetext={videoTimeLabel(clip[edge])} onPointerDown={event=>dragHandle(event,index,edge)} onClick={event=>event.stopPropagation()} onKeyDown={event=>{if(busy||!["ArrowLeft","ArrowRight"].includes(event.key))return;event.preventDefault();const next=trimSegment(segments,index,edge,clip[edge]+(event.key==="ArrowLeft"?-1:1)*(event.shiftKey?1:.1),duration);apply({...docRef.current,segments:next});seek(next[index][edge]);playbackIndex.current=index;}}/>) }
        </div>)}
        {dropIndex!==null&&<div className="kiri-video-drop-marker" style={{left:`${total?(timeline[dropIndex]?.start??total)/total*100:0}%`}}/>}
        <div className="kiri-video-playhead" style={{left:`${total?Math.min(total,clockTime)/total*100:0}%`}}><span onPointerDown={scrubTimeline} title={t("Drag to scrub")}/></div>
      </div>
      <VideoOutputEffectTracks playhead={clockTime} effects={timelineItems} labels={trackLabels} segments={segments} sourceDuration={duration} selectedId={annotating?annotationId:effectId} disabled={busy} onSelect={selectTrack} onSeek={seek} onChange={changeTracks}/>
      </div></div>
      <div className="kiri-video-timeline-detail">
        {thumbnailFailed&&<span>{t("Thumbnails unavailable; editing still works.")}</span>}
        {selectedTrack?<div className="kiri-video-track-help"><strong>{trackLabels[selectedTrack.id]??t(effectLabels[selectedTrack.kind])}</strong><span>{t("Drag the grip to reorder, the middle to move, or either edge to change the duration.")}</span></div>:selectedClip&&<div className="kiri-video-clip-fields"><span>{fmt("Segment %d",selected+1)}</span><ChoiceSelect label={t("Clip speed")} value={String(segmentSpeed(selectedClip))} disabled={busy} onChange={value=>{commitAnnotation.current?.();apply({...docRef.current,segments:docRef.current.segments.map((clip,index)=>index===selected?{...clip,speed:Number(value)}:clip)});}} options={[.25,.5,.75,1,1.25,1.5,2,3,4].map(speed=>({value:String(speed),label:`${speed}×`,description:speed===1?t("Original speed"):undefined}))}/><span>{videoTimeLabel((selectedClip.end-selectedClip.start)/segmentSpeed(selectedClip))}</span>{(["start","end"] as const).map(edge=><label key={edge}>{t(edge==="start"?"Source in":"Source out")}<VideoTimeInput key={`${selected}-${edge}`}  min={0} max={duration} step={.1} disabled={busy} aria-label={t(edge==="start"?"Start time in seconds":"End time in seconds")} value={Number(selectedClip[edge].toFixed(3))} onCommit={value=>{const next=trimSegment(segments,selected,edge,value,duration);apply({...docRef.current,segments:next});seek(next[selected][edge]);playbackIndex.current=selected;}}/></label>)}</div>}
      </div>
      {isWindows&&selectedClip&&segmentSpeed(selectedClip)!==1&&<p className="kiri-video-pitch-note">{t("Changing clip speed also changes audio pitch on Windows.")}</p>}
    </section></>}
  </div>;
}
