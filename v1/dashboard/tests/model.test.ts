import assert from "node:assert/strict";
import test from "node:test";
import {
  projects,
  summarize,
  visibleWork,
  sharedFrontier,
  mapFrontier,
} from "../src/model.ts";

test("work counters are derived from the selected project", () => {
  assert.deepEqual(summarize(projects[0].work), {
    active: 2,
    waiting: 1,
    review: 0,
    done: 0,
  });
  assert.deepEqual(summarize(projects[1].work), {
    active: 1,
    waiting: 1,
    review: 0,
    done: 1,
  });
});
test("attention view includes a waiting peer without calling runnable work reviewed", () => {
  assert.deepEqual(
    visibleWork(projects[0], "attention").map((item) => item.id),
    ["22222222-2222-4222-8222-222222222223"],
  );
});
test("mapping preserves shared identities, cursor, waiting owner and exact source state", () => {
  assert.equal(projects[0].id, sharedFrontier.project_id);
  assert.equal(projects[0].snapshotCursor, "42");
  assert.equal(projects[0].paused, false);
  const mapped = mapFrontier(sharedFrontier);
  assert.equal(mapped[2].sourceState, "waiting_peer");
  assert.equal(mapped[2].waitingFor, "ARC-4H9L2B");
  assert.equal(mapped[3].state, "ready");
  assert.equal(mapped[3].sourceState, "runnable");
});
test("every wait names who must respond; all local fixture identities are unique", () => {
  assert.equal(
    new Set(projects.map((project) => project.id)).size,
    projects.length,
  );
  for (const project of projects) {
    assert.equal(
      new Set(project.work.map((item) => item.id)).size,
      project.work.length,
    );
    for (const item of project.work) {
      assert.ok(item.progress >= 0 && item.progress <= 100);
      if (item.state === "waiting") assert.ok(item.waitingFor?.trim());
    }
  }
});
