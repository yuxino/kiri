/**
 * Small synchronous gate shared by capture and editor completion flows.
 * Acquiring before the first await closes the event-loop window in which a
 * second completion or annotation mutation could otherwise start.
 */
export class AnnotationInteractionLock {
  #locked = false;

  get locked() {
    return this.#locked;
  }

  acquire() {
    if (this.#locked) return false;
    this.#locked = true;
    return true;
  }

  release() {
    this.#locked = false;
  }
}
