// The Focus screen (decision 0012): the study timer with its module
// chip, this week's focus by day, and by module. Rust works out every number and colour.

import type { CSSProperties } from "react";
import { focusLength, focusModule, focusReset, focusToggle } from "../bridge";
import { Glyph } from "../capsule/Glyph";
import type { FocusView, StudyStats } from "../types";

export function Focus({ focus }: { focus: FocusView }) {
  return (
    <div className="main-screen focus-screen">
      <Timer focus={focus} />
      <Week focus={focus} />
      <Modules stats={focus.stats} />
    </div>
  );
}

const R = 44;
const C = 2 * Math.PI * R;

function Timer({ focus }: { focus: FocusView }) {
  const active = focus.phase === "running" || focus.phase === "paused";
  const running = focus.phase === "running";
  const sessions = focus.stats.sessions_today;
  const status = running
    ? "Pops wait until you pause"
    : focus.phase === "paused"
      ? "Paused"
      : sessions === 1
        ? "1 session today"
        : `${sessions} sessions today`;
  return (
    <section className="widget widget-focus" aria-label="Focus timer">
      <div className={`focus-ring phase-${focus.phase}`}>
        <svg viewBox="0 0 100 100" width="100" height="100" aria-hidden="true">
          <circle className="ring-track" cx="50" cy="50" r={R} />
          <circle
            className="ring-fill"
            cx="50"
            cy="50"
            r={R}
            strokeDasharray={C}
            strokeDashoffset={C * (1 - focus.progress)}
          />
        </svg>
        <span className="focus-time" role="timer" aria-live="off">
          {focus.display}
        </span>
        <span className="focus-caption">{focus.caption}</span>
      </div>
      <div className="focus-side">
        <button
          type="button"
          className="module-chip"
          onClick={() => focusModule()}
          aria-label={`Tag: ${focus.module?.code ?? "none"}. Press to change`}
        >
          <span className="module-dot" style={{ background: focus.module?.color ?? "var(--text-4)" }} />
          <span className="module-code">{focus.module?.code ?? "No tag"}</span>
          <Glyph name="chevron_down" className="module-chevron" />
        </button>
        <div className="focus-controls">
          <button
            type="button"
            className="focus-primary"
            onClick={() => focusToggle()}
            aria-label={running ? "Pause" : focus.phase === "paused" ? "Resume" : "Start"}
          >
            <Glyph name={running ? "pause" : "play"} />
          </button>
          <button
            type="button"
            className="focus-secondary"
            onClick={() => focusReset()}
            aria-label="Reset"
            disabled={focus.phase === "idle"}
          >
            <Glyph name="restart" />
          </button>
        </div>
        {active ? (
          <span className="focus-status">{status}</span>
        ) : (
          <div className="focus-lengths" role="radiogroup" aria-label="Length">
            {[...focus.lengths, 0].map((m) => (
              <button
                key={m}
                type="button"
                role="radio"
                aria-checked={focus.length_min === m}
                className="focus-length"
                onClick={() => focusLength(m)}
              >
                {m === 0 ? "Up" : m}
              </button>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}

function Week({ focus }: { focus: FocusView }) {
  const { stats, goal } = focus;
  return (
    <section className="widget widget-week" aria-label="This week">
      <div className="week-head">
        <span className="week-total">{stats.week}</span>
        <span className="widget-caption">this week</span>
        {stats.streak > 1 && <span className="week-streak">{stats.streak}-day streak</span>}
      </div>
      <div className="week-bars">
        {stats.bars.map((bar, i) => (
          <div key={i} className={`week-day ${bar.today ? "today" : ""}`} title={bar.time}>
            <span className="week-track">
              <span className="week-fill" style={{ "--h": bar.height } as CSSProperties} />
            </span>
            <span className="week-label">{bar.label}</span>
          </div>
        ))}
      </div>
      <span className="widget-caption week-today">
        Today · {goal.done} of {goal.target}
      </span>
    </section>
  );
}

function Modules({ stats }: { stats: StudyStats }) {
  return (
    <section className="widget widget-modules" aria-label="Tags this week">
      <span className="widget-caption">By tag</span>
      {stats.modules.length === 0 ? (
        <span className="widget-note">Start a session and your tags show here.</span>
      ) : (
        <ul className="module-rows">
          {stats.modules.map((m, i) => (
            <li key={m.code} className="module-row" style={{ "--i": i } as CSSProperties}>
              <span className="module-dot" style={{ background: m.color }} />
              <span className="module-name">{m.code}</span>
              <span className="module-track">
                <span className="module-fill" style={{ background: m.color, "--share": m.share } as CSSProperties} />
              </span>
              <span className="module-time">{m.time}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
