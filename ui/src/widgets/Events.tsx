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
  const { now, next } = events;
  const nothing = !now && next.length === 0;
  return (
    <>
      {now && (
        <div className="event-now">
          <span className="event-until">Now · until {now.until}</span>
          <span className="event-title">{now.title}</span>
          {now.location && <span className="widget-caption">{now.location}</span>}
        </div>
      )}
      {now && next.length > 0 && <hr className="divider" />}
      {next.length > 0 && (
        <ul className="event-next">
          {next.slice(0, 2).map((event, i) => (
            <li key={i}>
              <span className="event-name">{event.title}</span>
              <span className="event-when">{event.when}</span>
            </li>
          ))}
        </ul>
      )}
      {nothing && <p className="widget-note">Nothing else today</p>}
      {events.stale && <span className="widget-caption stale">Updated a while ago</span>}
    </>
  );
}
