import { pressHistory } from "../bridge";
import { Glyph, stateGlyph } from "../capsule/Glyph";
import type { HistoryView } from "../types";
import type { CSSProperties } from "react";

// Before the column was split, the clock was alone in it: 72 px, with the date under it,
// centred in the 128 px column. It opens from there and settles into the upper half, as a
// constant transform of its final layout, worked out here from the two sizes.
const COLUMN = 128;
const WAS = { time: 72, gap: 4, date: 17 }; // font size, gap, date line height
const NOW = { time: 48, band: 52 };
const WAS_CENTRE = COLUMN / 2 - (WAS.time + WAS.gap + WAS.date) / 2 + WAS.time / 2;
const OPENING = {
  "--clock-dy": `${WAS_CENTRE - NOW.band / 2}px`,
  "--clock-scale": WAS.time / NOW.time,
} as CSSProperties;

/** The clock column: the time and date above, the last notifications below. */
export function Clock({ time, date, history }: { time: string; date: string; history: HistoryView[] }) {
  return (
    <section className="widget widget-clock" aria-label="Clock and notifications" style={OPENING}>
      <div className="clock-top">
        <span className="widget-time">{time}</span>
        <span className="widget-date">{date}</span>
      </div>
      <div className="history-divider" />
      <div className="history" aria-label="Recent notifications">
        {history.length === 0 && <span className="history-empty">No notifications yet</span>}
        {history.map((h, i) => {
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
              style={{ "--i": i } as CSSProperties}
              aria-label={`${h.source_label}: ${h.title}`}
              onClick={() => pressHistory(h.source, h.id)}
            >
              {inner}
            </button>
          ) : (
            <div key={`${h.source}/${h.id}`} className="history-row" style={{ "--i": i } as CSSProperties}>
              {inner}
            </div>
          );
        })}
      </div>
    </section>
  );
}
