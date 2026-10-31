// Segoe Fluent Icons, which ship with Windows 11 (DESIGN.md section 8).

const GLYPHS: Record<string, string> = {
  agent: "\uE99A", // Robot
  app: "\uE71D", // AllApps
  build: "\uE756", // CommandPrompt
  download: "\uE896", // Download
  info: "\uE946", // Info
  check: "\uE73E", // CheckMark
  error: "\uE783", // Error
  chevron: "\uE76C", // ChevronRight
  close: "\uE711", // Cancel
  calendar: "\uE787", // Calendar
  music: "\uE8D6", // Audio
  // Weather. Segoe Fluent Icons has no weather set, so these are the closest shapes it has.
  clear: "\uE706", // Brightness
  partly_cloudy: "\uE753", // Cloud
  cloudy: "\uE753", // Cloud
  fog: "\uE753", // Cloud
  drizzle: "\uEB42", // Drop
  rain: "\uEB42", // Drop
  snow: "\u2744\uFE0E", // Unicode snowflake, text form (not in the icon font)
  storm: "\uE945", // LightningBolt
};

export function Glyph({ name, className = "" }: { name: string; className?: string }) {
  return (
    <span className={`glyph ${name === "storm" ? "glyph-warning" : ""} ${className}`} aria-hidden="true">
      {GLYPHS[name] ?? GLYPHS.info}
    </span>
  );
}

/** The icon for an item, or a tick or cross once it has finished. */
export function stateGlyph(state: string, icon: string): string {
  if (state === "succeeded") return "check";
  if (state === "failed") return "error";
  return icon;
}
