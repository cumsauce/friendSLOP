use super::*;

fn spectrum_hi(h: &[f32; TAPS]) -> f32 {
    // Energy of the difference signal: high-frequency content.
    h.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum()
}

#[test]
fn file_round_trips_and_rejects_junk() {
    let set = synthetic(48_000);
    let bytes = set.to_bytes();
    assert_eq!(HrirSet::parse(&bytes), Some(set));
    assert!(HrirSet::parse(b"version https://git-lfs.github.com/spec/v1\n").is_none());
    assert!(HrirSet::parse(&bytes[..bytes.len() - 4]).is_none());
    assert!(HrirSet::parse(&[]).is_none());
}

#[test]
fn ahead_is_balanced_and_sides_are_not() {
    let set = synthetic(48_000);
    let ahead = set.lookup([0.0, 0.0, -1.0]);
    assert!((ahead.energy(0) - ahead.energy(1)).abs() < 1e-5);
    let right = set.lookup([1.0, 0.0, 0.0]);
    assert!(right.energy(1) > right.energy(0) * 1.5, "right ear louder");
    let left = set.lookup([-1.0, 0.0, 0.0]);
    assert!(left.energy(0) > left.energy(1) * 1.5, "left ear louder");
}

#[test]
fn elevation_changes_the_spectrum() {
    let set = synthetic(48_000);
    let level = set.lookup([0.0, 0.0, -1.0]);
    let above = set.lookup([0.0, 1.0, 0.0]);
    assert!(
        (spectrum_hi(&level.ear[0]) - spectrum_hi(&above.ear[0])).abs() > 0.1,
        "up and ahead sound different"
    );
}

#[test]
fn lookup_is_smooth_between_nodes() {
    let set = synthetic(48_000);
    let mut last = set.lookup([0.0, 0.0, -1.0]);
    for i in 1..=90 {
        let a = (i as f32).to_radians();
        let h = set.lookup([a.sin(), 0.0, -a.cos()]);
        for ear in 0..2 {
            for (x, y) in h.ear[ear].iter().zip(&last.ear[ear]) {
                assert!((x - y).abs() < 0.05, "a jump at {i} degrees");
            }
        }
        last = h;
    }
}

#[test]
fn bad_directions_do_not_blow_up() {
    let set = synthetic(48_000);
    for d in [[0.0; 3], [f32::NAN, 0.0, 0.0], [0.0, f32::INFINITY, 0.0]] {
        let h = set.lookup(d);
        assert!(h.ear.iter().flatten().all(|x| x.is_finite()));
    }
    // Straight up and down, and every way round.
    for d in [[0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]] {
        assert!(set.lookup(d).ear.iter().flatten().all(|x| x.is_finite()));
    }
}

#[test]
fn resampling_keeps_the_dc_level() {
    let mut set = synthetic(48_000);
    // A smooth bump so interpolation has something to do.
    for n in &mut set.nodes {
        for ear in &mut n.ear {
            for (k, t) in ear.iter_mut().enumerate() {
                *t = (-((k as f32 - 6.0) / 4.0).powi(2)).exp() * 0.2;
            }
        }
    }
    let dc = |h: &Hrir| h.ear[0].iter().sum::<f32>();
    let base = dc(set.node(3, 3));
    for rate in [22_050, 44_100, 16_000] {
        let r = set.resampled(rate);
        assert_eq!(r.rate(), rate);
        let got = dc(r.node(3, 3));
        assert!((got - base).abs() < base * 0.1, "{rate}: {got} vs {base}");
    }
    assert_eq!(set.resampled(48_000), set);
}

#[test]
fn fir_matches_a_direct_convolution() {
    let mut taps = [0.0; TAPS];
    for (k, t) in taps.iter_mut().enumerate() {
        *t = ((k * 7 % 11) as f32 - 5.0) * 0.01;
    }
    let input: Vec<f32> = (0..200)
        .map(|i| ((i * 13 % 17) as f32 - 8.0) * 0.1)
        .collect();
    let mut fir = Fir::default();
    for (n, &x) in input.iter().enumerate() {
        let got = fir.tick(x, &taps);
        let want: f32 = (0..TAPS)
            .map(|k| if n >= k { input[n - k] * taps[k] } else { 0.0 })
            .sum();
        assert!((got - want).abs() < 1e-4, "sample {n}: {got} vs {want}");
    }
}

#[test]
fn blending_taps_stays_finite_and_hits_the_ends() {
    let a = Hrir::passthrough().ear[0];
    let b = synthetic(48_000).lookup([1.0, 0.0, 0.0]).ear[0];
    assert_eq!(blend(&a, &b, 0.0), a);
    assert_eq!(blend(&a, &b, 1.0), b);
    assert!(blend(&a, &b, 0.5).iter().all(|x| x.is_finite()));
}

/// The shipped file, when it's really there (not an LFS pointer): the set
/// parses, straight ahead has unit power, the right is louder on the right.
#[test]
fn shipped_set_is_sane() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../game/assets/audio/hrtf/sadie_ku100.hrir"
    );
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    if !bytes.starts_with(b"HRIR") {
        return; // a Git LFS pointer
    }
    let Some(set) = HrirSet::parse(&bytes) else {
        panic!("the shipped file doesn't parse");
    };
    assert_eq!(set.rate(), 48_000);
    let ahead = set.lookup([0.0, 0.0, -1.0]);
    let power = ahead.energy(0) + ahead.energy(1);
    assert!((power - 2.0).abs() < 0.3, "{power}");
    let right = set.lookup([1.0, 0.0, 0.0]);
    assert!(right.energy(1) > right.energy(0) * 2.0);
    let left = set.lookup([-1.0, 0.0, 0.0]);
    assert!(left.energy(0) > left.energy(1) * 2.0);
    let above = set.lookup([0.0, 1.0, 0.0]);
    assert!(above.ear.iter().flatten().all(|x| x.is_finite()));
}
