import { Fragment, type CSSProperties } from "react";
import type { EventsReady, EventsView } from "../types";

export function Events({ events }: { events: EventsView }) {
  return (
    <section className="widget widget-events" aria-label="Calendar">
      <Body events={events} />
    </section>
  );
}

function Body({ events }: { events: EventsView }) {
  switch (events.kind) {
    case "no_link":
      return (
        <p className="widget-note">
          Add a calendar with <code>qapsule calendar set-link</code>
        </p>
      );
    case "unavailable":
      return <p className="widget-note">Calendar unavailable</p>;
    case "loading":
      return (
        <div className="placeholder" aria-busy="true" aria-label="Loading calendar">
          <span className="placeholder-bar wide" />
          <span className="placeholder-bar" />
          <span className="placeholder-bar" />
        </div>
      );
    case "ready":
      return <Ready events={events} />;
  }
}

function Ready({ events }: { events: EventsReady }) {
  const { head, days, more } = events;
  return (
    <>
      {head && (
        <>
          <div className="event-head">
            <span className="event-until">{head.caption}</span>
            <span className="event-title">{head.title}</span>
            {head.location && <span className="widget-caption">{head.location}</span>}
          </div>
          <hr className="divider" />
        </>
      )}
      <div className="event-list">
        {days.map((day, d) => (
          <section key={d} className="event-day">
            {day.label && <h3 className="event-day-name">{day.label}</h3>}
            <ul className="event-rows">
              {day.rows.map((event, i) => (
                <Fragment key={i}>
                  {day.now_line === i && <li className="event-now-line" aria-hidden="true" />}
                  <li className={event.past ? "event-row past" : "event-row"}>
                    <span
                      className="event-bar"
                      style={event.color ? ({ "--event-color": event.color } as CSSProperties) : undefined}
                    />
                    <span className="event-name">{event.title}</span>
                    <span className="event-time">{event.time}</span>
                  </li>
                </Fragment>
              ))}
              {day.now_line === day.rows.length && (
                <li className="event-now-line" aria-hidden="true" />
              )}
            </ul>
          </section>
        ))}
        {days.length === 0 && !more && <p className="event-empty">Nothing else this week</p>}
      </div>
      {more && <p className="event-more">{more}</p>}
      {events.stale && <span className="widget-caption stale">Updated a while ago</span>}
    </>
  );
}
