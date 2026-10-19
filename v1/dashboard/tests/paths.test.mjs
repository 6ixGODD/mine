import assert from "node:assert/strict";
import test from "node:test";
import path from "node:path";
import { selectOutput } from "../scripts/paths.mjs";

const root = path.resolve("/fixture-root");
// mine-release-allow-plan-reference: canonical ID fixture, not release provenance
const checkout = path.join(root, ".mine", "worktrees", "plan25");
const envelope = {
  ok: true,
  data: {
    // mine-release-allow-plan-reference: canonical ID fixture, not release provenance
    id: "plan25-C01",
    status: "IN_PROGRESS",
    // mine-release-allow-plan-reference: canonical ID fixture, not release provenance
    worktree_path: ".mine/worktrees/plan25",
  },
};
test("Root foundation has explicit outputs, not a guessed Plan", () => {
  assert.equal(
    selectOutput(root, root),
    path.join(root, ".mine/build/v1-foundation/ui"),
  );
  // mine-release-allow-plan-reference: canonical ID fixture, not release provenance
  assert.throws(() => selectOutput(root, root, "plan25"), /Root foundation/);
});
test("compensation shares its lineage checkout, never its mutable node outputs", () => {
  assert.equal(
    // mine-release-allow-plan-reference: canonical ID fixture, not release provenance
    selectOutput(root, checkout, "plan25-C01", envelope),
    // mine-release-allow-plan-reference: canonical ID fixture, not release provenance
    path.join(root, ".mine/build/plan25-C01/ui"),
  );
});
test("missing ID, mismatched native owner, foreign checkout and path escape refuse", () => {
  assert.throws(() => selectOutput(root, checkout), /exact MINE_PLAN_ID/);
  assert.throws(
    // mine-release-allow-plan-reference: canonical ID fixture, not release provenance
    () => selectOutput(root, checkout, "plan25", envelope),
    /Native Plan/,
  );
  assert.throws(
    // mine-release-allow-plan-reference: canonical ID fixture, not release provenance
    () => selectOutput(root, path.join(root, "other"), "plan25-C01", envelope),
    /does not match/,
  );
  assert.throws(
    () =>
      // mine-release-allow-plan-reference: canonical ID fixture, not release provenance
      selectOutput(root, checkout, "plan25-C01", {
        ok: true,
        data: { ...envelope.data, worktree_path: "../foreign" },
      }),
    /does not match/,
  );
});
