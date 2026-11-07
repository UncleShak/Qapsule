// Control (the overhaul): quick toggles, volume, and the battery and system readings.
// The readings come from the core, sampled only while this page shows. Tiles whose state
// Windows doesn't share open their Settings page instead of pretending to switch.

import type { ChangeEvent, CSSProperties } from "react";
import { openSystemPage, setVolume, toggleDnd, toggleMute, type SystemPage } from "../bridge";
import { Glyph } from "../capsule/Glyph";
import type { ExpandedView } from "../types";

const TILES: { name: string; detail: string; glyph: string; page: SystemPage }[] = [
  { name: "Wi-Fi", detail: "Networks", glyph: "wifi", page: "ms-settings:network-wifi" },
  { name: "Bluetooth", detail: "Devices", glyph: "bluetooth", page: "ms-settings:bluetooth" },
  { name: "Night light", detail: "Settings", glyph: "brightness", page: "ms-settings:nightlight" },
];

export function Control({ expanded }: { expanded: ExpandedView }) {
  const { system } = expanded;
  return (
    <div className="main-screen control-screen">
      <section className="widget control-tiles" aria-label="Quick settings">
        {TILES.map((t) => (
          <button key={t.name} type="button" className="control-tile" onClick={() => openSystemPage(t.page)}>
            <span className="control-dot">
              <Glyph name={t.glyph} />
            </span>
            <span className="control-tile-text">
              <span className="control-tile-name">{t.name}</span>
              <span className="control-tile-detail">{t.detail}</span>
            </span>
          </button>
        ))}
        <button
          type="button"
          className={`control-tile ${expanded.do_not_disturb ? "on" : ""}`}
          aria-pressed={expanded.do_not_disturb}
          onClick={() => toggleDnd()}
        >
          <span className="control-dot">
            <Glyph name="moon" />
          </span>
          <span className="control-tile-text">
            <span className="control-tile-name">Do not disturb</span>
            <span className="control-tile-detail">{expanded.do_not_disturb ? "On" : "Off"}</span>
          </span>
        </button>
      </section>

      <section className="widget control-sound" aria-label="Sound and display">
        <span className="control-label">Volume</span>
        <div className="control-slider-row">
          <button
            type="button"
            className="glyph-button control-mute"
            aria-label={system.muted ? "Unmute" : "Mute"}
            aria-pressed={system.muted}
            onClick={() => toggleMute()}
          >
            <Glyph name={system.muted ? "mute" : "volume"} />
          </button>
          <input
            type="range"
            min={0}
            max={100}
            step={1}
            aria-label="Volume"
            className={`control-slider ${system.muted ? "muted" : ""}`}
            disabled={system.volume === null}
            value={system.volume ?? 0}
            style={{ "--level": `${system.volume ?? 0}%` } as CSSProperties}
            onChange={(e: ChangeEvent<HTMLInputElement>) => setVolume(Number(e.target.value))}
          />
          <span className="control-value">{system.volume ?? "–"}</span>
        </div>
        <button type="button" className="control-link" onClick={() => openSystemPage("ms-settings:display")}>
          <Glyph name="display" />
          Display and brightness
        </button>
      </section>

      <section className="widget control-system" aria-label="Battery and system">
        {system.battery ? (
          <div className="control-battery">
            <span className="control-battery-icon" aria-hidden="true">
              <span style={{ width: `${system.battery.percent}%` }} />
            </span>
            <span className="control-battery-text">
              <span className="control-battery-percent">{system.battery.percent}%</span>
              <span className="control-tile-detail">{system.battery.charging ? "Charging" : "On battery"}</span>
            </span>
          </div>
        ) : (
          <div className="control-battery">
            <span className="control-tile-detail">Plugged in, no battery</span>
          </div>
        )}
        {[
          ["Memory", system.ram],
          ["Processor", system.cpu],
        ].map(([label, value]) => (
          <div key={label as string} className="control-meter">
            <span>{label}</span>
            <span className="control-meter-track">
              <span style={{ transform: `scaleX(${((value as number | null) ?? 0) / 100})` }} />
            </span>
            <span className="control-value">{value === null ? "–" : `${value}%`}</span>
          </div>
        ))}
      </section>
    </div>
  );
}
