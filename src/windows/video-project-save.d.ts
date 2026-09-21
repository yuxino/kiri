import type {VideoProject,VideoProjectSnapshot} from "./video-project";
export type VideoProjectSaveState={status:"idle"|"waiting"|"saving"|"saved"|"error";error:string|null};
export function sameVideoProjectValue(a:unknown,b:unknown):boolean;
export interface VideoProjectSaveQueue {
  enqueue(project:VideoProject):void;
  flush():Promise<boolean>;
  discardUnwritten():boolean;
  getState():VideoProjectSaveState&{dirty:boolean;revision:string};
  dispose():void;
}
export function createVideoProjectSaveQueue(options:{revision:string;project?:VideoProject|null;save(revision:string,project:VideoProject):Promise<VideoProjectSnapshot>;onState?(state:VideoProjectSaveState):void;delay?:number}):VideoProjectSaveQueue;
