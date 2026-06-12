#!/usr/bin/env python3
"""earworm analysis: beat grid (beat_this) + section suggestions.

Run through `scripts/analyze` (which owns the venv). stdout carries exactly
one JSON object:

    {"bpm": 98.2, "beats": [...], "downbeats": [...],
     "sections": [{"label": "A", "start": 0.0, "end": 31.4}, ...],
     "engine": "beat_this+novelty"}

All diagnostics go to stderr. Sections are best-effort: any failure there
still ships the beat grid (sections: [], engine: "beat_this").
"""

import argparse
import json
import string
import sys


def log(*args):
    print("earworm-analyze:", *args, file=sys.stderr, flush=True)


def beat_grid(audio_path):
    """beats, downbeats (lists of seconds) via beat_this."""
    import torch
    from beat_this.inference import File2Beats

    device = "cuda" if torch.cuda.is_available() else "cpu"
    log(f"beat_this on {device}")
    f2b = File2Beats(checkpoint_path="final0", device=device, dbn=False)
    beats, downbeats = f2b(audio_path)
    return [float(b) for b in beats], [float(d) for d in downbeats]


def median_bpm(beats):
    import numpy as np

    if len(beats) < 2:
        return None
    return float(np.median(60.0 / np.diff(np.asarray(beats))))


def snap_to_downbeat(t, downbeats):
    return min(downbeats, key=lambda d: abs(d - t)) if downbeats else t


def novelty_boundaries(audio_path, beats):
    """Foote novelty over a beat-synced chroma+MFCC self-similarity matrix.

    Returns candidate boundary times (seconds, on beats).
    """
    import librosa
    import numpy as np

    y, sr = librosa.load(audio_path, sr=22050, mono=True)
    hop = 512
    chroma = librosa.feature.chroma_cqt(y=y, sr=sr, hop_length=hop)
    mfcc = librosa.feature.mfcc(y=y, sr=sr, hop_length=hop, n_mfcc=13)
    feats = np.vstack(
        [
            librosa.util.normalize(chroma, axis=0),
            librosa.util.normalize(mfcc, axis=0),
        ]
    )
    beat_frames = np.unique(
        np.clip(
            librosa.time_to_frames(np.asarray(beats), sr=sr, hop_length=hop),
            0,
            feats.shape[1] - 1,
        )
    )
    sync = librosa.util.sync(feats, beat_frames)
    norm = sync / (np.linalg.norm(sync, axis=0, keepdims=True) + 1e-9)
    ssm = norm.T @ norm
    n = ssm.shape[0]

    # checkerboard kernel, half-width L beats (~4 bars of 4/4), gaussian taper
    half = 16
    sign = np.concatenate([-np.ones(half), np.ones(half)])
    kernel = np.outer(sign, sign)
    taper = np.exp(-0.5 * (np.arange(-half, half) / (half / 2.0)) ** 2)
    kernel *= np.outer(taper, taper)

    padded = np.pad(ssm, half, mode="edge")
    novelty = np.array(
        [np.sum(padded[i : i + 2 * half, i : i + 2 * half] * kernel) for i in range(n)]
    )
    novelty = np.maximum(novelty, 0.0)
    novelty /= novelty.max() + 1e-9

    peaks = librosa.util.peak_pick(
        novelty,
        pre_max=8,
        post_max=8,
        pre_avg=16,
        post_avg=16,
        delta=0.05,
        wait=half,
    )
    # sync segment i starts at the i-th boundary time ([0] + beats)
    times = np.concatenate([[0.0], np.asarray(beats)])
    return [float(times[p]) for p in peaks if p < len(times)], (chroma, sr, hop)


def label_sections(bounds, chroma_ctx):
    """Label segments A B C ... reusing letters for chroma-similar segments."""
    import librosa
    import numpy as np

    chroma, sr, hop = chroma_ctx
    letters = string.ascii_uppercase
    centroids = []  # (label, mean-chroma)
    sections = []
    for start, end in bounds:
        f0 = librosa.time_to_frames(start, sr=sr, hop_length=hop)
        f1 = max(librosa.time_to_frames(end, sr=sr, hop_length=hop), f0 + 1)
        mean = chroma[:, f0:f1].mean(axis=1)
        mean = mean / (np.linalg.norm(mean) + 1e-9)
        label = None
        best = 0.0
        for known, cen in centroids:
            sim = float(mean @ cen)
            if sim > 0.92 and sim > best:
                label, best = known, sim
        if label is None:
            label = letters[len(centroids) % len(letters)]
            centroids.append((label, mean))
        sections.append({"label": label, "start": round(start, 4), "end": round(end, 4)})
    return sections


def novelty_sections(audio_path, beats, downbeats, duration):
    import numpy as np

    candidates, chroma_ctx = novelty_boundaries(audio_path, beats)
    snapped = sorted({snap_to_downbeat(t, downbeats) for t in candidates})

    # minimum section length: 4 bars (median downbeat spacing)
    bar = float(np.median(np.diff(downbeats))) if len(downbeats) > 1 else 2.0
    min_len = 4.0 * bar
    kept = []
    prev = 0.0
    for b in snapped:
        if b - prev >= min_len and duration - b >= min_len:
            kept.append(b)
            prev = b

    edges = [0.0] + kept + [duration]
    bounds = list(zip(edges[:-1], edges[1:]))
    return label_sections(bounds, chroma_ctx)


def main():
    ap = argparse.ArgumentParser(description="earworm beat + section analysis")
    ap.add_argument("audio")
    ap.add_argument("--no-sections", action="store_true")
    args = ap.parse_args()

    beats, downbeats = beat_grid(args.audio)
    bpm = median_bpm(beats)
    log(f"{len(beats)} beats, {len(downbeats)} downbeats, bpm {bpm}")

    sections = []
    engine = "beat_this"
    if not args.no_sections and beats:
        try:
            import soundfile as sf

            info = sf.info(args.audio)
            duration = round(info.frames / info.samplerate, 4)
        except Exception:
            duration = round(beats[-1], 4)
        try:
            sections = novelty_sections(args.audio, beats, downbeats, duration)
            engine = "beat_this+novelty"
            log(f"novelty sections: {len(sections)}")
        except Exception as e:  # beat grid must ship regardless
            log(f"section detection failed, shipping beat grid only: {e!r}")

    json.dump(
        {
            "bpm": round(bpm, 2) if bpm is not None else None,
            "beats": [round(b, 4) for b in beats],
            "downbeats": [round(d, 4) for d in downbeats],
            "sections": sections,
            "engine": engine,
        },
        sys.stdout,
    )
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
