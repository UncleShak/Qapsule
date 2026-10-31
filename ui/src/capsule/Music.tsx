// The song in the peek (decision 0010): cover, clock and audio bars; a click on the cover
// opens the media view (title, artist, previous / pause / next, progress line). Whether the
// media view is open, and whether the song is playing, are the core's to say.

import { type CSSProperties, type MouseEvent, useState } from "react";
import { expandClick, mediaClick, press, swapPeek } from "../bridge";
import type { MusicView } from "../types";
import { MUSIC_FRAME as F } from "./geometry";
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
function Progress({ music, className = "" }: { music: MusicView; className?: string }) {
  const { position_ms: pos, length_ms: len, rate, playing } = music;
  if (pos === null || len === null || len <= 0) return null;
  const done = Math.min(Math.max(pos / len, 0), 1);
  const moving = playing && rate > 0 && done < 1;
  const style = { "--from": done } as CSSProperties;
  if (moving) {
    const remain = (len - pos) / rate;
    const vars = style as Record<string, string | number>;
    vars["--remain"] = `${Math.round(remain)}ms`;
    // One step a second, capped so a very long stream doesn't get a huge step count.
    vars["--steps"] = Math.min(600, Math.max(1, Math.ceil(remain / 1000)));
  }
  return (
    <span className={`bar ${className}`} aria-hidden="true">
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

const FRAME = {
  "--m-w": `${F.width}px`,
  "--m-h": `${F.height}px`,
  "--m-pad": `${F.pad}px`,
  "--m-art": `${F.art}px`,
  "--m-art-scale": F.artScale,
  "--m-art-dx": `${F.artDx}px`,
  "--m-title-left": `${F.titleLeft}px`,
  "--m-title-w": `${F.titleWidth}px`,
  "--m-button": `${F.button}px`,
  "--m-controls-right": `${F.controlsRight}px`,
  "--m-bars-right": `${F.barsRight}px`,
  "--m-inset-x": `${F.inset}px`,
  "--m-inset-y": `${F.lift}px`,
} as CSSProperties;

/** The song filling the pill. Plain: cover, clock and bars. `open` is the media view (the
 *  core's say): the cover grows, the clock gives way to title and artist, the bars to the
 *  buttons. Both forms are the same elements, so the change is only CSS transitions on a
 *  class. A click on the cover opens or closes the media view; elsewhere it opens the
 *  expanded view. */
export function MusicPeek({ music, clock, open }: { music: MusicView; clock: string; open: boolean }) {
  const act = (action: string) => (e: MouseEvent) => {
    e.stopPropagation();
    press(music.source, music.id, action);
  };
  const tab = open ? 0 : -1;
  return (
    <div className={`music music-frame ${open ? "music-hover" : ""}`} style={FRAME}>
      <button type="button" className="music-body" onClick={() => expandClick()}>
        <span className="music-clock">{clock}</span>
        <span className="music-title">{music.title}</span>
        <span className="music-artist" aria-hidden={!open}>
          {music.artist || music.app}
        </span>
      </button>
      <button
        type="button"
        className="music-cover"
        aria-label={open ? "Close media view" : "Open media view"}
        aria-expanded={open}
        onClick={(e) => {
          e.stopPropagation();
          mediaClick();
        }}
      >
        <Art art={music.art} className="art-fill" />
      </button>
      <Bars playing={music.playing} />
      <span className="music-controls" aria-hidden={!open}>
        <span className="ctl">
          <button type="button" className="glyph-button media-button" aria-label="Previous" tabIndex={tab} onClick={act("media.previous")}>
            <Glyph name="previous" />
          </button>
        </span>
        <span className="ctl">
          <button
            type="button"
            className="glyph-button media-button"
            aria-label={music.playing ? "Pause" : "Play"}
            tabIndex={tab}
            onClick={act("media.play_pause")}
          >
            <Glyph name={music.playing ? "pause" : "play"} />
          </button>
        </span>
        <span className="ctl">
          <button type="button" className="glyph-button media-button" aria-label="Next" tabIndex={tab} onClick={act("media.next")}>
            <Glyph name="next" />
          </button>
        </span>
      </span>
      {/* Only in the media view: it mounts, and starts moving, when the view opens. */}
      {open && <Progress music={music} className="music-progress" />}
    </div>
  );
}

/** The song as the ¾ of a dual peek: cover, clock and bars, narrower. */
export function MusicMain({ music, clock }: { music: MusicView; clock: string }) {
  return (
    <div className="music music-dual" style={{ "--m-bars-right": "16px" } as CSSProperties}>
      <button type="button" className="music-body" onClick={() => expandClick()}>
        <span className="music-clock">{clock}</span>
      </button>
      <button
        type="button"
        className="dual-cover"
        aria-label="Open media view"
        onClick={(e) => {
          e.stopPropagation();
          mediaClick();
        }}
      >
        <Art art={music.art} className="art-small" />
      </button>
      <Bars playing={music.playing} />
    </div>
  );
}

/** The song as the ¼ bubble: a round, still cover (or the music glyph). Clicking swaps it with the ¾. */
export function MusicBubble({ music }: { music: MusicView }) {
  return (
    <button type="button" className="bubble" onClick={() => swapPeek()} aria-label={`Show ${music.title}`}>
      <Art art={music.art} className="art-round" />
    </button>
  );
}
