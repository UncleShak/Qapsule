// The song in the peek (DESIGN.md sections 3 and 8): art, title, audio bars, a progress
// line, and in the hover form the artist and the previous / pause / next buttons.
// Whether the controls show, and whether the song is playing, are the core's to say.

import { type CSSProperties, type MouseEvent, useState } from "react";
import { expandClick, press, swapPeek } from "../bridge";
import type { MusicView } from "../types";
import { Glyph } from "./Glyph";

/** Cover art over a placeholder (a subtle fill with the music glyph). A missing or broken
 *  image just leaves the placeholder showing. */
function Art({ art, className }: { art: string | null; className: string }) {
  // Local presentation state only: which URL failed to load.
  const [failed, setFailed] = useState<string | null>(null);
  return (
    <span className={`art ${className}`} aria-hidden="true">
      <Glyph name="music" className="art-glyph" />
      {art && failed !== art && <img src={art} alt="" draggable={false} onError={() => setFailed(art)} />}
    </span>
  );
}

/** The thin accent line along the bottom: where the song is, moving at the song's pace.
 *  A keyframe animation does the moving, so there is no timer; a new state remounts it. */
function Progress({ music }: { music: MusicView }) {
  const { position_ms: pos, length_ms: len, rate, playing } = music;
  if (pos === null || len === null || len <= 0) return null;
  const done = Math.min(Math.max(pos / len, 0), 1);
  const moving = playing && rate > 0 && done < 1;
  const style = { "--from": done } as CSSProperties;
  if (moving) (style as Record<string, string>)["--remain"] = `${Math.round((len - pos) / rate)}ms`;
  return (
    <span className="bar" aria-hidden="true">
      <span
        key={`${pos}-${len}-${rate}-${playing}`}
        className={`bar-fill ${moving ? "bar-moving" : ""}`}
        style={{ ...style, transform: moving ? undefined : `scaleX(${done})` }}
      />
    </span>
  );
}

function Bars({ playing }: { playing: boolean }) {
  return (
    <span className={`audio-bars ${playing ? "" : "audio-paused"}`} aria-hidden="true">
      <i />
      <i />
      <i />
      <i />
    </span>
  );
}

/** The song filling the pill. `controls` is the core saying the pointer is over it. */
export function MusicPeek({ music }: { music: MusicView }) {
  const hover = music.controls;
  const press1 = (action: string) => (e: MouseEvent) => {
    e.stopPropagation();
    press(music.source, music.id, action);
  };
  return (
    <div className={`music ${hover ? "music-hover" : ""}`}>
      {/* A click on the song opens the expanded view (the core ignores it in hover mode). */}
      <button type="button" className="music-body" onClick={() => expandClick()}>
        <Art art={music.art} className={hover ? "art-hover" : "art-small"} />
        <span className="music-text">
          <span className="music-title">{music.title}</span>
          {hover && <span className="music-artist">{music.artist || music.app}</span>}
        </span>
      </button>
      {hover ? (
        <span className="music-controls">
          <button type="button" className="glyph-button media-button" aria-label="Previous" onClick={press1("media.previous")}>
            <Glyph name="previous" />
          </button>
          <button
            type="button"
            className="glyph-button media-button"
            aria-label={music.playing ? "Pause" : "Play"}
            onClick={press1("media.play_pause")}
          >
            <Glyph name={music.playing ? "pause" : "play"} />
          </button>
          <button type="button" className="glyph-button media-button" aria-label="Next" onClick={press1("media.next")}>
            <Glyph name="next" />
          </button>
        </span>
      ) : (
        <Bars playing={music.playing} />
      )}
      <Progress music={music} />
    </div>
  );
}

/** The song as the ¾ of a dual peek: art and title, with no room for bars. */
export function MusicMain({ music }: { music: MusicView }) {
  return (
    <div className="music">
      <button type="button" className="music-body" onClick={() => expandClick()}>
        <Art art={music.art} className="art-small" />
        <span className="music-text">
          <span className="music-title">{music.title}</span>
        </span>
      </button>
      <Progress music={music} />
    </div>
  );
}

/** The song as the ¼ bubble: its art as a circle, or the music glyph. Clicking swaps it with the ¾. */
export function MusicBubble({ music }: { music: MusicView }) {
  return (
    <button type="button" className="bubble" onClick={() => swapPeek()} aria-label={`Show ${music.title}`}>
      <Art art={music.art} className="art-round" />
    </button>
  );
}
