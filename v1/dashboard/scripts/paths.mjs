import path from "node:path";
import { realpathSync } from "node:fs";
import { execFileSync } from "node:child_process";

const normalized = (value) => {
  const resolved = path.resolve(value);
  return process.platform === "win32" ? resolved.toLowerCase() : resolved;
};
export function selectOutput(root, checkout, planId, planEnvelope) {
  if (normalized(root) === normalized(checkout)) {
    if (planId)
      throw new Error(
        "A Plan ID must not direct a Root foundation run into a Plan's outputs.",
      );
    return path.join(root, ".mine", "build", "v1-foundation", "ui");
  }
  if (!planId || !/^plan\d{2}(?:-\d{2})*(?:-C\d{2})?$/.test(planId)) {
    throw new Error(
      "Linked checkouts require the exact MINE_PLAN_ID; no lineage or ancestor guessing.",
    );
  }
  const node = planEnvelope?.data?.plan ?? planEnvelope?.data;
  if (
    planEnvelope?.ok !== true ||
    node?.id !== planId ||
    !["IN_PROGRESS", "IMPLEMENTED"].includes(node?.status)
  ) {
    throw new Error(
      "Native Plan observation must identify this active/reviewable node.",
    );
  }
  if (
    typeof node.worktree_path !== "string" ||
    path.isAbsolute(node.worktree_path)
  ) {
    throw new Error("Expected a Root-relative native worktree locator.");
  }
  const worktree = path.resolve(root, node.worktree_path);
  const relative = path.relative(root, worktree);
  if (
    relative.startsWith("..") ||
    path.isAbsolute(relative) ||
    normalized(worktree) !== normalized(checkout)
  ) {
    throw new Error("Native Plan worktree does not match this checkout.");
  }
  return path.join(root, ".mine", "build", planId, "ui");
}
export function resolvePaths(dashboard, env = process.env) {
  const git = (args, cwd = dashboard) =>
    execFileSync("git", args, {
      cwd,
      encoding: "utf8",
      windowsHide: true,
    }).trim();
  const common = realpathSync(
    git(["rev-parse", "--path-format=absolute", "--git-common-dir"]),
  );
  // Standard Root .git is verified, not guessed from the nested package's ancestors.
  if (path.basename(common) !== ".git")
    throw new Error(
      "Nonstandard separate Git-directory layout needs an explicit supported Root resolver.",
    );
  const root = realpathSync(path.dirname(common));
  const checkout = realpathSync(git(["rev-parse", "--show-toplevel"]));
  if (
    normalized(realpathSync(git(["rev-parse", "--show-toplevel"], root))) !==
      normalized(root) ||
    normalized(
      realpathSync(
        git(["rev-parse", "--path-format=absolute", "--git-dir"], root),
      ),
    ) !== normalized(common)
  ) {
    throw new Error(
      "Git common directory did not resolve to a verified Root checkout.",
    );
  }
  let envelope;
  if (normalized(checkout) !== normalized(root)) {
    const id = env.MINE_PLAN_ID;
    if (!id || !/^plan\d{2}(?:-\d{2})*(?:-C\d{2})?$/.test(id))
      throw new Error("Linked checkout requires an exact MINE_PLAN_ID.");
    envelope = JSON.parse(
      execFileSync("mine", ["plan", "show", "--id", id, "--format", "json"], {
        cwd: root,
        encoding: "utf8",
        windowsHide: true,
      }),
    );
    const node = envelope?.data?.plan ?? envelope?.data;
    if (
      typeof node?.worktree_path === "string" &&
      normalized(realpathSync(path.resolve(root, node.worktree_path))) !==
        normalized(checkout)
    ) {
      throw new Error("Registered worktree real path differs from checkout.");
    }
    const branch = git(["branch", "--show-current"]);
    if (node?.branch !== branch)
      throw new Error("Registered Plan branch differs from this checkout.");
  }
  return {
    root,
    checkout,
    output: selectOutput(root, checkout, env.MINE_PLAN_ID, envelope),
    environment: path.join(
      root,
      ".mine",
      "build",
      "v1-foundation",
      "ui",
      "environment",
    ),
    isRoot: normalized(root) === normalized(checkout),
  };
}
