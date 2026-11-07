// Music (the overhaul): the song large, with its album glow, progress and controls.

import type { MouseEvent } from "react";
import { press } from "../bridge";
import { Art, Progress } from "../capsule/Music";
import { Glyph } from "../capsule/Glyph";
import type { MusicView } from "../types";

export function MusicPage({ music }: { music: MusicView | null }) {
  if (!music) {
    return (
      <div className="music-page empty">
        <Glyph name="music" className="music-page-empty-glyph" />
        <span className="widget-note">Play something and it shows here</span>
      </div>
    );
  }
  const act = (action: string) => (e: MouseEvent) => {
    e.stopPropagation();
    press(music.source, music.id, action);
  };
  return (
    <div className="music-page">
      <div className="music-page-art">
        {music.art && <img className="music-page-glow" src={music.art} alt="" draggable={false} aria-hidden="true" />}
        <Art art={music.art} className="music-page-cover" />
      </div>
      <div className="music-page-body">
        <span className="music-page-app">{music.app}</span>
        <span className="music-page-title">{music.title}</span>
        <span className="music-page-artist">{music.artist}</span>
        <Progress music={music} className="music-page-progress" />
      </div>
      <div className="music-page-controls">
        <button type="button" className="glyph-button" aria-label="Previous" onClick={act("media.previous")}>
          <Glyph name="previous" />
        </button>
        <button
          type="button"
          className="glyph-button music-page-play"
          aria-label={music.playing ? "Pause" : "Play"}
          onClick={act("media.play_pause")}
        >
          <Glyph name={music.playing ? "pause" : "play"} />
        </button>
        <button type="button" className="glyph-button" aria-label="Next" onClick={act("media.next")}>
          <Glyph name="next" />
        </button>
      </div>
    </div>
  );
}
