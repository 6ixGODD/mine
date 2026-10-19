import fixture from "../../contracts/fixtures/frontier.json" with { type: "json" };
import { parseFrontier } from "./frontier.ts";
import type { FrontierSnapshot, FrontierNode } from "./generated/contracts";
/** View-only grouping and decoration, never a second wire/persistence schema. */
export type WorkState =
  | "running"
  | "waiting"
  | "review"
  | "done"
  | "ready"
  | "paused"
  | "recovering"
  | "failed";
export interface WorkItem {
  id: string;
  title: string;
  detail: string;
  owner: string;
  role: "Executor" | "Architect" | "Reviewer" | "Unknown";
  state: WorkState;
  progress: number;
  waitingFor?: string;
  sourceState?: FrontierNode["state"];
}
const demoProfiles: Record<
  string,
  { label: string; role: WorkItem["role"]; progress: number }
> = {
  "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1": {
    label: "EXE-1DF3XA",
    role: "Executor",
    progress: 64,
  },
  "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa2": {
    label: "EXE-82LF4Q",
    role: "Executor",
    progress: 38,
  },
  "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa3": {
    label: "EXE-9KQ2MP",
    role: "Executor",
    progress: 25,
  },
  "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa4": {
    label: "ARC-4H9L2B",
    role: "Architect",
    progress: 0,
  },
  "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa5": {
    label: "REV-78PIKZ",
    role: "Reviewer",
    progress: 0,
  },
};
const stateGroups: Record<FrontierNode["state"], WorkState> = {
  runnable: "ready",
  running: "running",
  waiting_peer: "waiting",
  waiting_decision: "waiting",
  waiting_resource: "waiting",
  pausing: "paused",
  paused: "paused",
  recovering: "recovering",
  failed: "failed",
  done: "done",
};
export function mapFrontier(snapshot: FrontierSnapshot): WorkItem[] {
  return snapshot.nodes.map((node) => ({
    id: node.id,
    title: node.title,
    detail: node.next_action,
    owner: demoProfiles[node.owner]?.label ?? node.owner,
    role: demoProfiles[node.owner]?.role ?? "Unknown",
    state: stateGroups[node.state],
    sourceState: node.state,
    progress: demoProfiles[node.owner]?.progress ?? 0,
    ...(node.waiting_on
      ? { waitingFor: demoProfiles[node.waiting_on]?.label ?? node.waiting_on }
      : {}),
  }));
}
export const sharedFrontier = parseFrontier(fixture);
export interface ProjectView {
  id: string;
  name: string;
  initials: string;
  subtitle: string;
  goal: string;
  snapshotCursor?: FrontierSnapshot["cursor"];
  paused?: FrontierSnapshot["paused"];
  work: WorkItem[];
  decisions: {
    id: string;
    title: string;
    kind: "Business decision" | "Technical decision";
    detail: string;
  }[];
  handoffs: {
    from: string;
    to: string;
    title: string;
    detail: string;
    age: string;
  }[];
}
export const projects: readonly ProjectView[] = [
  {
    id: sharedFrontier.project_id,
    name: "MINE",
    initials: "M",
    subtitle: "Agent collaboration · demo workspace",
    goal: "Make parallel work visible, accountable, and recoverable.",
    snapshotCursor: sharedFrontier.cursor,
    paused: sharedFrontier.paused,
    work: mapFrontier(sharedFrontier),
    decisions: [
      {
        id: "D-004",
        title: "Who may authorize an external publication?",
        kind: "Business decision",
        detail:
          "Owner decision needed. No publication authority is inferred from autonomous mode.",
      },
      {
        id: "D-005",
        title: "Retry policy for an interrupted session",
        kind: "Technical decision",
        detail:
          "Architecture proposal ready for review. Autonomous technical approval is off in this demo.",
      },
    ],
    handoffs: [
      {
        from: "EXE-9KQ2MP",
        to: "ARC-4H9L2B",
        title: "Ownership contract clarification",
        detail:
          "Executor is waiting; responsibility stays held until the transfer is acknowledged.",
        age: "4m ago",
      },
      {
        from: "EXE-1DF3XA",
        to: "REV-78PIKZ",
        title: "Recovery evidence ready",
        detail:
          "Review requested for the pinned input. A request is not an acceptance.",
        age: "12m ago",
      },
      {
        from: "ARC-4H9L2B",
        to: "EXE-82LF4Q",
        title: "Delivery boundary clarified",
        detail: "Architecture response attached to the local example handoff.",
        age: "26m ago",
      },
    ],
  },
  {
    id: "atlas-demo",
    name: "Atlas",
    initials: "A",
    subtitle: "Knowledge platform · demo workspace",
    goal: "Deliver a searchable, source-linked knowledge collection.",
    work: [
      {
        id: "import",
        title: "Document import",
        detail: "Input validation and source attribution",
        owner: "EXE-6JP2DV",
        role: "Executor",
        state: "running",
        progress: 48,
      },
      {
        id: "privacy",
        title: "Access boundaries",
        detail: "Retention needs a product-owner decision",
        owner: "ARC-7HF9QL",
        role: "Architect",
        state: "waiting",
        progress: 15,
        waitingFor: "Project owner",
      },
      {
        id: "search",
        title: "Search contract",
        detail: "Reviewed fixture; not connected to a service",
        owner: "REV-5CN3WB",
        role: "Reviewer",
        state: "done",
        progress: 100,
      },
    ],
    decisions: [
      {
        id: "D-001",
        title: "How long should uploaded documents be retained?",
        kind: "Business decision",
        detail:
          "This changes privacy and operating policy; the project owner must decide.",
      },
    ],
    handoffs: [
      {
        from: "ARC-7HF9QL",
        to: "Project owner",
        title: "Retention requirement is unresolved",
        detail:
          "No technical default is substituted for this product decision.",
        age: "8m ago",
      },
    ],
  },
];
export function summarize(work: readonly WorkItem[]) {
  return {
    active: work.filter((item) => item.state === "running").length,
    waiting: work.filter((item) => item.state === "waiting").length,
    review: work.filter((item) => item.state === "review").length,
    done: work.filter((item) => item.state === "done").length,
  };
}
export function visibleWork(project: ProjectView, filter: "all" | "attention") {
  return filter === "all"
    ? project.work
    : project.work.filter(
        (item) => item.state === "waiting" || item.state === "review",
      );
}
