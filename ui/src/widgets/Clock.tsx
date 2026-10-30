export function Clock({ time, date }: { time: string; date: string }) {
  return (
    <section className="widget widget-clock" aria-label="Clock">
      <span className="widget-time">{time}</span>
      <span className="widget-date">{date}</span>
    </section>
  );
}
