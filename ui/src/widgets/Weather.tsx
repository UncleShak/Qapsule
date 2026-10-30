import { Glyph } from "../capsule/Glyph";
import type { WeatherView } from "../types";

const degrees = (t: number) => `${t}°`;

export function Weather({ weather }: { weather: WeatherView }) {
  return (
    <section className="widget widget-weather" aria-label="Weather">
      <Body weather={weather} />
    </section>
  );
}

function Body({ weather }: { weather: WeatherView }) {
  switch (weather.kind) {
    case "no_city":
      return <p className="widget-note">Add your city to config.toml</p>;
    case "unavailable":
      return <p className="widget-note">Weather unavailable</p>;
    case "loading":
      return (
        <div className="placeholder" aria-busy="true" aria-label="Loading weather">
          <span className="placeholder-bar wide" />
          <span className="placeholder-bar" />
          <span className="placeholder-bar" />
        </div>
      );
    case "ready":
      return (
        <>
          <div className="weather-now">
            <Glyph name={weather.glyph} className="weather-glyph" />
            <span className="weather-temp">{degrees(weather.temp_c)}</span>
            <span className="weather-text">
              <span className="weather-condition">{weather.condition}</span>
              <span className="widget-caption">
                {weather.city} · {degrees(weather.high_c)} / {degrees(weather.low_c)}
              </span>
            </span>
          </div>
          <ol className="hours">
            {weather.hourly.slice(0, 5).map((hour, i) => (
              <li key={i} className="hour">
                <span className={`hour-label ${i === 0 ? "first" : ""}`}>{hour.label}</span>
                <span className="hour-temp">{degrees(hour.temp_c)}</span>
              </li>
            ))}
          </ol>
          {weather.stale && <span className="widget-caption stale">Updated a while ago</span>}
        </>
      );
  }
}
