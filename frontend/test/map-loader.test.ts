import assert from "node:assert/strict";
import test from "node:test";

import { createMapLoader } from "../src/map-loader.ts";

test("loads the public map lazily and caches it", async () => {
  let requests = 0;
  const load = createMapLoader("all", async () => {
    requests += 1;
    return "public";
  });

  assert.equal(await load(false), "all");
  assert.equal(requests, 0);
  assert.deepEqual(await Promise.all([load(true), load(true)]), ["public", "public"]);
  assert.equal(await load(true), "public");
  assert.equal(requests, 1);
});

test("retries a failed public-map request", async () => {
  let requests = 0;
  const load = createMapLoader("all", async () => {
    requests += 1;
    if (requests === 1) throw new Error("failed");
    return "public";
  });

  await assert.rejects(load(true), /failed/);
  assert.equal(await load(true), "public");
  assert.equal(requests, 2);
});
