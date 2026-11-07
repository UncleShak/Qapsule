import { useEffect, useId, useRef, useState } from "react";
import {
  getAppearance,
  setAppearance,
  type AccentSource,
  type Appearance,
  type ExpandOn,
  type Motion,
  type OpenOn,
  type PageName,
} from "./bridge";

const SOURCES: { id: AccentSource; label: string }[] = [
  { id: "wallpaper", label: "Wallpaper" },
  { id: "windows", label: "Windows" },
  { id: "custom", label: "Custom" },
];

const MOTIONS: { id: Motion; label: string }[] = [
  { id: "lively", label: "Lively" },
  { id: "calm", label: "Calm" },
];

const PAGES: { id: PageName; label: string; desc: string }[] = [
  { id: "home", label: "Home", desc: "Music, the time, what's next and your goal" },
  { id: "control", label: "Control", desc: "Volume, quick settings, battery" },
  { id: "focus", label: "Focus", desc: "Timer, tags and your week" },
  { id: "calendar", label: "Calendar", desc: "Weather, notifications and events" },
  { id: "music", label: "Music", desc: "The song, large" },
];

const OPEN_ON: { id: OpenOn; label: string }[] = [
  { id: "last", label: "Last page" },
  { id: "home", label: "First page" },
];

const EXPAND_ON: { id: ExpandOn; label: string }[] = [
  { id: "hover", label: "Hover" },
  { id: "click", label: "Click" },
];

const GOALS = [60, 120, 180, 240, 360, 480];

const HEX = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i;
const HEX6 = /^#?[0-9a-f]{6}$/i;
const BAD_COLOUR = "Use a colour like #60CDFF.";

