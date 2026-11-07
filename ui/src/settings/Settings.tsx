import { useEffect, useId, useRef, useState } from "react";
import { getAppearance, setAppearance, type AccentSource, type Appearance, type Motion } from "./bridge";

const SOURCES: { id: AccentSource; label: string }[] = [
  { id: "wallpaper", label: "Wallpaper" },
  { id: "windows", label: "Windows" },
  { id: "custom", label: "Custom" },
];

const MOTIONS: { id: Motion; label: string }[] = [
  { id: "lively", label: "Lively" },
  { id: "calm", label: "Calm" },
];

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

  return (
    <main className="settings">
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
