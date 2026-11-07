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
  ["--spring-hover", 0.45, 300, "hover stretch, elastic: about 20% overshoot of a few pixels"],
  ["--spring-settle", 1, 180, "shrinking back, critically damped"],
  ["--spring-collapse", 1, 220, "expanded → peek, critically damped"],
];

for (const [name, damping, ms, note] of curves) {
  console.log(`  /* ${note}: damping ${damping}, ${ms}ms */`);
  console.log(`  ${name}: ${linear(damping, ms)};`);
}

// Lively motion (decision 0012): real springs, from stiffness, damping and mass, the way
// the old app tuned them. Lively shapes run LIVELY_SCALE times longer than the calm ones
// (Capsule.tsx), so each curve is sampled over that longer time.
// Paste into the :root[data-motion="lively"] block.
const LIVELY_SCALE = 1.25;

/** x(t) of a mass on a spring released from 0 towards 1, stepped at 1/4 ms. */
function physical(stiffness: number, damping: number, mass: number): (t: number) => number {
  const dt = 0.00025;
  return (t) => {
    let x = 0;
    let v = 0;
    for (let s = 0; s < t; s += dt) {
      const a = (-stiffness * (x - 1) - damping * v) / mass;
      v += a * dt;
      x += v * dt;
    }
    return x;
  };
}

function livelyLinear(stiffness: number, damping: number, mass: number, calmMs: number): string {
  const x = physical(stiffness, damping, mass);
  const T = (calmMs * LIVELY_SCALE) / 1000;
  const values = Array.from({ length: POINTS }, (_, i) => {
    if (i === POINTS - 1) return 1;
    return Math.round(x((i / (POINTS - 1)) * T) * 1000) / 1000;
  });
  return `linear(${values.join(", ")})`;
}

// [name, stiffness, damping, mass, calm duration, note]
const lively: [string, number, number, number, number, string][] = [
  ["--spring-expand", 425, 30, 1, 280, "the notch opening: width 400/31 and height 450/29, averaged"],
  ["--spring-pop", 400, 25, 1, 240, "a pop arriving: the bouncier content spring"],
  ["--spring-replace", 600, 30, 1, 220, "one pop replacing another: quick and firm"],
  ["--spring-grow", 400, 30, 1, 220, "idle to one thing: the everyday spring"],
  ["--spring-hover", 400, 18, 1, 300, "hover stretch: elastic, a few pixels"],
  ["--spring-settle", 500, 30, 1, 180, "shrinking back: the default spring"],
  ["--spring-collapse", 500, 32, 1, 220, "closing: the default spring, a touch firmer"],
];

console.log("");
console.log(":root[data-motion=\"lively\"] {");
for (const [name, k, c, m, ms, note] of lively) {
  console.log(`  /* ${note}: stiffness ${k}, damping ${c}, mass ${m} */`);
  console.log(`  ${name}: ${livelyLinear(k, c, m, ms)};`);
}
console.log("}");
