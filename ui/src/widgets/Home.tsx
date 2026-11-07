// Home (the overhaul): the song on the left, the time in the middle, what's next and
// today's focus goal on the right. Every number and word comes from the core.

import type { MouseEvent } from "react";
import { press, setScreen } from "../bridge";
import { Art, Progress } from "../capsule/Music";
import { Glyph } from "../capsule/Glyph";
import type { ExpandedView, MusicView, UiState } from "../types";

export function Home({ state, expanded }: { state: UiState; expanded: ExpandedView }) {
  return (
    <div className="main-screen home-screen">
      <NowPlaying music={expanded.music} />
      <Now time={state.clock} expanded={expanded} />
      <UpNext expanded={expanded} />
    </div>
  );
}

function NowPlaying({ music }: { music: MusicView | null }) {
  if (!music) {
    return (
      <section className="widget home-music empty" aria-label="Music">
        <span className="home-art">
          <Glyph name="music" />
        </span>
        <span className="widget-note">Nothing playing</span>
      </section>
    );
  }
  const act = (action: string) => (e: MouseEvent) => {
    e.stopPropagation();
    press(music.source, music.id, action);
  };
  return (
    <section className="widget home-music" aria-label="Now playing">
      <div className="home-track">
        <Art art={music.art} className="home-art" />
        <span className="home-track-text">
          <span className="home-title">{music.title}</span>
          <span className="home-artist">{music.artist || music.app}</span>
          <span className="home-app">{music.app}</span>
        </span>
      </div>
      <Progress music={music} className="home-progress" />
      <div className="home-controls">
        <button type="button" className="glyph-button" aria-label="Previous" onClick={act("media.previous")}>
          <Glyph name="previous" />
        </button>
        <button
          type="button"
          className="glyph-button home-play"
          aria-label={music.playing ? "Pause" : "Play"}
          onClick={act("media.play_pause")}
        >
          <Glyph name={music.playing ? "pause" : "play"} />
        </button>
        <button type="button" className="glyph-button" aria-label="Next" onClick={act("media.next")}>
          <Glyph name="next" />
        </button>
      </div>
    </section>
  );
}

function Now({ time, expanded }: { time: string; expanded: ExpandedView }) {
  const w = expanded.weather;
  return (
    <section className="widget home-now" aria-label="Now">
      <span className="home-time">{time}</span>
      <span className="home-date">{expanded.date}</span>
      {w.kind === "ready" && (
        <span className="home-weather">
          <Glyph name={w.glyph} />
          {Math.round(w.temp_c)}° · {w.condition}
        </span>
      )}
    </section>
  );
}

const R = 21;
const C = 2 * Math.PI * R;

function UpNext({ expanded }: { expanded: ExpandedView }) {
  const events = expanded.events;
  const head = events.kind === "ready" ? events.head : null;
  const goal = expanded.focus.goal;
  return (
    <section className="widget home-next" aria-label="Up next and focus">
      {head ? (
        <div className="home-event">
          <span className="home-caption">{head.caption}</span>
          <span className="home-event-title">{head.title}</span>
          {head.location && <span className="home-event-where">{head.location}</span>}
        </div>
      ) : (
        <div className="home-event">
          <span className="home-caption">Next</span>
          <span className="home-event-where">Nothing else today</span>
        </div>
      )}
      <div className="home-goal">
        <svg viewBox="0 0 50 50" width="50" height="50" className={`goal-ring ${goal.met ? "met" : ""}`} aria-hidden="true">
          <circle className="ring-track" cx="25" cy="25" r={R} />
          <circle className="ring-fill" cx="25" cy="25" r={R} strokeDasharray={C} strokeDashoffset={C * (1 - goal.progress)} />
        </svg>
        <span className="home-goal-text">
          <span className="home-goal-done">{goal.done}</span>
          <span className="home-goal-of">of {goal.target} today</span>
        </span>
        <button type="button" className="home-focus" onClick={() => setScreen("focus")}>
          Focus
        </button>
      </div>
    </section>
  );
}
