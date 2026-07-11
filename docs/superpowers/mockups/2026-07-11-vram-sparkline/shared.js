// Shared fake data + row scaffolding so all four directions render the same
// three scenarios and the comparison is apples-to-apples.
const TOTAL = 16384; // MB

function series(kind) {
  const n = 220;
  const out = [];
  let v = 3900;
  for (let i = 0; i < n; i++) {
    const t = i / n;
    if (kind === "ramp") {
      // demucs-like: baseline, model load step, chunked processing wobble
      v = 3900 + (t > 0.08 ? 900 : 0) + (t > 0.14 ? 500 * Math.min(1, (t - 0.14) * 6) : 0);
      v += 140 * Math.sin(i * 0.55) + 90 * Math.sin(i * 0.13);
    } else if (kind === "nearcap") {
      v = 11800 + 2600 * Math.min(1, t * 2.2) + 300 * Math.sin(i * 0.4);
      if (t > 0.75) v = 15300 + 250 * Math.sin(i * 0.9);
    } else {
      // spiky: allocator churn
      v = 5200 + 260 * Math.sin(i * 0.09);
      if (i % 37 < 3) v += 2400;
      if (i % 61 < 2) v += 3600;
    }
    out.push(Math.max(600, Math.min(TOTAL, v)));
  }
  return out;
}

const SCENARIOS = [
  { key: "ramp", label: "typical demucs run" },
  { key: "spiky", label: "allocator churn" },
  { key: "nearcap", label: "near capacity" },
];

const gb = (mb) => (mb / 1024).toFixed(1);
const gbi = (mb) => Math.round(mb / 1024);
const lvl = (f) => (f >= 0.9 ? "hot" : f >= 0.72 ? "warm" : "ok");

// Renders the standard meter row: label | <viz from cb> | values. cb(used, el)
// draws the visualization into the .hist element.
function rows(cb) {
  const root = document.getElementById("rows");
  for (const s of SCENARIOS) {
    const used = series(s.key);
    const min = Math.min(...used);
    const peak = Math.max(...used);
    const cur = used[used.length - 1];
    const row = document.createElement("div");
    row.className = "meter";
    row.innerHTML = `
      <span class="mlabel mono">vram</span>
      <span class="hist"></span>
      <span class="vals mono">
        <span class="now ${lvl(cur / TOTAL)}">${gb(cur)} / ${gbi(TOTAL)} GB</span>
        <span class="range">${gb(min)}–${gb(peak)}</span>
      </span>`;
    const cap = document.createElement("p");
    cap.className = "caption mono";
    cap.textContent = s.label;
    root.appendChild(cap);
    root.appendChild(row);
    cb(used, row.querySelector(".hist"), { min, peak, cur });
  }
}
