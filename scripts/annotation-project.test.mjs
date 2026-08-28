import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";

import {
  ANNOTATION_PROJECT_LIMITS,
  MAX_ANNOTATION_DOCUMENT_BYTES,
  annotationSourceCrop,
  documentUnitsPerViewPixel,
  parseAnnotationDocument,
  viewPointToDocument,
} from "../src/annotation/project.js";

const TRANSPILE_OPTIONS = {
  compilerOptions: {
    module: ts.ModuleKind.ESNext,
    target: ts.ScriptTarget.ES2022,
  },
};

function moduleDataUrl(source) {
  return `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
}

async function loadAnnotationModel() {
  const [geomSource, modelSource] = await Promise.all([
    readFile(new URL("../src/annotation/geom.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/annotation/model.ts", import.meta.url), "utf8"),
  ]);
  const geomJavaScript = ts.transpileModule(geomSource, TRANSPILE_OPTIONS).outputText;
  const geomUrl = moduleDataUrl(geomJavaScript);
  const modelJavaScript = ts.transpileModule(
    modelSource.replaceAll('"./geom"', JSON.stringify(geomUrl)),
    TRANSPILE_OPTIONS,
  ).outputText;
  return import(moduleDataUrl(modelJavaScript));
}

async function loadAnnotationRender() {
  const [geomSource, modelSource, renderSource] = await Promise.all([
    readFile(new URL("../src/annotation/geom.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/annotation/model.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/annotation/render.ts", import.meta.url), "utf8"),
  ]);
  const geomUrl = moduleDataUrl(ts.transpileModule(geomSource, TRANSPILE_OPTIONS).outputText);
  const modelJavaScript = ts.transpileModule(
    modelSource.replaceAll('"./geom"', JSON.stringify(geomUrl)),
    TRANSPILE_OPTIONS,
  ).outputText;
  const modelUrl = moduleDataUrl(modelJavaScript);
  const textLayoutUrl = new URL("../src/annotation/text-layout.js", import.meta.url).href;
  const renderJavaScript = ts.transpileModule(
    renderSource
      .replaceAll('"./model"', JSON.stringify(modelUrl))
      .replaceAll('"./geom"', JSON.stringify(geomUrl))
      .replaceAll('"./text-layout.js"', JSON.stringify(textLayoutUrl)),
    TRANSPILE_OPTIONS,
  ).outputText;
  return import(moduleDataUrl(renderJavaScript));
}

function documentWith(marks = []) {
  return {
    schemaVersion: 1,
    canvas: { width: 640, height: 360 },
    sourcePixels: { width: 1280, height: 720 },
    marks,
  };
}

const ALL_MARKS = [
  {
    kind: "pen",
    id: 1.25,
    points: [{ x: 1, y: 2 }, { x: 3, y: 4 }],
    color: "violet",
    width: 3,
  },
  {
    kind: "rectangle",
    id: 2.25,
    rect: { x: 10, y: 11, width: 120, height: 80 },
    color: "cherry",
    width: 4,
  },
  {
    kind: "line",
    id: 3.25,
    start: { x: 20, y: 21 },
    end: { x: 220, y: 121 },
    color: "orange",
    width: 5,
  },
  {
    kind: "arrow",
    id: 4.25,
    start: { x: 30, y: 31 },
    end: { x: 230, y: 131 },
    color: "yellow",
    width: 6,
  },
  {
    kind: "text",
    id: 5.25,
    text: "editable text\n第二行",
    rect: { x: 40, y: 41, width: 180, height: 60 },
    color: "white",
    background: "transparent",
    fontSize: 18,
  },
  {
    kind: "mosaic",
    id: 6.25,
    points: [{ x: 50, y: 51 }],
    brushDiameter: 20,
    intensity: "standard",
    style: "pixel",
  },
];

test("annotation documents round-trip every mark kind without changing IDs or order", () => {
  const input = documentWith(structuredClone(ALL_MARKS));
  const parsed = parseAnnotationDocument(input);

  assert.deepEqual(parsed, input);
  assert.deepEqual(parsed.marks.map((mark) => mark.kind), ALL_MARKS.map((mark) => mark.kind));
  assert.deepEqual(parsed.marks.map((mark) => mark.id), ALL_MARKS.map((mark) => mark.id));

  input.marks[0].points[0].x = 999;
  assert.equal(parsed.marks[0].points[0].x, 1, "the validated document must be detached");
});

test("annotation document validation is strict about schema, dimensions, numbers, and enums", () => {
  assert.throws(
    () => parseAnnotationDocument({ ...documentWith(), schemaVersion: 2 }),
    /schemaVersion/,
  );
  assert.throws(
    () => parseAnnotationDocument({ ...documentWith(), unexpected: true }),
    /unexpected/,
  );
  assert.throws(
    () => parseAnnotationDocument({ ...documentWith(), canvas: { width: 0, height: 360 } }),
    /canvas.width/,
  );
  assert.throws(
    () =>
      parseAnnotationDocument({
        ...documentWith(),
        sourcePixels: { width: 1280.5, height: 720 },
      }),
    /sourcePixels.width/,
  );
  assert.throws(
    () => parseAnnotationDocument(documentWith([{ ...ALL_MARKS[1], id: Number.NaN }])),
    /marks\[0\]\.id/,
  );
  assert.throws(
    () => parseAnnotationDocument(documentWith([{ ...ALL_MARKS[0], color: "green" }])),
    /marks\[0\]\.color/,
  );
  assert.throws(
    () =>
      parseAnnotationDocument(
        documentWith([{ ...ALL_MARKS[2], start: { x: Number.POSITIVE_INFINITY, y: 1 } }]),
      ),
    /marks\[0\]\.start\.x/,
  );
  assert.throws(
    () => parseAnnotationDocument(documentWith([{ ...ALL_MARKS[4], background: "light" }])),
    /marks\[0\]\.background/,
  );
  assert.throws(
    () => parseAnnotationDocument(documentWith([{ ...ALL_MARKS[5], intensity: "extreme" }])),
    /marks\[0\]\.intensity/,
  );
  assert.throws(
    () => parseAnnotationDocument(documentWith([{ ...ALL_MARKS[5], style: "smear" }])),
    /marks\[0\]\.style/,
  );
  assert.throws(
    () => parseAnnotationDocument(documentWith([{ ...ALL_MARKS[1], kind: "ellipse" }])),
    /marks\[0\]\.kind/,
  );
});

test("annotation document validation bounds aggregate data and rejects duplicate IDs", () => {
  assert.throws(
    () => parseAnnotationDocument(documentWith([ALL_MARKS[0], { ...ALL_MARKS[1], id: 1.25 }])),
    /duplicate/,
  );

  const tooManyMarks = Array.from(
    { length: ANNOTATION_PROJECT_LIMITS.maxMarks + 1 },
    (_, index) => ({ ...ALL_MARKS[1], id: index }),
  );
  assert.throws(() => parseAnnotationDocument(documentWith(tooManyMarks)), /marks/);

  const tooManyPoints = Array.from(
    { length: ANNOTATION_PROJECT_LIMITS.maxTotalPoints + 1 },
    () => ({ x: 1, y: 1 }),
  );
  assert.throws(
    () =>
      parseAnnotationDocument(
        documentWith([{ ...ALL_MARKS[5], id: 99, points: tooManyPoints }]),
      ),
    /points/,
  );

  assert.throws(
    () =>
      parseAnnotationDocument(
        documentWith([
          {
            ...ALL_MARKS[4],
            id: 100,
            text: "x".repeat(ANNOTATION_PROJECT_LIMITS.maxTotalText + 1),
          },
        ]),
      ),
    /text/,
  );
});

test("annotation document UTF-8 size matches the Rust four MiB boundary", () => {
  const verboseCoordinate = 0.12345678901234568;
  const points = Array.from({ length: ANNOTATION_PROJECT_LIMITS.maxTotalPoints }, (_, index) => ({
    x: verboseCoordinate,
    y: index < 55_000 ? verboseCoordinate : 0,
  }));
  const exactLimit = documentWith([
    {
      ...ALL_MARKS[5],
      id: 200,
      points,
    },
    {
      ...ALL_MARKS[4],
      id: 201,
      text: "",
    },
  ]);
  const remainingBytes =
    MAX_ANNOTATION_DOCUMENT_BYTES - Buffer.byteLength(JSON.stringify(exactLimit));
  assert.ok(remainingBytes > 0 && remainingBytes <= ANNOTATION_PROJECT_LIMITS.maxTotalText);
  exactLimit.marks[1].text = "x".repeat(remainingBytes);

  assert.equal(Buffer.byteLength(JSON.stringify(exactLimit)), MAX_ANNOTATION_DOCUMENT_BYTES);
  assert.equal(
    parseAnnotationDocument(exactLimit).marks[0].points.length,
    ANNOTATION_PROJECT_LIMITS.maxTotalPoints,
  );

  exactLimit.marks[1].text += "x";
  assert.equal(Buffer.byteLength(JSON.stringify(exactLimit)), MAX_ANNOTATION_DOCUMENT_BYTES + 1);
  assert.throws(() => parseAnnotationDocument(exactLimit), /UTF-8 size limit/);

  for (const point of points) point.y = verboseCoordinate;
  exactLimit.marks[1].text = "";
  assert.ok(Buffer.byteLength(JSON.stringify(exactLimit)) > MAX_ANNOTATION_DOCUMENT_BYTES);
  assert.throws(() => parseAnnotationDocument(exactLimit), /UTF-8 size limit/);
});

test("live history previews commit the original element as the undo baseline", async () => {
  const { AnnotationHistory } = await loadAnnotationModel();
  const original = structuredClone(ALL_MARKS[4]);
  const resized = { ...original, fontSize: 30 };
  const history = new AnnotationHistory([original]);

  history.overwrite([resized]);
  assert.equal(history.canUndo, false, "a live preview is not independently undoable");
  history.commitOverwrite(0, original);

  assert.equal(history.canUndo, true);
  history.undo();
  assert.equal(history.elements[0].fontSize, original.fontSize);
  history.redo();
  assert.equal(history.elements[0].fontSize, resized.fontSize);
});

test("text commit emptiness checks preserve meaningful leading and trailing whitespace", async () => {
  const { annotationTextForCommit } = await loadAnnotationModel();
  const original = "  Kiri\nnext line  ";

  assert.equal(annotationTextForCommit(original), original);
  assert.equal(annotationTextForCommit(" \n\t "), null);
});

test("view coordinates project into a fixed document space without viewport drift", () => {
  const canvas = { width: 640, height: 360 };
  const firstViewport = { width: 960, height: 540 };
  const secondViewport = { width: 320, height: 180 };

  assert.deepEqual(viewPointToDocument({ x: 480, y: 270 }, firstViewport, canvas), {
    x: 320,
    y: 180,
  });
  assert.deepEqual(viewPointToDocument({ x: 160, y: 90 }, secondViewport, canvas), {
    x: 320,
    y: 180,
  });
  assert.throws(
    () => viewPointToDocument({ x: 1, y: 1 }, { width: 0, height: 10 }, canvas),
    /viewSize.width/,
  );
});

test("CSS-sized interaction targets expand in document space when the viewport shrinks", () => {
  const units = documentUnitsPerViewPixel(
    { width: 800, height: 450 },
    { width: 1600, height: 900 },
  );

  assert.deepEqual(units, { x: 2, y: 2, radial: 2 });
  assert.equal(5 * units.radial * (800 / 1600), 5, "a 5px handle stays 5 CSS px");
  assert.throws(
    () => documentUnitsPerViewPixel({ width: 0, height: 450 }, { width: 1600, height: 900 }),
    /viewSize.width/,
  );
});

test("fractional display selections use the same rounded integer source crop as persistence", () => {
  assert.deepEqual(
    annotationSourceCrop(
      { width: 8, height: 6 },
      { width: 4, height: 3 },
      { x: 0.25, y: 0.5, width: 2, height: 1.5 },
      { width: 4, height: 3 },
    ),
    { x: 1, y: 1, width: 4, height: 3 },
  );
  assert.deepEqual(
    annotationSourceCrop(
      { width: 8, height: 6 },
      { width: 4, height: 3 },
      { x: 3.75, y: 2.75, width: 0.25, height: 0.25 },
      { width: 1, height: 1 },
    ),
    { x: 7, y: 5, width: 1, height: 1 },
  );
  assert.throws(
    () =>
      annotationSourceCrop(
        { width: 8, height: 6 },
        { width: 4, height: 3 },
        { x: 0, y: 0, width: 2, height: 1.5 },
        { width: 3, height: 3 },
      ),
    /outputSize/,
  );
});

test("non-export rendering keeps document and CSS geometry unchanged", async () => {
  const { mosaicBlurRadius, renderGeometryScale, scaleRectForRender } =
    await loadAnnotationRender();
  const scale = renderGeometryScale(false, 4, 2);

  assert.deepEqual(scale, { x: 1, y: 1, stroke: 1 });
  assert.deepEqual(
    scaleRectForRender({ x: 10, y: 5, width: 30, height: 12 }, scale),
    { x: 10, y: 5, width: 30, height: 12 },
  );
  assert.equal(mosaicBlurRadius(20, "standard", scale), 5);
});

test("nonuniform export scales directional geometry on its own axis", async () => {
  const { renderGeometryScale, scaleRectForRender } = await loadAnnotationRender();
  const scale = renderGeometryScale(true, 4, 2);

  assert.deepEqual(scale, { x: 4, y: 2, stroke: 2 });
  assert.deepEqual(
    scaleRectForRender({ x: 10, y: 5, width: 30, height: 12 }, scale),
    { x: 40, y: 10, width: 120, height: 24 },
  );
});

test("blur mosaic export maps its document radius through the stroke scale", async () => {
  const { mosaicBlurRadius, renderGeometryScale } = await loadAnnotationRender();
  const scale = renderGeometryScale(true, 4, 2);

  assert.equal(mosaicBlurRadius(20, "soft", scale), 8);
  assert.equal(mosaicBlurRadius(20, "standard", scale), 10);
  assert.equal(mosaicBlurRadius(20, "strong", scale), 14);
});
