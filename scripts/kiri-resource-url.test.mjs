import assert from "node:assert/strict";
import test, { afterEach, beforeEach } from "node:test";

import { kiriResourceUrl } from "../src/lib/kiri-resource-url.js";

let calls;

beforeEach(() => {
  calls = [];
  globalThis.window = {
    __TAURI_INTERNALS__: {
      convertFileSrc(route, protocol) {
        calls.push({ route, protocol });
        return `http://${protocol}.localhost/${encodeURIComponent(route)}`;
      },
    },
  };
});

afterEach(() => {
  delete globalThis.window;
});

test("converts each complete non-empty private resource route", () => {
  const cases = [
    ["capture", ["frozen", "abc.png"]],
    ["thumbnail", ["00000000-0000-4000-8000-000000000000"]],
    ["media", ["00000000-0000-4000-8000-000000000000"]],
    ["annotation-source", ["00000000-0000-4000-8000-000000000000"]],
    ["asset", ["00000000-0000-4000-8000-000000000000"]],
  ];

  for (const [route, segments] of cases) {
    const joined = [route, ...segments].join("/");
    assert.equal(
      kiriResourceUrl(route, segments),
      `http://kiri.localhost/${encodeURIComponent(joined)}`,
    );
  }
  assert.deepEqual(
    calls,
    cases.map(([route, segments]) => ({
      route: [route, ...segments].join("/"),
      protocol: "kiri",
    })),
  );

  calls = [];
  window.__TAURI_INTERNALS__.convertFileSrc = (route, protocol) => {
    calls.push({ route, protocol });
    return `${protocol}://localhost/${encodeURIComponent(route)}`;
  };

  for (const [route, segments] of cases) {
    const joined = [route, ...segments].join("/");
    assert.equal(
      kiriResourceUrl(route, segments),
      `kiri://localhost/${encodeURIComponent(joined)}`,
    );
  }
  assert.deepEqual(
    calls,
    cases.map(([route, segments]) => ({
      route: [route, ...segments].join("/"),
      protocol: "kiri",
    })),
  );
});

test("appends encoded query parameters after route conversion", () => {
  const revision = "a".repeat(64);
  const joined = "annotation-source/00000000-0000-4000-8000-000000000000";
  assert.equal(
    kiriResourceUrl(
      "annotation-source",
      ["00000000-0000-4000-8000-000000000000"],
      { revision, page: 2 },
    ),
    `http://kiri.localhost/${encodeURIComponent(joined)}?revision=${revision}&page=2`,
  );
  assert.equal(calls.length, 1);
});

test("rejects unknown routes and path or query injection", () => {
  for (const operation of [
    () => kiriResourceUrl("unknown", ["id"]),
    () => kiriResourceUrl("media", [""]),
    () => kiriResourceUrl("media", ["."]),
    () => kiriResourceUrl("media", [".."]),
    () => kiriResourceUrl("media", ["nested/id"]),
    () => kiriResourceUrl("media", ["id\\other"]),
    () => kiriResourceUrl("media", ["id?other"]),
    () => kiriResourceUrl("media", ["id#other"]),
    () => kiriResourceUrl("media", ["id"], { "bad/key": "value" }),
  ]) {
    assert.throws(operation, TypeError);
  }
  assert.equal(calls.length, 0);
});
