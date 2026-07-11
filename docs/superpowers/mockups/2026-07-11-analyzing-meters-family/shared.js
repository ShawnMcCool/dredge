// The full analyzing-readout meters block, rendered from one generic
// trace+gauge renderer so both variants differ ONLY in scaling policy.
const NS = "http://www.w3.org/2000/svg";
const N = 220;

function gen(kind) {
  const out = [];
  for (let i = 0; i < N; i++) {
    const t = i / N;
    let v;
    if (kind === "cpu") {
      // demucs: single-threaded decode, then multi-core burst
      v = 90 + (t > 0.12 ? 190 : 0) + 45 * Math.sin(i * 0.3) + 25 * Math.sin(i * 0.07);
      if (i % 43 < 2) v += 120;
    } else if (kind === "gpu") {
      v = t < 0.1 ? 4 : 62 + 26 * Math.sin(i * 0.22) + 6 * Math.sin(i * 0.9);
    } else if (kind === "ram") {
      v = 15300 + 900 * Math.min(1, t * 1.6) + 120 * Math.sin(i * 0.15);
    } else {
      v = 3900 + (t > 0.08 ? 900 : 0) + (t > 0.14 ? 500 * Math.min(1, (t - 0.14) * 6) : 0);
      v += 140 * Math.sin(i * 0.55) + 90 * Math.sin(i * 0.13);
    }
    out.push(Math.max(0, v));
  }
  return out;
}

const fmtPct = (v) => `${Math.round(v)}%`;
const fmtGb = (v, total) => `${(v / 1024).toFixed(1)} / ${Math.round(total / 1024)} GB`;
const lvl = (f) => (f >= 0.9 ? "hot" : f >= 0.72 ? "warm" : "ok");

const METERS = [
  { key: "cpu", total: 800, fmt: fmtPct, rangeFmt: (a, b) => `${Math.round(a)}–${Math.round(b)}%` },
  { key: "gpu", total: 100, fmt: fmtPct, rangeFmt: (a, b) => `${Math.round(a)}–${Math.round(b)}%` },
  { key: "ram", total: 31744, fmt: fmtGb, rangeFmt: (a, b) => `${(a / 1024).toFixed(1)}–${(b / 1024).toFixed(1)}` },
  { key: "vram", total: 16384, fmt: fmtGb, rangeFmt: (a, b) => `${(a / 1024).toFixed(1)}–${(b / 1024).toFixed(1)}` },
];

// zoomFor(key) -> bool comes from the variant page.
function renderBlock(zoomFor) {
  const root = document.getElementById("meters");
  for (const m of METERS) {
    const used = gen(m.key);
    const min = Math.min(...used);
    const peak = Math.max(...used);
    const cur = used[used.length - 1];
    const zoom = zoomFor(m.key);

    const row = document.createElement("div");
    row.className = "meter";
    row.innerHTML = `
      <span class="mlabel mono">${m.key}</span>
      <span class="hist"></span>
      <span class="vals mono">
        <span class="now ${lvl(cur / m.total)}">${m.fmt(cur, m.total)}</span>
        <span class="range">${m.rangeFmt(min, peak)}</span>
      </span>`;
    root.appendChild(row);
    const el = row.querySelector(".hist");

    // scale window
    let lo = 0, hi = m.total;
    if (zoom) {
      const pad = (peak - min) * 0.12 + m.total * 0.003;
      lo = Math.max(0, min - pad);
      hi = Math.min(m.total, peak + pad);
    }
    const y = (u) => 100 - ((u - lo) / (hi - lo)) * 100;

    const svg = document.createElementNS(NS, "svg");
    svg.setAttribute("viewBox", `0 0 ${N} 100`);
    svg.setAttribute("preserveAspectRatio", "none");
    svg.style.width = "calc(100% - 7px)";
    const pts = used.map((u, i) => `${i},${y(u).toFixed(2)}`).join(" ");
    const area = document.createElementNS(NS, "polygon");
    area.setAttribute("points", `0,100 ${pts} ${N - 1},100`);
    area.setAttribute("fill", "color-mix(in srgb, var(--meter) 45%, transparent)");
    const line = document.createElementNS(NS, "polyline");
    line.setAttribute("points", pts);
    line.setAttribute("fill", "none");
    line.setAttribute("stroke", "color-mix(in srgb, var(--fg) 75%, transparent)");
    line.setAttribute("stroke-width", "1.2");
    line.setAttribute("vector-effect", "non-scaling-stroke");
    svg.append(area, line);
    el.appendChild(svg);

    // capacity gauge: absolute 0..total strip with the working window marked
    const g = document.createElementNS(NS, "svg");
    g.setAttribute("viewBox", "0 0 6 100");
    g.setAttribute("preserveAspectRatio", "none");
    Object.assign(g.style, { position: "absolute", right: "0", top: "0", width: "6px", height: "100%" });
    const back = document.createElementNS(NS, "rect");
    back.setAttribute("x", 0); back.setAttribute("y", 0);
    back.setAttribute("width", 6); back.setAttribute("height", 100);
    back.setAttribute("fill", "color-mix(in srgb, var(--line) 70%, transparent)");
    const win = document.createElementNS(NS, "rect");
    const f = peak / m.total;
    win.setAttribute("x", 0);
    win.setAttribute("y", 100 - (peak / m.total) * 100);
    win.setAttribute("width", 6);
    win.setAttribute("height", Math.max(2, ((peak - min) / m.total) * 100));
    win.setAttribute("fill", f >= 0.9 ? "var(--miss)" : f >= 0.72 ? "var(--accent)" : "var(--meter)");
    g.append(back, win);
    el.appendChild(g);
  }
}
