// Prints the spring easing curves in DESIGN.md section 7 as CSS linear() values.
// Run with `bun scripts/springs.ts` and paste the output into ui/src/styles/tokens.css.

const POINTS = 29;

function spring(damping: number, durationMs: number): (t: number) => number {
  const T = durationMs / 1000;
  if (damping >= 1) {
    const w = 7.4 / T;
    return (t) => 1 - Math.exp(-w * t) * (1 + w * t);
  }
  const w = 5.3 / (damping * T);
  const wd = w * Math.sqrt(1 - damping * damping);
  return (t) =>
    1 - Math.exp(-damping * w * t) * (Math.cos(wd * t) + ((damping * w) / wd) * Math.sin(wd * t));
}

function linear(damping: number, durationMs: number): string {
  const x = spring(damping, durationMs);
  const T = durationMs / 1000;
  const values = Array.from({ length: POINTS }, (_, i) => {
    if (i === POINTS - 1) return 1;
    return Math.round(x((i / (POINTS - 1)) * T) * 1000) / 1000;
  });
  return `linear(${values.join(", ")})`;
}

const curves: [string, number, number, string][] = [
  ["--spring-expand", 0.75, 280, "peek → expanded, about 3% overshoot"],
  ["--spring-pop", 0.8, 240, "peek → pop, about 2% overshoot"],
  ["--spring-replace", 0.85, 220, "pop → pop"],
  ["--spring-grow", 0.9, 220, "idle → one thing, no visible overshoot"],
  ["--spring-settle", 1, 180, "shrinking back, critically damped"],
  ["--spring-collapse", 1, 220, "expanded → peek, critically damped"],
];

for (const [name, damping, ms, note] of curves) {
  console.log(`  /* ${note}: damping ${damping}, ${ms}ms */`);
  console.log(`  ${name}: ${linear(damping, ms)};`);
}
