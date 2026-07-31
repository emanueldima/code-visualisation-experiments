import assert from "node:assert/strict";
import test from "node:test";

import { calculatePopupPosition, createSourceResolver } from "../src/source-preview.ts";

test("resolves structure ranges from source-mode file text", () => {
  const fileSource = "\nimpl A {\nfn α() {}\n}\n";
  const encoded = new TextEncoder().encode(fileSource);
  const start = new TextEncoder().encode("\n").length;
  const end = encoded.length - new TextEncoder().encode("\n").length;
  const sourceForId = createSourceResolver(
    {
      breadcrumbs: [{ id: 0, path: "" }],
      children: [
        {
          breadcrumbs: [{ id: 1, path: "src/main.rs#1-24" }],
          start_byte: start,
          end_byte: end,
          children: [],
        },
      ],
    },
    {
      breadcrumbs: [{ id: 0, path: "" }],
      children: [
        {
          breadcrumbs: [{ id: 4, path: "src/main.rs" }],
          flow: { source: fileSource },
          children: [],
        },
      ],
    },
  );

  assert.equal(sourceForId("1"), "impl A {\nfn α() {}\n}");
  assert.equal(sourceForId("missing"), undefined);
});

test("uses the last hash in paths when resolving source", () => {
  const sourceForId = createSourceResolver(
    {
      breadcrumbs: [{ id: 0, path: "" }],
      start_byte: 0,
      end_byte: 2,
      children: [
        {
          breadcrumbs: [{ id: 2, path: "src/#generated.rs#0-2" }],
          start_byte: 0,
          end_byte: 2,
          children: [],
        },
      ],
    },
    {
      breadcrumbs: [{ id: 0, path: "" }],
      children: [
        {
          breadcrumbs: [{ id: 3, path: "src/#generated.rs" }],
          flow: { source: "fn" },
          children: [],
        },
      ],
    },
  );

  assert.equal(sourceForId("2"), "fn");
});

test("places the popup beside the anchor and flips at the right edge", () => {
  assert.deepEqual(
    calculatePopupPosition(
      { top: 40, right: 100, left: 60 },
      { width: 200, height: 100 },
      { width: 800, height: 600 },
    ),
    { left: 108, top: 40 },
  );
  assert.deepEqual(
    calculatePopupPosition(
      { top: 550, right: 780, left: 700 },
      { width: 200, height: 100 },
      { width: 800, height: 600 },
    ),
    { left: 492, top: 488 },
  );
});
