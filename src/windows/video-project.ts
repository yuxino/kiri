import type {AnnotationMark} from "../annotation/model";
import type {VideoEffect} from "./video-effects";
import type {VideoSticker} from "./video-stickers";
import type {VideoSegment} from "./video-trim.js";

export type VideoAnnotationTrack = {id:string;start:number;end:number;mark:AnnotationMark;layer?:number};
export type VideoEdit = {segments:VideoSegment[];effects:VideoEffect[];annotations:VideoAnnotationTrack[];stickers:VideoSticker[]};
export type VideoProject = {
  schemaVersion:1;
  sourceSize:{width:number;height:number};
  sourceDuration:number;
  edit:VideoEdit;
  preset:"original"|"share"|"small";
  playhead:number;
};
export type VideoProjectSnapshot = {
  state:"none"|"valid"|"invalid";
  revision:string;
  project:VideoProject|null;
  reason?:"sourceChanged"|"invalidDocument";
};
export type VideoExportProgress = {requestId:string;phase:"preparing"|"rendering"|"saving";progress:number|null};

export function hasVideoEdits(edit:VideoEdit,duration:number) {
  const clip=edit.segments[0];
  return duration>0&&(edit.segments.length!==1||clip?.start!==0||Math.abs((clip?.end??0)-duration)>.001||(clip?.speed??1)!==1||edit.effects.length>0||edit.annotations.length>0||edit.stickers.length>0);
}
