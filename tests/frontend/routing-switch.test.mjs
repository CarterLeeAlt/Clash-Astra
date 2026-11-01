import assert from "node:assert/strict";
import { test } from "node:test";
import { LatestOperation } from "../../src/shared/async/latest-operation.ts";

const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};

test("switch queue runs one request and keeps only the latest queued click", async () => {
  const first = deferred();
  const calls = [];
  const busy = [];
  const queue = new LatestOperation(
    async (value, budget) => {
      calls.push(value);
      assert.ok(budget > 0 && budget <= 7000);
      if (value === "A") await first.promise;
    },
    (value) => busy.push(value)
  );
  const left = queue.submit("A");
  const skipped = queue.submit("B");
  const right = queue.submit("C");
  await skipped;
  assert.deepEqual(calls, ["A"]);
  first.resolve();
  await Promise.all([left, right]);
  assert.deepEqual(calls, ["A", "C"]);
  assert.equal(busy.at(-1), undefined);
});

test("a failed earlier switch does not swallow the latest user selection", async () => {
  const first = deferred();
  const calls = [];
  const queue = new LatestOperation(
    async (value) => {
      calls.push(value);
      if (value === "A") await first.promise;
    },
    () => {}
  );
  const left = assert.rejects(queue.submit("A"), /controller rejected/);
  const right = queue.submit("B");
  first.reject(new Error("controller rejected"));
  await Promise.all([left, right]);
  assert.deepEqual(calls, ["A", "B"]);
});

test("queued work passes only the remaining part of the original seven-second deadline", async (context) => {
  let now = 0;
  context.mock.method(performance, "now", () => now);
  const first = deferred();
  const budgets = [];
  const queue = new LatestOperation(
    async (value, budget) => {
      budgets.push(budget);
      if (value === "A") await first.promise;
    },
    () => {}
  );
  const left = queue.submit("A");
  const right = queue.submit("B");
  now = 6500;
  first.resolve();
  await Promise.all([left, right]);
  assert.deepEqual(budgets, [7000, 500]);
});

test("an expired queued selection never reaches the backend", async (context) => {
  let now = 0;
  context.mock.method(performance, "now", () => now);
  const first = deferred();
  const calls = [];
  const queue = new LatestOperation(
    async (value) => {
      calls.push(value);
      if (value === "A") await first.promise;
    },
    () => {}
  );
  const left = queue.submit("A");
  const right = assert.rejects(queue.submit("B"), /7-second/);
  now = 7001;
  first.resolve();
  await Promise.all([left, right]);
  assert.deepEqual(calls, ["A"]);
});

test("IPC receives the original click deadline rather than a fresh relative timeout", async (context) => {
  context.mock.method(Date, "now", () => 100000);
  const first = deferred();
  const deadlines = [];
  const queue = new LatestOperation(
    async (value, _remaining, deadline) => {
      deadlines.push(deadline);
      if (value === "A") await first.promise;
    },
    () => {}
  );
  const left = queue.submit("A");
  const right = queue.submit("B");
  context.mock.method(Date, "now", () => 106000);
  first.resolve();
  await Promise.all([left, right]);
  assert.deepEqual(deadlines, [107000, 107000]);
});
