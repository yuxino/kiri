interface CompositionKey {
  isComposing?: boolean;
  keyCode?: number;
  target?: EventTarget | null;
  nativeEvent?: { isComposing?: boolean; keyCode?: number; target?: EventTarget | null };
}
interface TextKey extends CompositionKey {
  key: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
  shiftKey?: boolean;
  preventDefault(): void;
  stopPropagation(): void;
}
export function setTextComposition(target: EventTarget, active: boolean): void;
export function isTextComposition(event: CompositionKey): boolean;
export function handleTextEditorKey(event: TextKey, actions: {
  cancel(): void; commit(): void; undo(): void; redo(): void; finish?(): void;
}, nativeUndo?: boolean): void;
