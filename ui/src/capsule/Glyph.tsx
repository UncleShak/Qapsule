// Segoe Fluent Icons, which ship with Windows 11 (DESIGN.md section 8).

const GLYPHS: Record<string, string> = {
  agent: "\uE99A", // Robot
  build: "\uE756", // CommandPrompt
  download: "\uE896", // Download
  info: "\uE946", // Info
  check: "\uE73E", // CheckMark
  error: "\uE783", // Error
  chevron: "\uE76C", // ChevronRight
  close: "\uE711", // Cancel
};

export function Glyph({ name, className = "" }: { name: string; className?: string }) {
  return (
    <span className={`glyph ${className}`} aria-hidden="true">
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
