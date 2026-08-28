export interface TextFrame {
  x: number;
  y: number;
  width: number;
  height: number;
}

type MeasureText = (text: string) => number;

export function layoutTextLines(
  text: string,
  maxWidth: number,
  measureText: MeasureText,
): string[];

export function fitTextEditorFrame(options: {
  text: string;
  fontSize: number;
  x: number;
  y: number;
  maxWidth: number;
  boundsWidth: number;
  boundsHeight: number;
  measureText: MeasureText;
}): TextFrame;
