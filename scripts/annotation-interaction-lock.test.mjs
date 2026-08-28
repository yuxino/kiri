import assert from "node:assert/strict";
import test from "node:test";

import { AnnotationInteractionLock } from "../src/annotation/interaction-lock.js";

test("annotation completion holds one interaction lock until the action settles", () => {
  const lock = new AnnotationInteractionLock();

  assert.equal(lock.locked, false);
  assert.equal(lock.acquire(), true);
  assert.equal(lock.locked, true);
  assert.equal(lock.acquire(), false, "a second completion cannot start while export is pending");

  lock.release();
  assert.equal(lock.locked, false);
  assert.equal(lock.acquire(), true, "failed or cancelled actions can be retried after release");
});
