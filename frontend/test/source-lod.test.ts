import assert from "node:assert/strict";
import test from "node:test";

import { sourceLodOpacities } from "../src/source-lod.ts";

test("selects a single LOD outside transition bands", () => {
  assert.deepEqual(sourceLodOpacities(0.24), { detailed: 1, medium: 0, coarse: 0 });
  assert.deepEqual(sourceLodOpacities(0.12), { detailed: 0, medium: 1, coarse: 0 });
  assert.deepEqual(sourceLodOpacities(0.05), { detailed: 0, medium: 0, coarse: 1 });
});

test("cross-fades with complementary opacity", () => {
  const detailedTransition = sourceLodOpacities(0.2);
  const coarseTransition = sourceLodOpacities(0.07);

  assert.ok(Math.abs(detailedTransition.detailed - 0.5) < 1e-10);
  assert.ok(Math.abs(detailedTransition.medium - 0.5) < 1e-10);
  assert.equal(detailedTransition.detailed + detailedTransition.medium, 1);
  assert.ok(Math.abs(coarseTransition.medium - 0.5) < 1e-10);
  assert.ok(Math.abs(coarseTransition.coarse - 0.5) < 1e-10);
  assert.equal(coarseTransition.medium + coarseTransition.coarse, 1);
});