export default function Settings() {
  const [values, setValues] = useState<Appearance | null>(null);
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const picker = useRef<HTMLInputElement>(null);
  const hexId = useId();
  const errorId = useId();

  useEffect(() => {
    getAppearance().then(setValues, () => setError("Couldn't read the settings."));
  }, []);

  const send = (patch: Partial<Appearance>) =>
    setAppearance(patch).then(
      (next) => {
        setValues(next);
        if (patch.custom_accent !== undefined) {
          setDraft(null);
          setError(null);
        }
      },
      (reason) => setError(String(reason)),
    );

  // The colour picker fires `input` for every drag; only its commit is worth saving.
  const pickerSend = useRef(send);
  pickerSend.current = send;
  const custom = values?.accent_source === "custom";
  useEffect(() => {
    const el = picker.current;
    if (!el) return;
    const commit = () => void pickerSend.current({ custom_accent: el.value });
    el.addEventListener("change", commit);
    return () => el.removeEventListener("change", commit);
  }, [custom]);

  if (!values) {
    return <main className="settings">{error && <p className="error">{error}</p>}</main>;
  }

  const text = draft ?? values.custom_accent;
  const commitText = () => {
    if (draft === null) return;
    if (HEX.test(draft.trim())) void send({ custom_accent: draft.trim() });
    else setError(BAD_COLOUR);
  };

  // Shown pages first, in their order, then the hidden ones.
  const shown = values.pages;
  const ordered = [...shown, ...PAGES.map((p) => p.id).filter((id) => !shown.includes(id))];
  const move = (id: PageName, by: number) => {
    const at = shown.indexOf(id);
    const to = at + by;
    if (at < 0 || to < 0 || to >= shown.length) return;
    const next = [...shown];
    [next[at], next[to]] = [next[to], next[at]];
    void send({ pages: next });
  };
  const toggle = (id: PageName) => {
    const next = shown.includes(id) ? shown.filter((p) => p !== id) : [...shown, id];
    if (next.length > 0) void send({ pages: next });
  };

  return (
    <main className="settings">
      <h1>Pages</h1>
      <ul className="pages" aria-label="Pages, in order">
        {ordered.map((id) => {
          const page = PAGES.find((p) => p.id === id)!;
          const on = shown.includes(id);
          const at = shown.indexOf(id);
          return (
            <li key={id} className={"page-row" + (on ? "" : " off")}>
              <span className="page-text">
                <span className="page-name">{page.label}</span>
                <span className="page-desc">{page.desc}</span>
              </span>
              <button className="move" aria-label={`Move ${page.label} left`} disabled={!on || at === 0} onClick={() => move(id, -1)}>
                ↑
              </button>
              <button
                className="move"
                aria-label={`Move ${page.label} right`}
                disabled={!on || at === shown.length - 1}
                onClick={() => move(id, 1)}
              >
                ↓
              </button>
              <input
                type="checkbox"
                role="switch"
                className="switch"
                aria-label={`Show ${page.label}`}
                checked={on}
                disabled={on && shown.length === 1}
                onChange={() => toggle(id)}
              />
            </li>
          );
        })}
      </ul>
      <Segments label="Open on" options={OPEN_ON} value={values.open_on} onPick={(id) => void send({ open_on: id })} />
      <Segments label="Open with" options={EXPAND_ON} value={values.expand_on} onPick={(id) => void send({ expand_on: id })} />
      {values.expand_on === "hover" && (
        <section className="row">
          <label className="label" htmlFor="hover-delay">
            Hover delay · {values.hover_delay_ms} ms
          </label>
          <input
            id="hover-delay"
            type="range"
            min={0}
            max={1000}
            step={50}
            value={values.hover_delay_ms}
            onChange={(e) => void send({ hover_delay_ms: Number(e.target.value) })}
          />
        </section>
      )}
      <Segments
        label="Daily focus goal"
        options={GOALS.map((m) => ({ id: m, label: `${m / 60} h` }))}
        value={values.daily_goal_min}
        onPick={(m) => void send({ daily_goal_min: m })}
      />

      <h1>Appearance</h1>

      <section className="row">
        <span className="label" id="accent-label">
          Accent
        </span>
        <div className="segments" role="radiogroup" aria-labelledby="accent-label">
          {SOURCES.map(({ id, label }) => (
            <button
              key={id}
              role="radio"
              aria-checked={values.accent_source === id}
              className="segment"
              onClick={() => void send({ accent_source: id })}
            >
              <span>{label}</span>
            </button>
          ))}
        </div>
      </section>

      {custom && (
        <section className="row">
          <label className="label" htmlFor={hexId}>
            Custom colour
          </label>
          <div className="colour">
            <input
              id={hexId}
              className={"hex" + (error ? " invalid" : "")}
              type="text"
              value={text}
              spellCheck={false}
              maxLength={7}
              aria-invalid={error !== null}
              aria-describedby={error ? errorId : undefined}
              onChange={(e) => {
                const next = e.target.value;
                setDraft(next);
                if (HEX6.test(next.trim())) void send({ custom_accent: next.trim() });
                else if (HEX.test(next.trim())) setError(null);
                else setError(BAD_COLOUR);
              }}
              onBlur={commitText}
              onKeyDown={(e) => e.key === "Enter" && commitText()}
            />
            <input
              ref={picker}
              className="swatch"
              type="color"
              aria-label="Pick a colour"
              value={values.custom_accent}
              onChange={() => {}}
            />
          </div>
          {error && (
            <p className="error" id={errorId} role="alert">
              {error}
            </p>
          )}
        </section>
      )}

      <section className="row">
        <span className="label" id="motion-label">
          Motion
        </span>
        <div className="segments" role="radiogroup" aria-labelledby="motion-label">
          {MOTIONS.map(({ id, label }) => (
            <button
              key={id}
              role="radio"
              aria-checked={values.motion === id}
              className="segment"
              onClick={() => void send({ motion: id })}
            >
              <span>{label}</span>
            </button>
          ))}
        </div>
      </section>

      <Toggle
        label="Tint capsule with wallpaper"
        checked={values.wallpaper_tint}
        onChange={(on) => void send({ wallpaper_tint: on })}
      />
      <Toggle
        label="Tint capsule with music"
        checked={values.music_tint}
        onChange={(on) => void send({ music_tint: on })}
      />
      <Toggle
        label="See-through capsule"
        checked={values.translucent}
        onChange={(on) => void send({ translucent: on })}
      />
    </main>
  );
}

function Toggle(props: { label: string; checked: boolean; onChange: (on: boolean) => void }) {
  return (
    <label className="row toggle">
      <span className="label">{props.label}</span>
      <input
        type="checkbox"
        role="switch"
        checked={props.checked}
        onChange={(e) => props.onChange(e.target.checked)}
      />
      <span className="track" aria-hidden="true">
        <span className="on" />
        <span className="knob" />
      </span>
    </label>
  );
}

function Segments<T extends string | number>(props: {
  label: string;
  options: { id: T; label: string }[];
  value: T;
  onPick: (id: T) => void;
}) {
  const id = useId();
  return (
    <section className="row">
      <span className="label" id={id}>
        {props.label}
      </span>
      <div className="segments" role="radiogroup" aria-labelledby={id}>
        {props.options.map((o) => (
          <button
            key={String(o.id)}
            role="radio"
            aria-checked={props.value === o.id}
            className="segment"
            onClick={() => props.onPick(o.id)}
          >
            <span>{o.label}</span>
          </button>
        ))}
      </div>
    </section>
  );
}
