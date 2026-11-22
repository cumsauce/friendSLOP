//! The shared clock (#284 day/night): when the host's day began and the pure
//! maths that turns seconds into a time of day. It lives here, under the
//! features, so a feature can ask "what hour is it?" on the server (quest
//! givers who only come out at night). The sky that draws it is the game's
//! `atmosphere.rs`, which re-exports all of this.

use bevy::prelude::*;

/// When the host's session began (seconds since the epoch), replicated so every peer
/// counts the day from the same moment and the game opens in the morning.
#[derive(Component, serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct DayStart(pub f64);

/// When the session's day starts. `FRIENDSLOP_TIME=0..1` (goldens, S3) pins
/// the time of day by computing [`DayStart`] backwards so `time_of_day`
/// reads the target at session start; otherwise it is just now. Host env
/// only — [`DayStart`] replicates, so every peer counts the same day.
pub fn day_start_at() -> f64 {
    let now = crate::settings::since_epoch().as_secs_f64();
    let Some(target) = std::env::var("FRIENDSLOP_TIME")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|t| t.is_finite())
    else {
        return now;
    };
    let tuning = DAY_NIGHT.get();
    now - elapsed_for(target, tuning.day_minutes, tuning.start_hour)
}

/// `FRIENDSLOP_TIME_FREEZE=1` (with `FRIENDSLOP_TIME`) keeps the clock still at that time
/// of day for this run: the sky never moves. Local to this process, for screenshots and
/// looking around; it is not replicated.
pub fn frozen_time() -> Option<f32> {
    static FROZEN: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    *FROZEN.get_or_init(|| {
        let on =
            std::env::var("FRIENDSLOP_TIME_FREEZE").is_ok_and(|v| !matches!(v.trim(), "" | "0"));
        let t = std::env::var("FRIENDSLOP_TIME")
            .ok()?
            .trim()
            .parse::<f32>()
            .ok()?;
        (on && t.is_finite()).then(|| t.rem_euclid(1.0))
    })
}

/// Seconds after [`DayStart`] at which [`time_of_day`] reads `target` (0..1).
pub fn elapsed_for(target: f64, day_minutes: f32, start_hour: f32) -> f64 {
    let len = (day_minutes.max(0.1) as f64) * 60.0;
    let start = (start_hour as f64 / 24.0).rem_euclid(1.0);
    len * (target - start).rem_euclid(1.0)
}

/// Day/night tuning: `assets/data/game/day_night.ron` (read at run time).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct DayNight {
    /// How long one whole day lasts, in minutes.
    pub day_minutes: f32,
    /// How bright the night still is, 0..1.
    pub night_brightness: f32,
    /// The hour of day (0..24) a new game starts at.
    pub start_hour: f32,
    /// Sky colours (sRGB) at the horizon and straight up: by day, at sunset
    /// (strongest on the sun's side) and at night.
    pub day_sky: SkyPair,
    pub sunset_sky: SkyPair,
    pub night_sky: SkyPair,
    /// How many stars, and how big each is (m, on a 390 m shell).
    pub stars: u32,
    pub star_size: f32,
    /// How much of the clouds' daytime glow is left at night, 0..1.
    pub cloud_night: f32,
    /// The moon's radius (m, 380 m away).
    pub moon_size: f32,
}

/// A horizon and a zenith colour (sRGB).
#[derive(serde::Deserialize, Default, Debug, Clone, Copy)]
pub struct SkyPair {
    pub horizon: [f32; 3],
    pub zenith: [f32; 3],
}

pub static DAY_NIGHT: friendslop_data::Data<DayNight> =
    friendslop_data::Data::ron("data/game/day_night.ron");

/// Time of day 0..1 (0 = midnight, 0.25 = dawn, 0.5 = noon, 0.75 = dusk) from the seconds
/// since the session started, beginning at `start_hour` (#284). Every peer counts from the
/// host's [`DayStart`], so they agree.
pub fn time_of_day(elapsed_secs: f64, day_minutes: f32, start_hour: f32) -> f32 {
    let len = (day_minutes.max(0.1) as f64) * 60.0;
    let start = (start_hour as f64 / 24.0).rem_euclid(1.0);
    ((elapsed_secs.rem_euclid(len) / len + start).rem_euclid(1.0)) as f32
}

/// How high the sun is: -1 at midnight, 0 at dawn and dusk, 1 at noon.
pub fn sun_height(t: f32) -> f32 {
    -(t * std::f32::consts::TAU).cos()
}

/// Daylight 0..1 from the sun's height, with a soft twilight band around the horizon.
pub fn daylight(height: f32) -> f32 {
    let x = ((height + 0.15) / 0.45).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// How strongly the sky turns sunset-coloured: 1 with the sun on the horizon,
/// 0 once it's well up or well down.
pub fn dusk(height: f32) -> f32 {
    (1.0 - (height + 0.05).abs() * 3.5).clamp(0.0, 1.0)
}

/// The hour (0..24) on this peer right now, given the session's [`DayStart`]
/// (`None` before it replicates: the start hour). Honours `FRIENDSLOP_TIME_FREEZE`.
pub fn hour_now(start: Option<&DayStart>) -> f32 {
    let day = DAY_NIGHT.get();
    let now = crate::settings::since_epoch().as_secs_f64();
    let elapsed = start.map_or(0.0, |d| now - d.0);
    let t = frozen_time().unwrap_or_else(|| time_of_day(elapsed, day.day_minutes, day.start_hour));
    t * 24.0
}

/// Is `hour` inside the window `from..to` (hours 0..24)? A window that wraps
/// past midnight (21..4) works too; `from == to` is all day.
pub fn within_hours(hour: f32, (from, to): (f32, f32)) -> bool {
    let h = hour.rem_euclid(24.0);
    if from == to {
        true
    } else if from < to {
        (from..to).contains(&h)
    } else {
        h >= from || h < to
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hour_windows_wrap_past_midnight() {
        assert!(within_hours(23.0, (21.0, 4.0)));
        assert!(within_hours(2.0, (21.0, 4.0)));
        assert!(!within_hours(12.0, (21.0, 4.0)));
        assert!(within_hours(12.0, (9.0, 17.0)));
        assert!(!within_hours(18.0, (9.0, 17.0)));
        assert!(within_hours(5.0, (0.0, 0.0)));
    }
}
