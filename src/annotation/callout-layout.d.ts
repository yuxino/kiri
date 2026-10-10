import type {Rect} from "./geom";

export interface CalloutLabelSize {
  width: number;
  height: number;
  lineCount: number;
}

export function calloutLabelSize(options: {
  text: string;
  fontSize: number;
  boundsWidth: number;
  measureText(text: string): number;
  uiScale?: number;
  /** Existing saved width, which is kept when repairing only height. */
  width?: number;
}): CalloutLabelSize;

export function repairCalloutLabelHeight(rect: Rect, size: CalloutLabelSize, boundsHeight: number): Rect;
