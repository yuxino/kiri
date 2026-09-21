import {useCallback, useEffect, useRef, useState, type ReactNode} from "react";
import {createPortal} from "react-dom";
import {VideoEffectSlider} from "./VideoEffectSlider";
import {ChoiceSelect} from "../components/ChoiceSelect";
import AnnotationCanvas, {type AnnotationCanvasHandle} from "../annotation/AnnotationCanvas";
import {COLOR_HEX, COLOR_LABELS, COLOR_PRESETS, type AnnotationMark, type AppearanceSettings, type MosaicShape, type Tool} from "../annotation/model";
import {KiriIcon, type IconName} from "../components/KiriIcons";
import {t} from "../i18n";
import "./VideoAnnotationsEditor.css";
import {useAnnotationAppearance} from "../annotation/useAnnotationAppearance";

export type VideoAnnotationsEditorProps={
  onPendingTextChange?(pending:boolean):void;
  onLiveMarks?(marks:AnnotationMark[],draft:AnnotationMark|null,editingId:number|null):void;
  onFrame?(canvas:HTMLCanvasElement):void;active:boolean;onActivate():void;extraTools?:ReactNode;image:HTMLImageElement|null;sourceSize:{width:number;height:number};viewSize:{width:number;height:number};
  onToolChange?(tool:Tool):void;onCancelGesture?():boolean;
  marks:AnnotationMark[];revision:number;toolbarHost?:HTMLElement|null;appearanceHost?:HTMLElement|null;selectedMarkId?:number|null;onSelectionChange?(markId:number|null):void;disabled:boolean;
  onCommitReady?(commit:(()=>void)|null):void;
  onChange(marks:AnnotationMark[]):void;onUndo():void;onRedo():void;canUndo:boolean;canRedo:boolean;onClose():void;
};
const tools:{tool:Tool;icon:IconName;label:string;name:string;hint:string;key:string}[]=[
  {tool:"select",icon:"cursorarrow",label:"Select (V)",name:"Select",hint:"Click an object to edit it. Drag its handles to resize.",key:"v"},
  {tool:"pen",icon:"pencil.tip",label:"Pen (P)",name:"Pen",hint:"Draw on the picture. Your stroke stays editable.",key:"p"},
  {tool:"rectangle",icon:"rectangle.dashed",label:"Rectangle (R)",name:"Rectangle",hint:"Drag on the picture to draw a rectangle.",key:"r"},
  {tool:"line",icon:"line.diagonal",label:"Line (L)",name:"Line",hint:"Drag to draw a line. Move either endpoint to adjust it.",key:"l"},
  {tool:"arrow",icon:"arrow.up.right",label:"Arrow (A)",name:"Arrow",hint:"Drag toward the detail you want to point out.",key:"a"},
  {tool:"text",icon:"textformat",label:"Text (T)",name:"Text",hint:"Click the picture to type. Enter finishes; Shift + Enter adds a line.",key:"t"},
  {tool:"mosaic",icon:"square.grid.3x3.fill",label:"Mosaic (M)",name:"Mosaic",hint:"Brush over private details, or drag a rectangle or ellipse.",key:"m"},
];
const noop=()=>{};
export function VideoAnnotationsEditor(props:VideoAnnotationsEditorProps) {
  const canvas=useRef<AnnotationCanvasHandle>(null);
  const [tool,setTool]=useState<Tool>("select");
  useEffect(()=>props.onToolChange?.(tool),[tool,props.onToolChange]);
  const [mosaicShape,setMosaicShape]=useState<MosaicShape>("brush");
  const [selection,setSelection]=useState<{mark:AnnotationMark|null;editing:boolean}>({mark:null,editing:false});
  useEffect(()=>{props.onPendingTextChange?.(selection.editing);return()=>props.onPendingTextChange?.(false);},[selection.editing,props.onPendingTextChange]);
  const receiveSelection=useCallback((mark:AnnotationMark|null,editing:boolean)=>setSelection({mark,editing}),[]);
  const created=useCallback(()=>setTool("select"),[]);
  useEffect(()=>{
    props.onCommitReady?.(()=>{canvas.current?.finishAppearanceAdjustment();canvas.current?.endTextFontSizeAdjustment();canvas.current?.commitTextEditing();});
    return()=>props.onCommitReady?.(null);
  },[props.onCommitReady]);
  // Picking a track selects an object, regardless of the last drawing tool used.
  useEffect(()=>{if(props.selectedMarkId!=null)setTool("select");},[props.selectedMarkId]);
  const [appearance,setAppearance]=useAnnotationAppearance();
  const scale=props.sourceSize.width/Math.max(1,props.viewSize.width);
  const scaled={...appearance,penWidth:appearance.penWidth*scale,shapeWidth:appearance.shapeWidth*scale,textFontSize:appearance.textFontSize*scale,mosaicBrushDiameter:appearance.mosaicBrushDiameter*scale};
  const selected=selection.mark;
  const editingTool=selected?.kind??tool;
  const definition=tools.find(item=>item.tool===editingTool)!;
  const shape=selected?.kind==="mosaic"?selected.shape??"brush":mosaicShape;
  const values:AppearanceSettings={...appearance,...(selected?.kind==="mosaic"?{
    mosaicBrushDiameter:selected.brushDiameter/scale,mosaicIntensity:selected.intensity,mosaicStyle:selected.style,
  }:selected?{colorPreset:selected.color,...(selected.kind==="text"?{textFontSize:selected.fontSize/scale,textBackgroundStyle:selected.background}:
    selected.kind==="pen"?{penWidth:selected.width/scale}:{shapeWidth:selected.width/scale})}:{})};
  function selectTool(next:Tool) {
    canvas.current?.finishAppearanceAdjustment();canvas.current?.commitTextEditing();
    if(next!=="select"){canvas.current?.clearSelection();props.onSelectionChange?.(null);setSelection({mark:null,editing:false});}
    setTool(next);props.onActivate();
  }
  function changeAppearance(patch:Partial<AppearanceSettings>,transient=false){
    const remembered={...patch};
    for(const [key,min,max] of [["penWidth",1,24],["shapeWidth",1,16],["textFontSize",12,64],["mosaicBrushDiameter",12,120]] as const){
      if(remembered[key]!==undefined)remembered[key]=Math.max(min,Math.min(max,Math.round(remembered[key])));
    }
    setAppearance({...appearance,...remembered});
    const scaledPatch={...patch};
    for(const key of ["penWidth","shapeWidth","textFontSize","mosaicBrushDiameter"] as const)if(scaledPatch[key]!==undefined)scaledPatch[key]*=scale;
    canvas.current?.updateSelectionAppearance(scaledPatch,transient);
  }
  useEffect(()=>{
    const key=(event:KeyboardEvent)=>{
      if(props.disabled||event.defaultPrevented)return;
      if(event.target instanceof HTMLElement&&event.target.closest("[role=listbox],[role=dialog]"))return;
      if(event.target instanceof HTMLElement&&/^(INPUT|TEXTAREA|SELECT)$/.test(event.target.tagName))return;
      if(!props.active){const entry=tools.find(item=>item.key===event.key.toLowerCase());if(entry&&!event.metaKey&&!event.ctrlKey&&!event.altKey){event.preventDefault();event.stopImmediatePropagation();selectTool(entry.tool);}return;}
      if((event.metaKey||event.ctrlKey)&&event.key.toLowerCase()==="z"){event.preventDefault();event.stopImmediatePropagation();canvas.current?.finishAppearanceAdjustment();event.shiftKey?canvas.current?.redo():canvas.current?.undo();return;}
      if(event.metaKey||event.ctrlKey||event.altKey)return;
      if(event.key==="Delete"||event.key==="Backspace"){event.preventDefault();event.stopImmediatePropagation();canvas.current?.deleteSelection();return;}
      if(event.key==="Escape"){
        event.preventDefault();event.stopImmediatePropagation();
        if(props.onCancelGesture?.())return;
        if(!canvas.current?.cancelInteraction()){canvas.current?.clearSelection();setTool("select");}
        return;
      }
      if(event.key==="Enter"&&selected?.kind==="text"){event.preventDefault();event.stopImmediatePropagation();canvas.current?.editSelectedText();return;}
      const entry=tools.find(item=>item.key===event.key.toLowerCase());
      if(entry){event.preventDefault();event.stopImmediatePropagation();selectTool(entry.tool);}
    };
    window.addEventListener("keydown",key,true);return()=>window.removeEventListener("keydown",key,true);
  },[props.disabled,props.active,props.onClose,props.onActivate,selected]);
  const sizeKey=editingTool==="text"?"textFontSize":editingTool==="mosaic"?"mosaicBrushDiameter":editingTool==="pen"?"penWidth":"shapeWidth";
  const sizeMin=editingTool==="text"||editingTool==="mosaic"?12:1;
  const sizeMax=editingTool==="text"?64:editingTool==="mosaic"?120:editingTool==="pen"?24:16;
  const sizeValue=Math.round(values[sizeKey]*10)/10;
  const controls=props.active&&<div className="kiri-video-annotation-appearance">
    <div className="kiri-video-tool-heading"><strong>{t(definition.name)}</strong>{selected&&<span>{t("Selected object")}</span>}</div>
    <p className="kiri-video-tool-hint">{t(selection.editing?"Enter finishes; Shift + Enter adds a line. Esc cancels this edit.":selected?(editingTool==="text"?"Drag to move or resize. Double-click to edit the text.":"Drag to move. Use the handles to change its shape."):definition.hint)}</p>
    {editingTool==="text"&&selected&&<button type="button" className="kiri-button kiri-button--secondary kiri-video-edit-text" onClick={()=>selection.editing?canvas.current?.commitTextEditing():canvas.current?.editSelectedText()}>{t(selection.editing?"Finish text":"Edit text")}</button>}
    {editingTool==="mosaic"&&<div className="kiri-video-property"><span>{t("Shape")}</span><ChoiceSelect label={t("Mosaic shape")} value={shape} disabled={props.disabled} onChange={next=>{setMosaicShape(next);canvas.current?.setMosaicShape(next);}} options={[
      {value:"brush",label:t("Freehand"),description:t("Brush over private details")},
      {value:"rectangle",label:t("Rectangle"),description:t("Cover a rectangular area")},
      {value:"ellipse",label:t("Ellipse"),description:t("Cover a rounded area")},
    ]}/></div>}
    {editingTool!=="select"&&(editingTool!=="mosaic"||shape==="brush")&&<VideoEffectSlider label={t(editingTool==="text"?"Font":editingTool==="mosaic"||editingTool==="pen"?"Brush":"Line")} min={Math.min(sizeMin,sizeValue)} max={Math.max(sizeMax,sizeValue)} step={1} value={sizeValue} text={String(sizeValue)} disabled={props.disabled} onChange={(value,transient)=>changeAppearance({[sizeKey]:value},transient)}/>}
    {editingTool==="text"&&<div className="kiri-video-property"><span>{t("Text background")}</span><ChoiceSelect label={t("Text background")} disabled={props.disabled} value={values.textBackgroundStyle} onChange={textBackgroundStyle=>changeAppearance({textBackgroundStyle})} options={[{value:"transparent",label:t("Transparent"),description:t("No background")},{value:"dark",label:t("Dark"),description:t("Dark background")}]} /></div>}
    {editingTool==="mosaic"?<>
      <div className="kiri-video-property"><span>{t("Style")}</span><ChoiceSelect label={t("Mosaic")} disabled={props.disabled} value={values.mosaicStyle} onChange={mosaicStyle=>changeAppearance({mosaicStyle})} options={[{value:"pixel",label:t("Pixel"),description:t("Pixel mosaic")},{value:"blur",label:t("Blur"),description:t("Gaussian blur")}]} /></div>
      <div className="kiri-video-property"><span>{t("Intensity")}</span><ChoiceSelect label={t("Intensity")} disabled={props.disabled} value={values.mosaicIntensity} onChange={mosaicIntensity=>changeAppearance({mosaicIntensity})} options={[{value:"soft",label:t("Soft"),description:t("Soft")},{value:"standard",label:t("Standard"),description:t("Standard")},{value:"strong",label:t("Strong"),description:t("Strong")}]} /></div>
    </>:editingTool!=="select"&&<div className="kiri-video-annotation-colors" role="group" aria-label={t("Color")}>{COLOR_PRESETS.map(color=><button type="button" className="kiri-video-annotation-swatch" key={color} style={{background:COLOR_HEX[color]}} title={t(COLOR_LABELS[color])} aria-label={t(COLOR_LABELS[color])} aria-pressed={values.colorPreset===color} onClick={()=>changeAppearance({colorPreset:color})}/>)}</div>}
  </div>;
  const toolbar=<fieldset disabled={props.disabled} className={`kiri-video-annotation-tools${props.toolbarHost?" kiri-video-annotation-tools--hosted":""}`} aria-label={t("Annotations")}>
    {tools.map(item=><button type="button" key={item.tool} className="kiri-video-annotation-tool" title={t(item.label)} aria-label={t(item.label)} aria-pressed={props.active&&tool===item.tool} onClick={()=>selectTool(item.tool)}><KiriIcon name={item.icon} size={17}/></button>)}
    {props.extraTools}
    {!props.appearanceHost&&controls}
  </fieldset>;
  return <div className="kiri-video-annotations-editor">
    {props.toolbarHost?createPortal(toolbar,props.toolbarHost):toolbar}
    {props.appearanceHost&&createPortal(controls,props.appearanceHost)}
    {props.active&&<div className="kiri-video-annotation-surface" style={{width:props.viewSize.width,height:props.viewSize.height}}>
      <AnnotationCanvas ref={canvas} onLiveMarks={props.onLiveMarks} onFrame={props.onFrame} image={props.image} region={{x:0,y:0,...props.sourceSize}} viewSize={props.viewSize}
        initialDocument={{schemaVersion:1,canvas:props.sourceSize,sourcePixels:props.sourceSize,marks:props.marks}} documentRevision={props.revision} selectedMarkId={props.selectedMarkId} onSelectionChange={props.onSelectionChange}
        onSelectionInfo={receiveSelection} onMarkCreated={created} mosaicShape={mosaicShape} textEscapeCancelsEdit commitTextOnToolChange={false}
        interactionDisabled={props.disabled} tool={tool} appearance={scaled} onHistoryChange={noop} onDocumentChange={props.onChange} onUndo={props.onUndo} onRedo={props.onRedo} onCancel={props.onClose}/>
    </div>}
  </div>;
}
