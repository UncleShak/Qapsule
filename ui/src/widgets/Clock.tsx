import { pressHistory } from "../bridge";
import { Glyph, stateGlyph } from "../capsule/Glyph";
import type { HistoryView } from "../types";

/** The clock column: the time and date above, the last notifications below. */
export function Clock({ time, date, history }: { time: string; date: string; history: HistoryView[] }) {
  return (
    <section className="widget widget-clock" aria-label="Clock and notifications">
      <div className="clock-top">
        <span className="widget-time">{time}</span>
        <span className="widget-date">{date}</span>
      </div>
      <div className="history-divider" />
      <div className="history" aria-label="Recent notifications">
        {history.length === 0 && <span className="history-empty">No notifications yet</span>}
        {history.map((h) => {
          const inner = (
            <>
              <Glyph name={stateGlyph(h.state, h.icon)} className="history-icon" />
              <span className="history-title">{h.title}</span>
              <span className="history-at">{h.at}</span>
            </>
          );
          return h.actionable ? (
            <button
              key={`${h.source}/${h.id}`}
              type="button"
              className="history-row history-action"
              aria-label={`${h.source_label}: ${h.title}`}
              onClick={() => pressHistory(h.source, h.id)}
            >
              {inner}
            </button>
          ) : (
            <div key={`${h.source}/${h.id}`} className="history-row">
              {inner}
            </div>
          );
        })}
      </div>
    </section>
  );
}
