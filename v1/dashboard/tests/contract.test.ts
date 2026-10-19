import assert from "node:assert/strict";
import test from "node:test";
import fixture from "../../contracts/fixtures/frontier.json" with { type: "json" };
import { parseFrontier } from "../src/frontier.ts";

test("the canonical shared fixture crosses the schema boundary without coercion", () => {
  const result = parseFrontier(fixture);
  assert.equal(result.project_id, "11111111-1111-4111-8111-111111111111");
  assert.equal(result.cursor, "42");
  assert.deepEqual(
    result.nodes.map((node) => node.state),
    ["running", "running", "waiting_peer", "runnable"],
  );
});
test("unknown root and node fields are rejected, never stripped", () => {
  assert.throws(
    () => parseFrontier({ ...fixture, silently_ignored: true }),
    /Invalid FrontierSnapshot/,
  );
  const changed = structuredClone(fixture);
  Object.assign(changed.nodes[0], { silently_ignored: true });
  assert.throws(() => parseFrontier(changed), /Invalid FrontierSnapshot/);
  assert.equal("silently_ignored" in changed.nodes[0], true);
});
test("numeric cursor and unknown work state are rejected", () => {
  assert.throws(
    () => parseFrontier({ ...fixture, cursor: 42 }),
    /Invalid FrontierSnapshot/,
  );
  const changed = structuredClone(fixture);
  changed.nodes[0].state = "approved_by_magic";
  assert.throws(() => parseFrontier(changed), /Invalid FrontierSnapshot/);
});
test("UUID and cursor semantics agree with Rust without Number precision loss", () => {
  assert.throws(
    () => parseFrontier({ ...fixture, project_id: "not-a-uuid" }),
    /Invalid FrontierSnapshot/,
  );
  assert.throws(
    () =>
      parseFrontier({
        ...fixture,
        project_id: "00000000-0000-0000-0000-000000000000",
      }),
    /Invalid FrontierSnapshot/,
  );
  for (const cursor of ["01", "-1", "9223372036854775808"])
    assert.throws(
      () => parseFrontier({ ...fixture, cursor }),
      /Invalid FrontierSnapshot/,
    );
  assert.equal(
    parseFrontier({ ...fixture, cursor: "9223372036854775807" }).cursor,
    "9223372036854775807",
  );
});
