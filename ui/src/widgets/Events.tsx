import type { EventNext, EventsReady, EventsView } from "../types";

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
  return (
    <>
      {now && (
        <div className="event-now">
          <span className="event-until">Now · until {now.until}</span>
          <span className="event-title">{now.title}</span>
          {now.location && <span className="widget-caption">{now.location}</span>}
        </div>
      )}
      {now && <hr className="divider" />}
      <div className="event-scroll">
        {groups(next).map((group) => (
          <section key={group.day} className="event-day">
            <h3 className="event-day-name">{group.day}</h3>
            <ul className="event-next">
              {group.items.map((event, i) => (
                <li key={i}>
                  <span className="event-time">{event.time}</span>
                  <span className="event-name">{event.title}</span>
                </li>
              ))}
            </ul>
          </section>
        ))}
        {next.length === 0 && <p className="event-empty">Nothing else this week</p>}
      </div>
      {events.stale && <span className="widget-caption stale">Updated a while ago</span>}
    </>
  );
}

function groups(next: EventNext[]): { day: string; items: EventNext[] }[] {
  const out: { day: string; items: EventNext[] }[] = [];
  for (const event of next) {
    const last = out[out.length - 1];
    if (last && last.day === event.day) last.items.push(event);
    else out.push({ day: event.day, items: [event] });
  }
  return out;
}
