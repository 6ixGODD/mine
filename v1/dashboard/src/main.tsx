import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  Activity,
  ArrowDownLeft,
  ArrowRight,
  ArrowUpRight,
  Bell,
  ChevronDown,
  ChevronRight,
  Circle,
  CircleCheck,
  CircleHelp,
  Clock3,
  FileText,
  GitBranch,
  Layers3,
  LayoutDashboard,
  MessageSquare,
  MoreHorizontal,
  Radio,
  Search,
  Settings2,
  ShieldCheck,
  Sparkles,
  Users,
  X,
} from "lucide-react";
import { projects, summarize, visibleWork } from "./model";
import type { WorkItem } from "./model";
import { Button } from "./components/ui/button";
import "./index.css";
import "./styles.css";

const labels = {
  running: "In progress",
  waiting: "Waiting",
  review: "In review",
  done: "Complete",
  ready: "Ready",
  paused: "Paused",
  recovering: "Recovering",
  failed: "Failed",
};

function App() {
  const [projectId, setProjectId] = useState(projects[0].id);
  const [filter, setFilter] = useState<"all" | "attention">("all");
  const [detail, setDetail] = useState<WorkItem | null>(null);
  const [notice, setNotice] = useState("");
  useEffect(() => {
    if (!detail) return;
    const previous =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setDetail(null);
      if (event.key !== "Tab") return;
      const elements = Array.from(
        document.querySelectorAll<HTMLElement>(
          ".detail-drawer button, .detail-drawer a[href], .detail-drawer [tabindex='0']",
        ),
      );
      const first = elements[0];
      const last = elements[elements.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    };
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("keydown", handleKey);
      previous?.focus();
    };
  }, [detail]);
  const project = projects.find((entry) => entry.id === projectId)!;
  const summary = summarize(project.work);
  const inspectOnly = (feature: string) =>
    setNotice(
      `${feature} is a preview only. This demonstrator has no backend connection and cannot create sessions, send messages, or approve decisions.`,
    );

  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label="Main navigation">
        <a className="brand" href="#overview" aria-label="MINE overview">
          <span className="brand-mark">
            <Layers3 size={21} strokeWidth={2.2} />
          </span>
          MINE<span className="brand-version">1.0</span>
        </a>
        <label className="project-picker">
          <span className="project-avatar">{project.initials}</span>
          <span className="project-picker-label">
            Project
            <select
              aria-label="Select demo project"
              value={projectId}
              onChange={(event) => {
                setProjectId(event.target.value);
                setDetail(null);
              }}
            >
              {projects.map((entry) => (
                <option value={entry.id} key={entry.id}>
                  {entry.name}
                </option>
              ))}
            </select>
          </span>
          <ChevronDown size={14} aria-hidden="true" />
        </label>
        <div className="nav-label">WORKSPACE</div>
        <nav>
          <a className="nav-item selected" href="#overview" aria-current="page">
            <LayoutDashboard />
            Overview
          </a>
          <a className="nav-item" href="#frontier">
            <GitBranch />
            Work frontier
            <span className="nav-count">{project.work.length}</span>
          </a>
          <button
            className="nav-item"
            onClick={() => inspectOnly("Agent registry")}
          >
            <Users />
            Agents
          </button>
          <a className="nav-item" href="#decisions">
            <FileText />
            Decisions
            <span className="nav-count">{project.decisions.length}</span>
          </a>
          <a className="nav-item" href="#handoffs">
            <MessageSquare />
            Handoffs
          </a>
          <button
            className="nav-item"
            onClick={() => inspectOnly("Evidence browser")}
          >
            <ShieldCheck />
            Evidence
          </button>
        </nav>
        <div className="sidebar-bottom">
          <div className="autonomy-card">
            <span className="autonomy-title">
              <Sparkles size={15} />
              Autonomy<span className="off-label">OFF</span>
            </span>
            <p>
              You stay in control.
              <br />
              Experimental automation is disabled.
            </p>
          </div>
          <button
            className="nav-item"
            onClick={() => inspectOnly("Project settings")}
          >
            <Settings2 />
            Settings
          </button>
          <button
            className="nav-item"
            onClick={() => inspectOnly("Help center")}
          >
            <CircleHelp />
            Help & documentation
          </button>
          <div className="local-status">
            <span className="dot muted" /> Local fixture · not connected
          </div>
        </div>
      </aside>

      <div className="workspace">
        <header className="topbar">
          <div className="breadcrumb">
            <span>Workspace</span>
            <ChevronRight size={13} />
            <span>{project.name}</span>
            <ChevronRight size={13} />
            <strong>Overview</strong>
          </div>
          <div className="topbar-actions">
            <span className="demo-label">DEMO DATA</span>
            <button
              className="icon-button"
              aria-label="Search preview"
              onClick={() => inspectOnly("Search")}
            >
              <Search size={17} />
            </button>
            <button
              className="icon-button"
              aria-label="Notifications preview"
              onClick={() => inspectOnly("Notifications")}
            >
              <Bell size={17} />
            </button>
            <span className="user-avatar" title="Demo project owner">
              Y
            </span>
          </div>
        </header>
        <main id="overview">
          <div className="page-heading">
            <div>
              <div className="eyebrow">YOUR PROJECT, AT A GLANCE</div>
              <h1>Work overview</h1>
              <p>{project.goal}</p>
            </div>
            <Button
              variant="outline"
              className="button secondary"
              onClick={() => inspectOnly("Session creation")}
            >
              <Users size={15} />
              Manage agents
              <ArrowUpRight size={14} />
            </Button>
          </div>
          <div className="demo-banner">
            <Radio size={16} />
            <span>
              <strong>Interactive preview.</strong> Switch projects and inspect
              work below. All activity is illustrative; no real agents are
              running.
              {project.snapshotCursor &&
                ` Shared contract cursor: ${project.snapshotCursor}. Project ${project.paused ? "paused" : "not paused"}.`}
            </span>
          </div>
          <section className="metrics" aria-label="Project summary">
            <Metric
              label="Working now"
              value={summary.active}
              description="Independent execution lanes"
              icon={<Activity />}
            />
            <Metric
              label="Waiting on a response"
              value={summary.waiting}
              description="Responsibility stays assigned"
              icon={<Clock3 />}
              attention
            />
            <Metric
              label="Ready for review"
              value={summary.review}
              description="Independent verification next"
              icon={<ShieldCheck />}
            />
            <Metric
              label="Completed"
              value={summary.done}
              description="Accepted work in this example"
              icon={<CircleCheck />}
            />
          </section>

          <section className="panel frontier" id="frontier">
            <div className="panel-heading">
              <div>
                <h2>
                  Work frontier{" "}
                  <span className="number-pill">{project.work.length}</span>
                </h2>
                <p>What is moving forward, and what needs a hand.</p>
              </div>
              <div className="segmented" aria-label="Filter work">
                <button
                  className={filter === "all" ? "active" : ""}
                  onClick={() => setFilter("all")}
                  aria-pressed={filter === "all"}
                >
                  All work
                </button>
                <button
                  className={filter === "attention" ? "active" : ""}
                  onClick={() => setFilter("attention")}
                  aria-pressed={filter === "attention"}
                >
                  Needs attention
                </button>
              </div>
            </div>
            <div className="work-table" role="table" aria-label="Work items">
              <div className="table-header" role="row">
                <span role="columnheader">WORK ITEM</span>
                <span role="columnheader">RESPONSIBLE AGENT</span>
                <span role="columnheader">STATUS</span>
                <span role="columnheader">PROGRESS</span>
                <span />
              </div>
              {visibleWork(project, filter).map((item) => (
                <button
                  className="work-row"
                  key={item.id}
                  onClick={() => setDetail(item)}
                  aria-label={`Inspect ${item.title}`}
                >
                  <span className="work-name">
                    <span className={`work-symbol ${item.state}`}>
                      {item.state === "done" ? (
                        <CircleCheck size={18} />
                      ) : item.state === "waiting" ? (
                        <Clock3 size={18} />
                      ) : (
                        <GitBranch size={18} />
                      )}
                    </span>
                    <span>
                      <strong>{item.title}</strong>
                      <small>{item.detail}</small>
                    </span>
                  </span>
                  <span className="agent-cell">
                    <span className={`agent-avatar ${item.role.toLowerCase()}`}>
                      {item.role === "Reviewer"
                        ? "R"
                        : item.role === "Architect"
                          ? "A"
                          : "E"}
                    </span>
                    <span>
                      <strong>{item.owner}</strong>
                      <small>{item.role}</small>
                    </span>
                  </span>
                  <span>
                    <span className={`status-badge ${item.state}`}>
                      <span className="dot" />
                      {labels[item.state]}
                    </span>
                  </span>
                  <span className="progress-cell">
                    <span className="progress-track">
                      <span style={{ width: `${item.progress}%` }} />
                    </span>
                    <small>{item.progress}%</small>
                  </span>
                  <ChevronRight size={15} className="row-chevron" />
                </button>
              ))}
            </div>
            <div className="panel-foot">
              <span>
                <ShieldCheck size={13} />
                Owners stay accountable across session restarts.
              </span>
              <span>Illustrative progress, not acceptance evidence</span>
            </div>
          </section>

          <div className="lower-grid">
            <section className="panel" id="decisions">
              <div className="panel-heading">
                <div>
                  <h2>
                    Decisions that need you{" "}
                    <span className="number-pill">
                      {project.decisions.length}
                    </span>
                  </h2>
                  <p>Business intent stays with the project owner.</p>
                </div>
                <CircleHelp size={17} className="subtle" />
              </div>
              <div className="decision-list">
                {project.decisions.map((decision) => (
                  <article className="decision" key={decision.id}>
                    <div className="decision-meta">
                      <span>{decision.id}</span>
                      <span
                        className={
                          decision.kind === "Business decision"
                            ? "business-label"
                            : "technical-label"
                        }
                      >
                        {decision.kind}
                      </span>
                    </div>
                    <h3>{decision.title}</h3>
                    <p>{decision.detail}</p>
                    <button
                      className="text-button"
                      onClick={() => inspectOnly(`Decision ${decision.id}`)}
                    >
                      Inspect proposal
                      <ArrowRight size={14} />
                    </button>
                  </article>
                ))}
              </div>
              <div className="panel-foot">
                <span>
                  <Sparkles size={13} />
                  Technical auto-approval is experimental and off.
                </span>
              </div>
            </section>
            <section className="panel" id="handoffs">
              <div className="panel-heading">
                <div>
                  <h2>Recent handoffs</h2>
                  <p>No context should fall between agents.</p>
                </div>
                <MoreHorizontal size={18} className="subtle" />
              </div>
              <div className="timeline">
                {project.handoffs.map((handoff, index) => (
                  <article
                    className="handoff"
                    key={`${handoff.from}-${handoff.title}`}
                  >
                    <span
                      className={`timeline-icon ${index === 0 ? "highlight" : ""}`}
                    >
                      <ArrowDownLeft size={15} />
                    </span>
                    <div>
                      <div className="handoff-title">
                        <h3>{handoff.title}</h3>
                        <time>{handoff.age}</time>
                      </div>
                      <div className="handoff-route">
                        <span>{handoff.from}</span>
                        <ArrowRight size={11} />
                        <span>{handoff.to}</span>
                      </div>
                      <p>{handoff.detail}</p>
                    </div>
                  </article>
                ))}
              </div>
              <div className="panel-foot">
                <span>
                  <MessageSquare size={13} />
                  Demo messages only · nothing has been sent
                </span>
              </div>
            </section>
          </div>
          <footer className="page-footer">
            <span>
              <Circle size={11} />
              All views use local sample data.
            </span>
            <span>MINE / independent agents, shared understanding</span>
          </footer>
        </main>
      </div>

      {notice && (
        <div className="notice" role="status">
          <CircleHelp size={18} />
          <p>{notice}</p>
          <button
            className="icon-button"
            aria-label="Dismiss notice"
            onClick={() => setNotice("")}
          >
            <X size={17} />
          </button>
        </div>
      )}
      {detail && (
        <div className="drawer-backdrop" onClick={() => setDetail(null)}>
          <section
            className="detail-drawer"
            role="dialog"
            aria-modal="true"
            aria-labelledby="detail-title"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="drawer-top">
              <span className="demo-label">LOCAL DEMO DETAIL</span>
              <button
                className="icon-button"
                aria-label="Close work details"
                onClick={() => setDetail(null)}
                autoFocus
              >
                <X size={20} />
              </button>
            </div>
            <div className={`status-badge ${detail.state}`}>
              <span className="dot" />
              {labels[detail.state]}
            </div>
            <h2 id="detail-title">{detail.title}</h2>
            <p>{detail.detail}</p>
            <dl>
              <dt>Responsible agent</dt>
              <dd>{detail.owner}</dd>
              <dt>Role</dt>
              <dd>{detail.role}</dd>
              <dt>Observed work state</dt>
              <dd>{detail.sourceState ?? detail.state}</dd>
              <dt>Waiting for</dt>
              <dd>{detail.waitingFor ?? "No response required"}</dd>
              <dt>Session connection</dt>
              <dd>Not connected — example identity only</dd>
            </dl>
            <div className="detail-note">
              <ShieldCheck size={18} />
              <p>
                Ownership is not released because a session fails. A future
                authorized transfer must be recorded and acknowledged. This
                preview cannot perform that transfer.
              </p>
            </div>
            <button
              className="button secondary"
              onClick={() => setDetail(null)}
            >
              Back to overview
            </button>
          </section>
        </div>
      )}
    </div>
  );
}
function Metric({
  label,
  value,
  description,
  icon,
  attention = false,
}: {
  label: string;
  value: number;
  description: string;
  icon: React.ReactNode;
  attention?: boolean;
}) {
  return (
    <article className={`metric ${attention ? "attention" : ""}`}>
      <div className="metric-label">
        {label}
        <span>{icon}</span>
      </div>
      <div className="metric-value">{value.toString().padStart(2, "0")}</div>
      <p>{description}</p>
    </article>
  );
}
createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
