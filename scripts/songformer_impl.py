#!/usr/bin/env python3
"""SongFormer section inference. Runs inside the songformer venv
(~/.local/share/earworm/songformer-venv, torch 2.4.0).

Invoked by analyze_impl.py as a subprocess. stdout = JSON array of
{"label","start","end"}; diagnostics to stderr. Everything loads from the
HF snapshot (ASLP-lab/SongFormer) — the snapshot ships its own modeling
code, musicfm package, postprocessing, and weights.
"""

import json
import os
import sys


def log(*args):
    print("songformer:", *args, file=sys.stderr, flush=True)


def main():
    audio = sys.argv[1]
    os.environ.setdefault("PYTORCH_CUDA_ALLOC_CONF", "expandable_segments:True")

    from huggingface_hub import snapshot_download

    snap = snapshot_download("ASLP-lab/SongFormer")
    log(f"snapshot at {snap}")
    # the snapshot's modeling code resolves its config/stat files via this env
    # var and imports its bundled packages (musicfm, dataset, postprocessing)
    # from the snapshot root
    os.environ["SONGFORMER_LOCAL_DIR"] = snap
    sys.path.insert(0, snap)

    # msaf (imported by the snapshot's model.py) still uses scipy.inf
    import numpy as np
    import scipy

    scipy.inf = np.inf

    import torch
    from modeling_songformer import SongFormerModel

    device = "cuda" if torch.cuda.is_available() else "cpu"
    log(f"loading SongFormer on {device}")
    model = SongFormerModel.from_pretrained(snap).to(device).eval()
    sections = model(audio)
    json.dump(sections, sys.stdout)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
