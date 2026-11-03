import { useEffect } from "react";
import type { Palette } from "./types";

/** Puts Rust's palette on the page as CSS variables; no palette puts the tokens.css
 *  defaults back. The accents swap at once on purpose: they colour text, strokes and
 *  fills all over the page, and only opacity and transform may animate, so a smooth
 *  accent change would need every user of --accent to be drawn twice. The capsule tint,
 *  which is a single flat layer, does cross-fade (Capsule's tint layers). Depends on the
 *  colour strings, so it runs only when the palette changes, never per update. */
export function usePalette(palette: Palette | null): void {
  const accent = palette?.accent ?? null;
  const accent2 = palette?.accent_2 ?? null;
  const tint = palette?.tint ?? null;
  useEffect(() => {
    const style = document.documentElement.style;
    const set = (name: string, value: string | null) =>
      value ? style.setProperty(name, value) : style.removeProperty(name);
    set("--accent", accent);
    set("--accent-2", accent2);
    set("--tint", tint);
  }, [accent, accent2, tint]);
}
