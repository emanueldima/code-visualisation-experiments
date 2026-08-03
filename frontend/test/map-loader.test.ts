import assert from "node:assert/strict";
import test from "node:test";

import { createCachedLoader, createMapLoader } from "../src/map-loader.ts";

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

test("caches keyed maps and retries only a rejected key", async () => {
  const requests: string[] = [];
  let fail = true;
  const load = createCachedLoader(async (key) => {
    requests.push(key);
    if (key === "public:resolved" && fail) {
      fail = false;
      throw new Error("failed");
    }
    return key;
  });

  assert.deepEqual(await Promise.all([load("all:resolved"), load("all:resolved")]), [
    "all:resolved",
    "all:resolved",
  ]);
  await assert.rejects(load("public:resolved"), /failed/);
  assert.equal(await load("public:resolved"), "public:resolved");
  assert.deepEqual(requests, ["all:resolved", "public:resolved", "public:resolved"]);
});

test("seeds a keyed map without loading it again", async () => {
  let requests = 0;
  const load = createCachedLoader(
    async (key) => {
      requests += 1;
      return key;
    },
    ["all:hidden", "initial"],
  );

  assert.equal(await load("all:hidden"), "initial");
  assert.equal(requests, 0);
  assert.equal(await load("all:shown"), "all:shown");
  assert.equal(requests, 1);
});
