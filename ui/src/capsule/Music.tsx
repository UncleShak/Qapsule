// The song in the peek (DESIGN.md sections 3 and 8): art, title, audio bars, a progress
// line, and in the hover form the artist and the previous / pause / next buttons.
// Whether the controls show, and whether the song is playing, are the core's to say.

import { type CSSProperties, type MouseEvent, useState } from "react";
import { expandClick, press } from "../bridge";
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

/** The song filling the pill. `hover` is the hover form: the core allows it (`controls`) and
 *  the pointer is over the pill. Both forms are the same elements, so the change between
 *  them is only CSS transitions on a class: the art scales, the title shifts up, the artist
 *  and buttons fade in and the bars fade out. */
const FRAME = {
  "--m-w": `${F.width}px`,
  "--m-h": `${F.height}px`,
  "--m-pad": `${F.pad}px`,
  "--m-art": `${F.art}px`,
  "--m-art-scale": F.artScale,
  "--m-art-dx": `${F.artDx}px`,
  "--m-title-left": `${F.titleLeft}px`,
  "--m-title-dx": `${F.titleDx}px`,
  "--m-title-w": `${F.titleWidth}px`,
  "--m-title-w-plain": `${F.titleWidthPlain}px`,
  "--m-button": `${F.button}px`,
  "--m-controls-right": `${F.controlsRight}px`,
  "--m-bars-right": `${F.barsRight}px`,
  "--m-progress-scale": F.progressScale,
  "--m-inset-x": `${F.inset}px`,
  "--m-inset-y": `${F.lift}px`,
  "--m-progress-lift": `${F.progressLift}px`,
} as CSSProperties;

export function MusicPeek({ music, hover, ghost = false }: { music: MusicView; hover: boolean; ghost?: boolean }) {
  const act = (action: string) => (e: MouseEvent) => {
    e.stopPropagation();
    press(music.source, music.id, action);
  };
  const tab = hover ? 0 : -1;
  return (
    <div className={`music music-frame ${hover ? "music-hover" : ""}`} style={FRAME}>
      {/* A click on the song opens the expanded view (the core ignores it in hover mode). */}
      <button type="button" className="music-body" onClick={() => expandClick()}>
        <Art art={music.art} className="music-art" />
        {/* Two copies of the title, each ellipsised for its own width, cross-fading. */}
        <span className="music-title music-title-plain">{music.title}</span>
        <span className="music-title music-title-hover" aria-hidden="true">
          {music.title}
        </span>
        <span className="music-artist" aria-hidden={!hover}>
          {music.artist || music.app}
        </span>
      </button>
      {!ghost && <Bars playing={music.playing} />}
      <span className="music-controls" aria-hidden={!hover}>
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
      {!ghost && <Progress music={music} className="music-progress" />}
    </div>
  );
}

/** The song as the ¾ of a dual peek: art and title, with no room for bars. */
export function MusicMain({ music }: { music: MusicView }) {
  return (
    <div className="music music-dual">
      <button type="button" className="music-body" onClick={() => expandClick()}>
        <Art art={music.art} className="art-small" />
        <span className="dual-title">{music.title}</span>
      </button>
      <Progress music={music} />
    </div>
  );
}

/** The song as the ¼ bubble: its art as a round cover that turns while the song plays, or
 *  the music glyph. A click is the caller's: swap with the ¾, or open the expanded view. */
export function MusicBubble({ music, onClick }: { music: MusicView; onClick: () => void }) {
  return (
    <button type="button" className="bubble" onClick={onClick} aria-label={`Show ${music.title}`}>
      <Art art={music.art} className={`art-round art-spin ${music.playing ? "" : "art-still"}`} />
    </button>
  );
}
