//! The instruments the composer's own parts play (the kit pieces, the horn
//! section, the lead voices...), looked up by name in `instruments.ron`.
//!
//! Names intern to the same [`Inst`] forever, so this is resolved once. The
//! next step moves these choices into the score's band parts.

use std::sync::OnceLock;

use crate::synth::Inst;

pub(super) struct Picks {
    pub(super) bass: Inst,
    pub(super) bell: Inst,
    pub(super) bongo: Inst,
    pub(super) brass: Inst,
    pub(super) cajon: Inst,
    pub(super) chime: Inst,
    pub(super) clap: Inst,
    pub(super) clav: Inst,
    pub(super) claves: Inst,
    pub(super) conga: Inst,
    pub(super) crash: Inst,
    pub(super) epiano: Inst,
    pub(super) flute: Inst,
    pub(super) fretless: Inst,
    pub(super) ghost: Inst,
    pub(super) guiro: Inst,
    pub(super) guitar: Inst,
    pub(super) hat: Inst,
    pub(super) kalimba: Inst,
    pub(super) kick: Inst,
    pub(super) lead: Inst,
    pub(super) mini: Inst,
    pub(super) open_hat: Inst,
    pub(super) organ: Inst,
    pub(super) pad: Inst,
    pub(super) pan_flute: Inst,
    pub(super) poly_brass: Inst,
    pub(super) ride: Inst,
    pub(super) rim: Inst,
    pub(super) shaker: Inst,
    pub(super) snare: Inst,
    pub(super) strings: Inst,
    pub(super) swell: Inst,
    pub(super) tabla: Inst,
    pub(super) tambourine: Inst,
    pub(super) timbales: Inst,
    pub(super) tom: Inst,
    pub(super) woodblock: Inst,
}

/// The composer's picks.
pub(super) fn pk() -> &'static Picks {
    static PICKS: OnceLock<Picks> = OnceLock::new();
    PICKS.get_or_init(|| Picks {
        bass: Inst::named("Bass"),
        bell: Inst::named("Bell"),
        bongo: Inst::named("Bongo"),
        brass: Inst::named("Brass"),
        cajon: Inst::named("Cajon"),
        chime: Inst::named("Chime"),
        clap: Inst::named("Clap"),
        clav: Inst::named("Clav"),
        claves: Inst::named("Claves"),
        conga: Inst::named("Conga"),
        crash: Inst::named("Crash"),
        epiano: Inst::named("EPiano"),
        flute: Inst::named("Flute"),
        fretless: Inst::named("Fretless"),
        ghost: Inst::named("Ghost"),
        guiro: Inst::named("Guiro"),
        guitar: Inst::named("Guitar"),
        hat: Inst::named("Hat"),
        kalimba: Inst::named("Kalimba"),
        kick: Inst::named("Kick"),
        lead: Inst::named("Lead"),
        mini: Inst::named("Mini"),
        open_hat: Inst::named("OpenHat"),
        organ: Inst::named("Organ"),
        pad: Inst::named("Pad"),
        pan_flute: Inst::named("PanFlute"),
        poly_brass: Inst::named("PolyBrass"),
        ride: Inst::named("Ride"),
        rim: Inst::named("Rim"),
        shaker: Inst::named("Shaker"),
        snare: Inst::named("Snare"),
        strings: Inst::named("Strings"),
        swell: Inst::named("Swell"),
        tabla: Inst::named("Tabla"),
        tambourine: Inst::named("Tambourine"),
        timbales: Inst::named("Timbales"),
        tom: Inst::named("Tom"),
        woodblock: Inst::named("Woodblock"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every instrument the composer plays is in instruments.ron.
    #[test]
    fn picks_resolve() {
        let p = pk();
        let t = crate::inst::table();
        let all = [
            p.bass,
            p.bell,
            p.bongo,
            p.brass,
            p.cajon,
            p.chime,
            p.clap,
            p.clav,
            p.claves,
            p.conga,
            p.crash,
            p.epiano,
            p.flute,
            p.fretless,
            p.ghost,
            p.guiro,
            p.guitar,
            p.hat,
            p.kalimba,
            p.kick,
            p.lead,
            p.mini,
            p.open_hat,
            p.organ,
            p.pad,
            p.pan_flute,
            p.poly_brass,
            p.ride,
            p.rim,
            p.shaker,
            p.snare,
            p.strings,
            p.swell,
            p.tabla,
            p.tambourine,
            p.timbales,
            p.tom,
            p.woodblock,
        ];
        for i in all {
            assert!(t.get(i).is_some(), "{i:?} isn't in instruments.ron");
        }
    }
}
