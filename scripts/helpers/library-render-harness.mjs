import { readFileSync } from "node:fs";
import ts from "typescript";
import * as cardInteraction from "../../src/windows/library-card-interaction.js";

// Exercise the real component handlers without a WebView, native IPC, or a
// user's library. This models hook state/effect cleanup, not DOM or layout.
const source = readFileSync(new URL("../../src/windows/LibraryWindow.tsx", import.meta.url), "utf8");
const compiled = ts.transpileModule(`${source}\nexport { AssetCard };`, {
  compilerOptions: {
    target: ts.ScriptTarget.ES2021,
    jsx: ts.JsxEmit.React,
    module: ts.ModuleKind.CommonJS,
    esModuleInterop: true,
  },
}).outputText;

export const testAsset = {
  id: "00000000-0000-4000-8000-000000000001",
  kind: "image",
  title: null,
  filename: "test.png",
  pixelWidth: 20,
  pixelHeight: 20,
  createdAt: 0,
  tags: [],
  isFavorite: false,
  gifEligible: false,
  duration: null,
};

export function createLibraryHarness(apiOverrides = {}) {
  let active;
  const listeners = new Map();
  const events = new Map();
  const sameDeps = (left, right) => left && right && left.length === right.length &&
    left.every((value, index) => Object.is(value, right[index]));
  const React = {
    lazy: () => () => null,
    createElement: (type, props, ...children) => ({ type, props: { ...props, children } }),
    useState(initial) {
      const owner = active;
      const index = owner.cursor++;
      const hook = owner.hooks[index] ??= {
        value: typeof initial === "function" ? initial() : initial,
      };
      return [hook.value, (next) => {
        if (owner.unmounted) return;
        const value = typeof next === "function" ? next(hook.value) : next;
        if (!Object.is(value, hook.value)) owner.dirty = true;
        hook.value = value;
      }];
    },
    useRef(initial) {
      return React.useState(() => ({ current: initial }))[0];
    },
    useMemo(create, deps) {
      const index = active.cursor++;
      if (!sameDeps(active.hooks[index]?.deps, deps)) {
        active.hooks[index] = { value: create(), deps };
      }
      return active.hooks[index].value;
    },
    useCallback(callback, deps) {
      return React.useMemo(() => callback, deps);
    },
    useEffect(create, deps) {
      const owner = active;
      const index = owner.cursor++;
      if (sameDeps(owner.hooks[index]?.deps, deps)) return;
      owner.effects.push(() => {
        owner.hooks[index]?.cleanup?.();
        owner.hooks[index] = { deps, cleanup: create() };
      });
    },
  };
  const window = {
    addEventListener(name, callback) {
      if (!listeners.has(name)) listeners.set(name, new Set());
      listeners.get(name).add(callback);
    },
    removeEventListener(name, callback) { listeners.get(name)?.delete(callback); },
    dispatchEvent(event) { listeners.get(event.type)?.forEach((callback) => callback(event)); },
  };
  const subscribe = (name) => (callback) => {
    events.set(name, callback);
    return Promise.resolve(() => events.delete(name));
  };
  const modules = {
    react: React,
    "react-dom": { createPortal: (child) => child },
    "../lib/ipc": {
      api: {
        getAssetAvailability: async () => ({ status: "ready" }),
        getLibraryStatus: async () => ({ availability: "ready" }),
        listAssets: async () => [testAsset],
        listPendingRecordings: async () => [],
        getShortcutStatus: async () => ({ status: "enabled", label: "shortcut" }),
        ...apiOverrides,
      },
      mediaUrl: (id) => `media:${id}`,
      onLibraryChanged: subscribe("libraryChanged"),
      onAssetContentChanged: subscribe("assetContentChanged"),
      onGifConversionState: subscribe("gifConversionState"),
      onNotice: subscribe("notice"),
      onError: subscribe("error"),
    },
    "../i18n": { t: (value) => value, fmt: (value) => value },
    "../../src-tauri/icons/128x128.png": "",
    "../components/KiriIcons": { KiriIcon: "icon" },
    "../lib/kiri-resource-url.js": {
      kiriResourceUrl: (route, [id], { v }) => `${route}:${id}?v=${v}`,
    },
    "./library-card-interaction.js": cardInteraction,
  };
  const module = { exports: {} };
  new Function("require", "module", "exports", "window", compiled)((name) => {
    if (!(name in modules)) throw new Error(`Unexpected import: ${name}`);
    return modules[name];
  }, module, module.exports, window);

  return {
    window,
    emit: (name, payload) => events.get(name)?.(payload),
    mount(name, initialProps) {
      const owner = { hooks: [], cursor: 0, effects: [], dirty: false, unmounted: false };
      let props = initialProps;
      return {
        render(nextProps = props) {
          props = nextProps;
          for (let pass = 0; pass < 30; pass++) {
            active = owner;
            owner.cursor = 0;
            owner.dirty = false;
            const tree = module.exports[name](props);
            const effects = owner.effects.splice(0);
            effects.forEach((effect) => effect());
            if (!owner.dirty) return tree;
          }
          throw new Error("Component did not settle");
        },
        unmount() {
          owner.unmounted = true;
          owner.hooks.forEach((hook) => hook?.cleanup?.());
        },
      };
    },
  };
}

export function nodes(root) {
  if (root == null || typeof root === "boolean") return [];
  if (Array.isArray(root)) return root.flatMap(nodes);
  if (typeof root !== "object") return [root];
  return [root, ...nodes(root.props?.children)];
}

export function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((accept, fail) => { resolve = accept; reject = fail; });
  return { promise, resolve, reject };
}

export async function settleRequests() {
  for (let turn = 0; turn < 8; turn++) await Promise.resolve();
}
