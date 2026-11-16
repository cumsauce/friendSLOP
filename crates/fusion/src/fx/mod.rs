//! Bus effects and the plumbing they share: per-song mix overrides
//! ([`song_mix`]) and automation of bus settings ([`params`]).
//!
//! A new bus effect plugs in with one line in `synth.rs`'s `bus_mix!` list
//! (an `Option<...>` setting, `None` by default): the song files can then
//! override it per song and its numbers can be automated by name, as long as
//! its settings type implements [`params::Params`] (plain `f32`s, tuples of
//! them, `Option`s and `Vec`s already do; a struct uses [`crate::params!`]).

pub mod comp;
pub mod crush;
pub mod drive;
pub mod echo;
pub mod filter;
pub mod params;
pub mod phaser;
pub mod song_mix;
pub mod transient;
pub mod width;

/// A field in an override: `None` when the song doesn't write it,
/// `Some(v)` when it does (even when `v` is itself `None`, so a song can
/// switch an effect off with `chorus: None`).
#[derive(Debug, Clone, PartialEq)]
pub struct Set<T>(pub Option<T>);

impl<T> Default for Set<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Set<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        T::deserialize(d).map(|v| Set(Some(v)))
    }
}

/// Declares a bus's settings struct and its partial override (every field
/// a [`Set`]), with `merge` (override field by field) and `param`
/// (automation by name: `"trim"`, `"chorus.mix"`, `"eq.2.db"`).
///
/// A field may name its tuple's numbers (`=> [rate, depth]`) so lanes can
/// write `autopan.depth` instead of `autopan.1`.
#[macro_export]
macro_rules! bus_mix {
    (
        $(#[$m:meta])*
        pub struct $name:ident / $over:ident {
            $(
                $(#[$fm:meta])*
                $f:ident : $t:ty = $d:expr $(=> [$($n:ident),* $(,)?])?
            ),* $(,)?
        }
    ) => {
        $(#[$m])*
        #[derive(serde::Deserialize, Debug, Clone)]
        #[serde(default)]
        pub struct $name {
            $( $(#[$fm])* pub $f: $t, )*
        }

        impl Default for $name {
            fn default() -> Self {
                Self { $( $f: $d, )* }
            }
        }

        /// A song's changes to one bus (`mix:` in a song file): only the
        /// fields it writes.
        #[derive(serde::Deserialize, Default, Debug, Clone)]
        #[serde(default)]
        pub struct $over {
            $( pub $f: $crate::fx::Set<$t>, )*
        }

        impl $name {
            /// Every setting's name.
            pub const FIELDS: &'static [&'static str] = &[$(stringify!($f)),*];

            /// Copy `o` in, reusing this one's storage (no allocation on
            /// the audio thread).
            pub fn copy_from(&mut self, o: &Self) {
                $( self.$f.clone_from(&o.$f); )*
            }

            /// Lay a song's override over these settings, field by field.
            pub fn merge(&mut self, o: &$over) {
                $( if let Some(v) = &o.$f.0 { self.$f.clone_from(v); } )*
            }

            /// The number at `path` (`field` or `field.sub.sub`), if this
            /// bus has it (an effect that's off has none).
            pub fn param(&mut self, path: &str) -> Option<&mut f32> {
                let (head, rest) = path.split_once('.').unwrap_or((path, ""));
                match head {
                    $( stringify!($f) => {
                        $(
                            let names: &[&str] = &[$(stringify!($n)),*];
                            let (h, t) = rest.split_once('.').unwrap_or((rest, ""));
                            if let Some(i) = names.iter().position(|n| *n == h) {
                                return $crate::fx::params::Params::index(&mut self.$f, i, t);
                            }
                        )?
                        $crate::fx::params::Params::param(&mut self.$f, rest)
                    } )*
                    _ => None,
                }
            }
        }
    };
}
