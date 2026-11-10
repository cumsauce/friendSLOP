#!/usr/bin/env python3
"""Cut the babble voice bank (crates/game/assets/audio/voice/grains.wav).

Takes CC0 speech clips (22.05 kHz mono 16-bit WAV, one per voice, in the
order given) and keeps only GRAINS short voiced syllables from each: loud,
low-noise, strongly periodic bits, sorted from dark to bright vowel. They're
written back to back, every grain exactly GRAIN_MS long, so the game finds
grain `v * GRAINS + i` at a fixed offset. Standard library only.

    python3 -I scripts/voice_grains.py out.wav a_22k.wav b_22k.wav ...
"""

import array
import math
import sys
import wave

RATE = 22050
GRAINS = 10
GRAIN_MS = 90
FRAME = RATE // 100  # 10 ms


def read(path):
    with wave.open(path, "rb") as w:
        assert w.getframerate() == RATE and w.getnchannels() == 1 and w.getsampwidth() == 2
        a = array.array("h")
        a.frombytes(w.readframes(w.getnframes()))
    return [s / 32768.0 for s in a]


def periodicity(x):
    """Best normalised autocorrelation for a 70-500 Hz pitch."""
    e = sum(v * v for v in x) or 1e-9
    best = 0.0
    for lag in range(RATE // 500, RATE // 70, 2):
        c = sum(x[i] * x[i + lag] for i in range(0, len(x) - lag, 2))
        best = max(best, 2.0 * c / e)
    return best


def brightness(x):
    """Zero-crossing rate as a cheap spectral-centroid stand-in."""
    return sum(1 for i in range(1, len(x)) if (x[i - 1] < 0) != (x[i] < 0)) / len(x)


def grains(samples):
    n = len(samples) // FRAME
    rms = [math.sqrt(sum(v * v for v in samples[i * FRAME:(i + 1) * FRAME]) / FRAME) for i in range(n)]
    loud = sorted(rms)[int(n * 0.6)]
    length = RATE * GRAIN_MS // 1000
    cands = []
    for i in range(2, n - 8):
        if rms[i] < loud or rms[i] < max(rms[i - 2:i + 3]):
            continue
        start = i * FRAME - length // 3
        if start < 0 or start + length > len(samples):
            continue
        g = samples[start:start + length]
        p = periodicity(g)
        z = brightness(g)
        if p < 0.55 or z > 0.12:  # noisy (s, f, breath): skip
            continue
        cands.append((rms[i] * p, start, z))
    cands.sort(reverse=True)
    picked = []
    for score, start, z in cands:
        if all(abs(start - s) > length for _, s, _ in picked):
            picked.append((score, start, z))
        if len(picked) == GRAINS:
            break
    if len(picked) < GRAINS:
        raise SystemExit(f"only {len(picked)} good grains")
    picked.sort(key=lambda t: t[2])  # dark to bright
    out = []
    for _, start, _ in picked:
        g = samples[start:start + length]
        peak = max(abs(v) for v in g) or 1.0
        fade_in, fade_out = RATE * 8 // 1000, RATE * 30 // 1000
        for j, v in enumerate(g):
            env = min(1.0, j / fade_in, (length - 1 - j) / fade_out)
            out.append(v / peak * 0.9 * env)
    return out


def main():
    out_path, *inputs = sys.argv[1:]
    bank = []
    for path in inputs:
        bank += grains(read(path))
        print(path, "ok")
    a = array.array("h", (int(max(-1.0, min(1.0, v)) * 32767) for v in bank))
    with wave.open(out_path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(a.tobytes())


if __name__ == "__main__":
    main()
