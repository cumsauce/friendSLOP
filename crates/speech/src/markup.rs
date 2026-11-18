//! The writer's markup: `*emphasis*`, `...` (hesitation), `--` (short break), shouted words.

/// A piece of a line: words to say (`emph` 0 plain, 1 stressed, 2 shouted) or a silence in ms.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Chunk {
    Say { text: String, emph: u8 },
    Pause(f32),
}

const HESITATE_MS: f32 = 450.0;
const BREAK_MS: f32 = 220.0;
/// Capital words this long or longer are shouted; shorter ones are spelled out as acronyms.
const SHOUT_LETTERS: usize = 5;

pub(crate) fn parse(text: &str) -> Vec<Chunk> {
    let mut pieces: Vec<Chunk> = vec![];
    let (mut buf, mut emph) = (String::new(), 0u8);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let flush = |buf: &mut String, emph: u8, pieces: &mut Vec<Chunk>| {
        if !buf.trim().is_empty() {
            pieces.push(Chunk::Say {
                text: std::mem::take(buf),
                emph,
            });
        }
        buf.clear();
    };
    while i < chars.len() {
        let rest = &chars[i..];
        if rest.starts_with(&['.', '.', '.']) {
            flush(&mut buf, emph, &mut pieces);
            pieces.push(Chunk::Pause(HESITATE_MS));
            i += 3;
            while chars.get(i) == Some(&'.') {
                i += 1;
            }
        } else if rest.starts_with(&['-', '-']) {
            flush(&mut buf, emph, &mut pieces);
            pieces.push(Chunk::Pause(BREAK_MS));
            i += 2;
        } else if chars[i] == '*' {
            flush(&mut buf, emph, &mut pieces);
            emph = if emph == 0 { 1 } else { 0 };
            i += 1;
        } else {
            buf.push(chars[i]);
            i += 1;
        }
    }
    flush(&mut buf, emph, &mut pieces);
    let mixed = text.chars().any(|c| c.is_lowercase());
    pieces
        .into_iter()
        .flat_map(|piece| match piece {
            Chunk::Say { text, emph: 0 } if mixed => shout(&text),
            other => vec![other],
        })
        .collect()
}

fn is_shout(word: &str) -> bool {
    let letters: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
    letters.len() >= SHOUT_LETTERS && letters.iter().all(|c| c.is_uppercase())
}

/// Splits plain text so that shouted words are their own chunk.
fn shout(text: &str) -> Vec<Chunk> {
    let (mut out, mut plain) = (vec![], vec![]);
    let flush = |plain: &mut Vec<&str>, out: &mut Vec<Chunk>| {
        if !plain.is_empty() {
            out.push(Chunk::Say {
                text: plain.join(" "),
                emph: 0,
            });
            plain.clear();
        }
    };
    for word in text.split_whitespace() {
        if is_shout(word) {
            flush(&mut plain, &mut out);
            out.push(Chunk::Say {
                text: word.to_owned(),
                emph: 2,
            });
        } else {
            plain.push(word);
        }
    }
    flush(&mut plain, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn say(text: &str, emph: u8) -> Chunk {
        Chunk::Say {
            text: text.into(),
            emph,
        }
    }

    #[test]
    fn plain_text_is_one_chunk() {
        assert_eq!(parse("hello there."), vec![say("hello there.", 0)]);
        assert!(parse("").is_empty());
    }

    #[test]
    fn stars_stress_words() {
        assert_eq!(
            parse("it was *you* who did it"),
            vec![say("it was", 0), say("you", 1), say("who did it", 0)]
        );
    }

    #[test]
    fn breaks_and_hesitations() {
        assert_eq!(
            parse("well... no -- yes"),
            vec![
                say("well", 0),
                Chunk::Pause(HESITATE_MS),
                say("no", 0),
                Chunk::Pause(BREAK_MS),
                say("yes", 0)
            ]
        );
    }

    #[test]
    fn long_capitals_are_shouted_but_acronyms_are_not() {
        assert_eq!(
            parse("The FBI said STOPPING now"),
            vec![say("The FBI said", 0), say("STOPPING", 2), say("now", 0)]
        );
        assert_eq!(parse("NO STOP HELP"), vec![say("NO STOP HELP", 0)]);
    }
}
