export class AnnotationInteractionLock {
  readonly locked: boolean;
  acquire(): boolean;
  release(): void;
}
