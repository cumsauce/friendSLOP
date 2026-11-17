#!/usr/bin/env python3
"""Bake the SADIE II KU100 HRIRs into the tiny file the game loads.

    python3 -m venv venv && venv/bin/pip install numpy scipy h5py
    venv/bin/python scripts/hrtf-bake.py D1_48K_24bit_256tap_FIR_SOFA.sofa \
        crates/game/assets/audio/hrtf/sadie_ku100.hrir

Get the input from https://zenodo.org/records/12092466 (D1_HRIR_SOFA.zip,
SADIE II, University of York, Apache-2.0; credit in assets/CREDITS.md).

What it does: picks the nearest measurement for each node of a 10 degree
grid (azimuth all round, elevation -40..+90), turns every response into a
minimum-phase one (the arrival-time difference is added back by the game, per
sound, so it can glide), cuts it to TAPS with a short fade, and scales the
whole set so a sound straight ahead has unit power over both ears.

File (little endian), read by `friendslop_hrtf::HrirSet::parse`:
    "HRIR", u32 version(1), u32 rate, u32 taps, u32 az_n, u32 el_n,
    f32 az_step, f32 el_min, f32 el_step,
    f32 data[el][az][ear 0=left 1=right][tap]
Azimuth 0 is straight ahead and grows towards the listener's left (SOFA).
"""

import struct
import sys

import h5py
import numpy as np

TAPS = 48
AZ_STEP = 10.0
EL_MIN, EL_MAX, EL_STEP = -40.0, 90.0, 10.0
FFT = 1024


def unit(az, el):
    a, e = np.radians(az), np.radians(el)
    return np.stack([np.cos(e) * np.cos(a), np.cos(e) * np.sin(a), np.sin(e)], axis=-1)


def min_phase(h):
    """Same magnitude response, all the energy as early as it can be."""
    spec = np.fft.rfft(h, FFT)
    mag = np.maximum(np.abs(spec), 1e-6)
    cep = np.fft.irfft(np.log(mag), FFT)
    fold = np.zeros(FFT)
    fold[0] = cep[0]
    fold[1 : FFT // 2] = 2 * cep[1 : FFT // 2]
    fold[FFT // 2] = cep[FFT // 2]
    out = np.fft.irfft(np.exp(np.fft.rfft(fold, FFT)), FFT)
    return out


def main():
    src, dst = sys.argv[1], sys.argv[2]
    f = h5py.File(src)
    ir = f["Data.IR"][()]  # [measurement][ear][sample]
    rate = int(f["Data.SamplingRate"][()][0])
    pos = f["SourcePosition"][()]  # azimuth, elevation, radius (degrees)
    dirs = unit(pos[:, 0], pos[:, 1])

    azs = np.arange(0.0, 360.0, AZ_STEP)
    els = np.arange(EL_MIN, EL_MAX + 0.1, EL_STEP)
    fade = np.ones(TAPS)
    fade[TAPS * 3 // 4 :] = np.cos(np.linspace(0, np.pi / 2, TAPS - TAPS * 3 // 4)) ** 2

    grid = np.zeros((len(els), len(azs), 2, TAPS), dtype=np.float32)
    for i, el in enumerate(els):
        for j, az in enumerate(azs):
            k = int(np.argmax(dirs @ unit(az, el)))
            for ear in range(2):
                grid[i, j, ear] = (min_phase(ir[k, ear])[:TAPS] * fade).astype(np.float32)

    # Straight ahead: unit power over both ears.
    ahead = grid[int(np.argmin(np.abs(els))), 0]
    grid /= np.sqrt(np.sum(ahead**2) / 2.0)

    with open(dst, "wb") as out:
        out.write(b"HRIR")
        out.write(struct.pack("<IIIII", 1, rate, TAPS, len(azs), len(els)))
        out.write(struct.pack("<fff", AZ_STEP, EL_MIN, EL_STEP))
        out.write(grid.astype("<f4").tobytes())
    print(f"{dst}: {rate} Hz, {TAPS} taps, {len(els)}x{len(azs)} directions")


if __name__ == "__main__":
    main()
