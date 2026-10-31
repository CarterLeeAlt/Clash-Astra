import assert from "node:assert/strict";
import { test } from "node:test";
import { LatestResource } from "../../src/shared/async/latest-resource.ts";
import { FallbackPoller } from "../../src/shared/async/fallback-poller.ts";
import { RequestEpoch } from "../../src/shared/async/request-epoch.ts";
import {
  acceptSiteTestVersion,
  setSiteTestRuntime,
} from "../../src/features/test/site-test-results.ts";
import { mergeConnectionSnapshot } from "../../src/features/connections/model.ts";
import { normalizeTestList } from "../../src/features/test/normalize.ts";

const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};

test("initialization is single-flight and a failure remains retryable", async () => {
  let calls = 0;
  const first = deferred();
  const resource = new LatestResource(() =>
    ++calls === 1 ? first.promise : Promise.resolve("ready")
  );
  const left = resource.get(),
    right = resource.get();
  assert.equal(calls, 1);
  first.reject(new Error("not ready"));
  await assert.rejects(left, /not ready/);
  await assert.rejects(right, /not ready/);
  assert.equal(await resource.get(), "ready");
});

test("late initialization cannot replace the current controller", async () => {
  const old = deferred(),
    next = deferred();
  let calls = 0;
  const resource = new LatestResource(() =>
    ++calls === 1 ? old.promise : next.promise
  );
  const pendingOld = resource.get();
  resource.invalidate();
  const pendingNext = resource.get();
  next.resolve("new controller");
  old.resolve("old controller");
  assert.equal(await pendingOld, "new controller");
  assert.equal(await pendingNext, "new controller");
  assert.equal(await resource.get(), "new controller");
});

test("WS recovery stops an in-flight fallback from scheduling another poll", async (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const request = deferred();
  let calls = 0,
    accepted = 0;
  const poller = new FallbackPoller(async (isCurrent) => {
    calls++;
    await request.promise;
    if (isCurrent()) accepted++;
  }, 3000);
  poller.start();
  context.mock.timers.tick(3000);
  assert.equal(calls, 1);
  poller.stop();
  request.resolve();
  await Promise.resolve();
  await Promise.resolve();
  context.mock.timers.tick(30000);
  assert.equal(calls, 1);
  assert.equal(accepted, 0);
});

test("stream snapshots and rebinding invalidate old HTTP responses", () => {
  const epoch = new RequestEpoch();
  const old = epoch.next();
  epoch.invalidate();
  assert.equal(epoch.accepts(old), false);
  const current = epoch.next();
  assert.equal(epoch.accepts(current), true);
  epoch.next();
  assert.equal(epoch.accepts(current), false);
});

test("site snapshots cannot overwrite newer events or a new runtime", () => {
  setSiteTestRuntime({ runtimeRevision: 2, generation: 1 });
  assert.equal(
    acceptSiteTestVersion({
      runId: 2,
      sequence: 5,
      runtimeRevision: 2,
      generation: 1,
    }),
    true
  );
  assert.equal(
    acceptSiteTestVersion({
      runId: 2,
      sequence: 3,
      runtimeRevision: 2,
      generation: 1,
    }),
    false
  );
  setSiteTestRuntime({ runtimeRevision: 3, generation: 2 });
  assert.equal(
    acceptSiteTestVersion({
      runId: 3,
      sequence: 1,
      runtimeRevision: 2,
      generation: 1,
    }),
    false
  );
  assert.equal(
    acceptSiteTestVersion({
      runId: 3,
      sequence: 1,
      runtimeRevision: 3,
      generation: 2,
    }),
    true
  );
});

test("connection merge preserves order without mutating incoming snapshots", () => {
  const old = [
    { id: "a", upload: 10, download: 20 },
    { id: "b", upload: 5, download: 5 },
  ];
  const incoming = [
    Object.freeze({ id: "b", upload: 8, download: 7 }),
    Object.freeze({ id: "a", upload: 2, download: 1 }),
  ];
  const result = mergeConnectionSnapshot(old, incoming);
  assert.deepEqual(
    result.map((item) => item.id),
    ["a", "b"]
  );
  assert.equal(result[0].curDownload, 0);
  assert.equal(result[1].curUpload, 3);
  assert.equal(incoming[0].curUpload, undefined);
});

test("built-in site normalization is idempotent and preserves custom sites", () => {
  const builtin = {
    uid: "builtin-a",
    name: "A",
    url: "https://example.com",
    icon: "icon",
    builtIn: true,
  };
  const defaults = new Map([["A::https://example.com", builtin]]);
  const custom = { uid: "custom", name: "Custom", url: "https://example.org" };
  const first = normalizeTestList([custom], defaults);
  assert.equal(first.changed, true);
  assert.deepEqual(first.normalized, [builtin, custom]);
  const persisted = first.normalized.map(({ builtIn, ...item }) => item);
  const second = normalizeTestList(persisted, defaults);
  assert.equal(second.changed, false);
  assert.deepEqual(second.normalized, [builtin, custom]);
});
