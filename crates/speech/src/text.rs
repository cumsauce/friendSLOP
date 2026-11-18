//! Text to phonemes, vendored from the `samiam` crate 0.1.3 (Christian Haynes, MIT OR Apache-2.0,
//! https://github.com/06chaynes/samiam), minus its synthesizer, which `synth.rs` replaces.
//!
//! The letter-to-sound rules follow NRL Report 7948 (Elovitz et al.). The phoneme table's formant
//! targets are samiam's, unchanged.

// Letter-to-sound rules after NRL Report 7948 as (left context, letters, right context, phonemes),
// tried in order; a letter no rule covers says itself. In a context a space is a word edge, `#` one
// or more vowels, `:` any consonants, `^` one consonant, `.` a voiced consonant, `+` E, I or Y, `@`
// a consonant that makes U say oo, `&` a sibilant and `%` a suffix such as -ED or -ING.
pub(crate) static RULES: [(&str, &str, &str, &str); 312] = [
    ("", "A", " ", "AX"),
    (" ", "ARE", " ", "AA R"),
    (" ", "AR", "O", "AX R"),
    ("", "AR", "#", "EH R"),
    ("", "A", "WA", "AX"),
    ("", "AW", "", "AO"),
    (" :", "ANY", "", "EH N IY"),
    ("", "A", "^+#", "EY"),
    ("^", "AS", "#", "EY S"),
    ("#:", "ALLY", "", "AX L IY"),
    (" ", "AL", "#", "AX L"),
    ("", "AGAIN", "", "AX G EH N"),
    ("#:", "AG", "E", "IH JH"),
    ("", "A", "^+:#", "AE"),
    (" :", "A", "^+ ", "EY"),
    ("", "A", "^%", "EY"),
    (" ", "ARR", "", "AX R"),
    ("", "ARR", "", "AE R"),
    (" :", "AR", " ", "AA R"),
    ("", "AR", " ", "ER"),
    ("", "AR", "", "AA R"),
    ("", "AIR", "", "EH R"),
    ("", "AI", "", "EY"),
    ("", "AY", "", "EY"),
    ("", "AU", "", "AO"),
    ("#:", "AL", " ", "AX L"),
    ("#:", "ALS", " ", "AX L Z"),
    ("", "ALK", "", "AO K"),
    ("", "AL", "^", "AO L"),
    (" :", "ABLE", "", "EY B AX L"),
    ("", "ABLE", "", "AX B AX L"),
    ("", "ANG", "+", "EY N JH"),
    ("", "A", "", "AE"),
    (" ", "BE", "^#", "B IX"),
    ("", "BEING", "", "B IY IH NG"),
    (" ", "BOTH", " ", "B OW TH"),
    (" ", "BUS", "#", "B IH Z"),
    ("", "BUIL", "", "B IH L"),
    (" ", "CH", "^", "K"),
    ("^E", "CH", "", "K"),
    ("", "CH", "", "CH"),
    (" S", "CI", "#", "S AY"),
    ("", "CI", "A", "SH"),
    ("", "CI", "O", "SH"),
    ("", "CI", "EN", "SH"),
    ("", "C", "+", "S"),
    ("", "CK", "", "K"),
    (" ", "COM", "PU", "K AX M"),
    ("", "COM", "%", "K AH M"),
    ("", "C", "", "K"),
    ("", "DOLLAR", "", "D AA L ER"),
    ("#:", "DED", " ", "D IH D"),
    (".E", "D", " ", "D"),
    ("#:^E", "D", " ", "T"),
    (" ", "DE", "^#", "D IX"),
    (" ", "DO", " ", "D UW"),
    (" ", "DOES", "", "D AH Z"),
    (" ", "DOING", "", "D UW IH NG"),
    (" ", "DOW", "", "D AW"),
    ("", "DU", "A", "JH UW"),
    ("#:", "E", " ", ""),
    (" :", "E", " ", "IY"),
    ("#", "ED", " ", "D"),
    ("#:", "E", "D ", ""),
    ("", "EV", "ER", "EH V"),
    ("", "E", "^%", "IY"),
    ("", "ERI", "#", "IY R IY"),
    ("", "ERI", "", "EH R IH"),
    ("#:", "ER", "#", "ER"),
    ("", "ER", "#", "EH R"),
    ("", "ER", "", "ER"),
    (" ", "EVEN", "", "IY V EH N"),
    ("#:", "E", "W", ""),
    ("@", "EW", "", "UW"),
    ("", "EW", "", "Y UW"),
    ("", "E", "O", "IY"),
    ("#:&", "ES", " ", "IH Z"),
    ("#:", "E", "S ", ""),
    ("#:", "ELY", " ", "L IY"),
    ("#:", "EMENT", "", "M EH N T"),
    ("", "EFUL", "", "F UH L"),
    ("", "EE", "", "IY"),
    ("", "EARN", "", "ER N"),
    (" ", "EAR", "^", "ER"),
    ("", "EAD", "", "EH D"),
    ("#:", "EA", " ", "IY AX"),
    ("", "EA", "SU", "EH"),
    ("", "EA", "", "IY"),
    ("", "EIGH", "", "EY"),
    ("", "EI", "", "IY"),
    (" ", "EYE", "", "AY"),
    ("", "EY", "", "IY"),
    ("", "EU", "", "Y UW"),
    ("", "E", "", "EH"),
    ("", "FUL", "", "F UH L"),
    ("", "GIV", "", "G IH V"),
    (" ", "G", "I^", "G"),
    ("", "GE", "T", "G EH"),
    ("SU", "GGES", "", "G JH EH S"),
    ("", "GG", "", "G"),
    (" B#", "G", "", "G"),
    ("", "G", "+", "JH"),
    ("", "GREAT", "", "G R EY T"),
    ("#", "GH", "", ""),
    ("", "HUNDRED", "", "HH AH N D R AX D"),
    (" ", "HAV", "", "HH AE V"),
    (" ", "HERE", "", "HH IY R"),
    (" ", "HOUR", "", "AW ER"),
    ("", "HOW", "", "HH AW"),
    ("", "H", "#", "HH"),
    ("", "H", "", ""),
    ("", "IETH", "", "IY AX TH"),
    (" ", "IN", "", "IH N"),
    (" ", "I", " ", "AY"),
    ("", "IN", "D", "AY N"),
    ("", "IER", "", "IY ER"),
    ("#:R", "IED", "", "IY D"),
    ("", "IED", " ", "AY D"),
    ("", "IEN", "", "IY EH N"),
    ("", "IE", "T", "AY EH"),
    (" :", "I", "%", "AY"),
    ("", "I", "%", "IY"),
    ("", "IE", "", "IY"),
    ("", "I", "^+:#", "IH"),
    ("", "IR", "#", "AY R"),
    ("", "IZ", "%", "AY Z"),
    ("", "IS", "%", "AY Z"),
    ("", "I", "D%", "AY"),
    ("+^", "I", "^+", "IH"),
    ("", "I", "T%", "AY"),
    ("#:^", "I", "^+", "IH"),
    ("", "I", "^I", "IH"),
    ("", "I", "^+", "AY"),
    ("", "IR", "", "ER"),
    ("", "IGH", "", "AY"),
    ("", "ILD", "", "AY L D"),
    ("", "IGN", " ", "AY N"),
    ("", "IGN", "^", "AY N"),
    ("", "IGN", "%", "AY N"),
    ("", "IQUE", "", "IY K"),
    (" :", "I", "#", "AY"),
    ("", "I", "#", "IY"),
    ("", "I", "", "IH"),
    ("", "J", "", "JH"),
    (" ", "K", "N", ""),
    ("", "LO", "C#", "L OW"),
    ("L", "L", "", ""),
    ("#:^", "L", "%", "AX L"),
    ("", "LEAD", "", "L IY D"),
    (" ", "MINUS", "", "M AY N AX S"),
    ("", "MICRO", "", "M AY K R OW"),
    ("", "MOV", "", "M UW V"),
    (" ", "NINE", "", "N AY N"),
    (" ", "NINTH", "", "N AY N TH"),
    ("E", "NG", "+", "N JH"),
    ("", "NG", "R", "NG G"),
    ("", "NG", "#", "NG G"),
    ("", "NGL", "%", "NG G AX L"),
    ("", "NG", "", "NG"),
    ("", "NK", "", "NG K"),
    (" ", "NOW", " ", "N AW"),
    ("", "OF", " ", "AX V"),
    ("", "OROUGH", "", "ER OW"),
    ("#:", "OR", " ", "ER"),
    ("#:", "ORS", " ", "ER Z"),
    ("", "OR", "", "AO R"),
    (" ", "ONE", "", "W AH N"),
    ("", "OW", "", "OW"),
    (" ", "OVER", "", "OW V ER"),
    ("", "OV", "", "AH V"),
    ("", "O", "^%", "OW"),
    ("", "O", "^EN", "OW"),
    ("", "O", "^I#", "OW"),
    ("", "OL", "D", "OW L"),
    ("", "OUGHT", "", "AO T"),
    ("", "OUGH", "", "AH F"),
    (" ", "OU", "", "AW"),
    ("H", "OU", "S#", "AW"),
    ("", "OUS", "", "AX S"),
    ("", "OUR", "", "AO R"),
    ("", "OULD", "", "UH D"),
    ("^", "OU", "^L", "AH"),
    ("", "OUP", "", "UW P"),
    ("", "OU", "I", "UW"),
    ("", "OU", "", "AW"),
    ("", "OY", "", "OY"),
    ("", "OING", "", "OW IH NG"),
    ("", "OI", "", "OY"),
    ("", "OOR", "", "AO R"),
    ("", "OOK", "", "UH K"),
    ("", "OOD", "", "UH D"),
    ("", "OO", "", "UW"),
    ("", "O", "E", "OW"),
    ("", "O", " ", "OW"),
    ("", "O", "S ", "OW"),
    ("", "OA", "", "OW"),
    (" ", "ONLY", "", "OW N L IY"),
    (" ", "ONCE", "", "W AH N S"),
    ("", "ON'T", "", "OW N T"),
    ("C", "O", "N", "AA"),
    ("", "O", "NG", "AO"),
    (" :^", "O", "N", "AH"),
    ("I", "ON", "", "AX N"),
    ("#:", "ON", " ", "AX N"),
    ("#^", "ON", "", "AX N"),
    ("", "O", "ST ", "OW"),
    ("", "OF", "^", "AO F"),
    ("", "OTHER", "", "AH DH ER"),
    ("", "OSS", " ", "AO S"),
    ("#:^", "OM", "", "AH M"),
    ("", "O", "", "AA"),
    ("", "PH", "", "F"),
    ("", "PEOP", "", "P IY P"),
    ("", "POW", "", "P AW"),
    ("", "PUT", " ", "P UH T"),
    ("", "QUAR", "", "K W AO R"),
    ("", "QU", "", "K W"),
    ("", "Q", "", "K"),
    (" ", "RE", "^#", "R IY"),
    ("", "SH", "", "SH"),
    ("#", "SION", "", "ZH AX N"),
    ("", "SOME", "", "S AH M"),
    ("", "SOFT", "", "S AO F T"),
    (" ", "SE", "LE", "S AX"),
    ("#", "SUR", "#", "ZH ER"),
    ("", "SUR", "#", "SH ER"),
    ("#", "SU", "#", "ZH UW"),
    ("#", "SSU", "#", "SH UW"),
    ("#", "SED", " ", "Z D"),
    ("#", "S", "#", "Z"),
    ("", "SAID", "", "S EH D"),
    ("^", "SION", "", "SH AX N"),
    ("", "S", "S", ""),
    (".", "S", " ", "Z"),
    ("#:.E", "S", " ", "Z"),
    ("U", "S", " ", "S"),
    ("#:^##", "S", " ", "Z"),
    ("#:^#", "S", " ", "S"),
    (" :#", "S", " ", "Z"),
    (" ", "SCH", "", "S K"),
    ("", "S", "C+", ""),
    ("#", "SM", "", "Z M"),
    ("#", "SN", "'", "Z AX N"),
    (" ", "THE", " ", "DH AX"),
    ("", "TO", " ", "T UW"),
    ("", "THAT", " ", "DH AE T"),
    (" ", "THIS", " ", "DH IH S"),
    (" ", "THEY", "", "DH EY"),
    (" ", "THERE", "", "DH EH R"),
    ("", "THER", "", "DH ER"),
    ("", "THEIR", "", "DH EH R"),
    (" ", "THAN", " ", "DH AE N"),
    (" ", "THEM", " ", "DH EH M"),
    ("", "THESE", " ", "DH IY Z"),
    (" ", "THEN", "", "DH EH N"),
    ("", "THROUGH", "", "TH R UW"),
    ("", "THOSE", "", "DH OW Z"),
    ("", "THOUGH", " ", "DH OW"),
    (" ", "THUS", "", "DH AH S"),
    ("", "TH", "", "TH"),
    ("#:", "TED", " ", "T IH D"),
    ("S", "TI", "#N", "CH"),
    ("", "TI", "O", "SH"),
    ("", "TI", "A", "SH"),
    ("", "TIEN", "", "SH AX N"),
    ("", "TUR", "#", "CH ER"),
    ("", "TU", "A", "CH UW"),
    (" ", "TWO", "", "T UW"),
    (" ", "UN", "I", "Y UW N"),
    (" ", "UN", "", "AH N"),
    (" ", "UPON", "", "AX P AO N"),
    ("@", "UR", "#", "UH R"),
    ("", "UR", "#", "Y UH R"),
    ("", "UR", "", "ER"),
    ("", "U", "^ ", "AH"),
    ("", "U", "^^", "AH"),
    ("", "UY", "", "AY"),
    (" G", "U", "#", ""),
    ("G", "U", "%", ""),
    ("G", "U", "#", "W"),
    ("#N", "U", "", "Y UW"),
    ("@", "U", "", "UW"),
    ("", "U", "", "Y UW"),
    ("", "VIEW", "", "V Y UW"),
    (" ", "WERE", "", "W ER"),
    ("", "WA", "S", "W AA"),
    ("", "WA", "T", "W AA"),
    ("", "WHERE", "", "WH EH R"),
    ("", "WHAT", "", "WH AA T"),
    ("", "WHOL", "", "HH OW L"),
    ("", "WHO", "", "HH UW"),
    ("", "WH", "", "WH"),
    ("", "WAR", "", "W AO R"),
    ("", "WOR", "^", "W ER"),
    ("", "WR", "", "R"),
    ("", "X", "", "K S"),
    ("", "YOUNG", "", "Y AH NG"),
    (" ", "YOU", "", "Y UW"),
    (" ", "YES", "", "Y EH S"),
    (" ", "Y", "", "Y"),
    ("#:^", "Y", " ", "IY"),
    ("#:^", "Y", "I", "IY"),
    (" :", "Y", " ", "AY"),
    (" :", "Y", "#", "AY"),
    (" :", "Y", "^+:#", "IH"),
    (" :", "Y", "^#", "AY"),
    ("", "Y", "", "IH"),
    (".", "'S", " ", "Z"),
    ("#:.E", "'S", " ", "Z"),
    ("#:^E", "'S", " ", "S"),
    ("#", "'S", " ", "Z"),
    ("", "'", "", ""),
];
// Per sound: milliseconds, three formants in Hz, then voicing, aspiration and frication levels
// and the frication centre in Hz.
pub(crate) const SOUNDS: [(&str, [f32; 8]); 37] = [
    ("IY", [110.0, 270.0, 2290.0, 3010.0, 1.0, 0.0, 0.0, 0.0]),
    ("IH", [80.0, 390.0, 1990.0, 2550.0, 1.0, 0.0, 0.0, 0.0]),
    ("IX", [50.0, 400.0, 1850.0, 2500.0, 1.0, 0.0, 0.0, 0.0]),
    ("EH", [90.0, 530.0, 1840.0, 2480.0, 1.0, 0.0, 0.0, 0.0]),
    ("AE", [120.0, 660.0, 1720.0, 2410.0, 1.0, 0.0, 0.0, 0.0]),
    ("AA", [120.0, 730.0, 1090.0, 2440.0, 1.0, 0.0, 0.0, 0.0]),
    ("AO", [120.0, 570.0, 840.0, 2410.0, 1.0, 0.0, 0.0, 0.0]),
    ("UH", [80.0, 440.0, 1020.0, 2240.0, 1.0, 0.0, 0.0, 0.0]),
    ("UW", [110.0, 300.0, 870.0, 2240.0, 1.0, 0.0, 0.0, 0.0]),
    ("ER", [110.0, 490.0, 1350.0, 1690.0, 1.0, 0.0, 0.0, 0.0]),
    ("AX", [60.0, 500.0, 1400.0, 2300.0, 1.0, 0.0, 0.0, 0.0]),
    ("AH", [80.0, 640.0, 1190.0, 2390.0, 1.0, 0.0, 0.0, 0.0]),
    ("L", [60.0, 330.0, 1050.0, 2880.0, 0.8, 0.0, 0.0, 0.0]),
    ("R", [60.0, 310.0, 1060.0, 1380.0, 0.8, 0.0, 0.0, 0.0]),
    ("W", [50.0, 290.0, 610.0, 2150.0, 0.8, 0.0, 0.0, 0.0]),
    ("Y", [50.0, 260.0, 2070.0, 3020.0, 0.8, 0.0, 0.0, 0.0]),
    ("M", [70.0, 280.0, 900.0, 2200.0, 0.6, 0.0, 0.0, 0.0]),
    ("N", [70.0, 280.0, 1700.0, 2600.0, 0.6, 0.0, 0.0, 0.0]),
    ("NG", [70.0, 280.0, 2300.0, 2750.0, 0.6, 0.0, 0.0, 0.0]),
    ("HH", [60.0, 500.0, 1500.0, 2500.0, 0.0, 0.5, 0.0, 0.0]),
    ("F", [90.0, 340.0, 1100.0, 2080.0, 0.0, 0.0, 0.1, 7500.0]),
    ("V", [60.0, 220.0, 1100.0, 2080.0, 0.5, 0.0, 0.07, 7500.0]),
    ("TH", [90.0, 320.0, 1290.0, 2540.0, 0.0, 0.0, 0.14, 5000.0]),
    ("DH", [50.0, 270.0, 1290.0, 2540.0, 0.5, 0.0, 0.1, 5000.0]),
    ("S", [100.0, 320.0, 1390.0, 2530.0, 0.0, 0.0, 0.39, 3900.0]),
    ("Z", [70.0, 240.0, 1390.0, 2530.0, 0.5, 0.0, 0.17, 3900.0]),
    ("SH", [100.0, 300.0, 1840.0, 2750.0, 0.0, 0.0, 0.98, 2700.0]),
    ("ZH", [70.0, 300.0, 1840.0, 2750.0, 0.5, 0.0, 0.63, 2700.0]),
    ("PC", [60.0, 400.0, 1100.0, 2150.0, 0.0, 0.0, 0.0, 0.0]),
    ("BC", [70.0, 400.0, 1100.0, 2150.0, 0.15, 0.0, 0.0, 0.0]),
    ("TC", [50.0, 400.0, 1600.0, 2600.0, 0.0, 0.0, 0.0, 0.0]),
    ("DC", [60.0, 400.0, 1600.0, 2600.0, 0.15, 0.0, 0.0, 0.0]),
    ("KC", [60.0, 300.0, 1990.0, 2850.0, 0.0, 0.0, 0.0, 0.0]),
    ("GC", [70.0, 300.0, 1990.0, 2850.0, 0.15, 0.0, 0.0, 0.0]),
    ("PB", [10.0, 400.0, 1100.0, 2150.0, 0.0, 0.0, 0.21, 1500.0]),
    ("TB", [10.0, 400.0, 1600.0, 2600.0, 0.0, 0.0, 0.42, 4000.0]),
    ("KB", [15.0, 300.0, 1990.0, 2850.0, 0.0, 0.0, 0.42, 2000.0]),
];
pub(crate) const PARTS: [(&str, &str); 14] = [
    ("EY", "EH IH"),
    ("OW", "AO UH"),
    ("AY", "AA IH"),
    ("AW", "AA UH"),
    ("OY", "AO IH"),
    ("P", "PC PB HH"),
    ("B", "BC PB"),
    ("T", "TC TB HH"),
    ("D", "DC TB"),
    ("K", "KC KB HH"),
    ("G", "GC KB"),
    ("CH", "TC SH"),
    ("JH", "DC ZH"),
    ("WH", "HH W"),
];
pub(crate) const VOWELS: [char; 5] = ['A', 'E', 'I', 'O', 'U'];
pub(crate) const PAUSES: [char; 4] = [',', '.', '!', '?'];
const ONES: [&str; 20] = [
    "ZERO",
    "ONE",
    "TWO",
    "THREE",
    "FOUR",
    "FIVE",
    "SIX",
    "SEVEN",
    "EIGHT",
    "NINE",
    "TEN",
    "ELEVEN",
    "TWELVE",
    "THIRTEEN",
    "FOURTEEN",
    "FIFTEEN",
    "SIXTEEN",
    "SEVENTEEN",
    "EIGHTEEN",
    "NINETEEN",
];
const TENS: [&str; 10] = [
    "", "", "TWENTY", "THIRTY", "FORTY", "FIFTY", "SIXTY", "SEVENTY", "EIGHTY", "NINETY",
];
const SCALES: [(u64, &str); 5] = [
    (1_000_000_000_000, "TRILLION"),
    (1_000_000_000, "BILLION"),
    (1_000_000, "MILLION"),
    (1_000, "THOUSAND"),
    (100, "HUNDRED"),
];
const ORDINALS: [(&str, &str); 8] = [
    ("ONE", "FIRST"),
    ("TWO", "SECOND"),
    ("THREE", "THIRD"),
    ("FIVE", "FIFTH"),
    ("EIGHT", "EIGHTH"),
    ("TWELVE", "TWELFTH"),
    ("Y", "IETH"),
    ("", "TH"),
];
pub(crate) const LETTERS: [&str; 26] = [
    "EY",
    "B IY",
    "S IY",
    "D IY",
    "IY",
    "EH F",
    "JH IY",
    "EY CH",
    "AY",
    "JH EY",
    "K EY",
    "EH L",
    "EH M",
    "EH N",
    "OW",
    "P IY",
    "K Y UW",
    "AA R",
    "EH S",
    "T IY",
    "Y UW",
    "V IY",
    "D AH B AX L Y UW",
    "EH K S",
    "W AY",
    "Z IY",
];
const ABBREVIATIONS: [(&str, &str); 19] = [
    ("Mr.", "MISTER"),
    ("Mrs.", "MISSUS"),
    ("Ms.", "MIZ"),
    ("Dr.", "DOCTOR"),
    ("St.", "STREET"),
    ("Jr.", "JUNIOR"),
    ("Sr.", "SENIOR"),
    ("vs.", "VERSUS"),
    ("vs", "VERSUS"),
    ("etc.", "ET CETERA"),
    ("e.g.", "FOR EXAMPLE"),
    ("i.e.", "THAT IS"),
    ("km", "KILOMETERS"),
    ("kg", "KILOGRAMS"),
    ("cm", "CENTIMETERS"),
    ("lb", "POUNDS"),
    ("lbs", "POUNDS"),
    ("oz", "OUNCES"),
    ("ft", "FEET"),
];
const LITTLE: [&str; 50] = [
    "A", "AN", "THE", "OF", "TO", "AND", "OR", "BUT", "IN", "ON", "AT", "BY", "FOR", "WITH",
    "FROM", "AS", "IS", "ARE", "WAS", "WERE", "BE", "BEEN", "AM", "IT", "ITS", "I", "YOU", "HE",
    "SHE", "WE", "THEY", "ME", "HIM", "HER", "US", "THEM", "MY", "YOUR", "HIS", "OUR", "THEIR",
    "THAT", "CAN", "WILL", "DO", "HAS", "HAVE", "HAD", "IF", "SO",
];

pub fn phonemes(text: &str) -> String {
    let mut out = vec![];
    let mut rest = text;
    // All-caps text is shouting, not acronyms.
    let shout = !text.contains(|c: char| c.is_ascii_lowercase());
    while let Some(c) = rest.chars().next() {
        let before = &text[..text.len() - rest.len()];
        let digits = rest.trim_start_matches(['$', '-']);
        let sign = &rest[..rest.len() - digits.len()];
        let len = if let Some((key, long)) = ABBREVIATIONS.iter().find(|(key, _)| leads(rest, key))
        {
            // St. follows a name or number when it means street.
            let saint = *key == "St."
                && !before.split_whitespace().next_back().is_some_and(|w| {
                    w.starts_with(|c: char| c.is_ascii_uppercase() || c.is_ascii_digit())
                });
            out.push(say(if saint { "SAINT" } else { long }));
            key.len()
        } else if digits.starts_with(|c: char| c.is_ascii_digit())
            && (!sign.starts_with('-') || !before.ends_with(|c: char| !c.is_whitespace()))
        {
            let end = digits
                .char_indices()
                .find(|&(i, c)| {
                    !c.is_ascii_digit()
                        && !(",.:".contains(c)
                            && digits[i + 1..].starts_with(|c: char| c.is_ascii_digit()))
                })
                .map_or(digits.len(), |(i, _)| i);
            let (whole, fraction) = digits[..end]
                .split_once('.')
                .unwrap_or((&digits[..end], ""));
            let after = &digits[end..];
            let percent = after.starts_with('%');
            let spell = |digits: &str| {
                let words = digits.bytes().filter(u8::is_ascii_digit);
                let words = words.map(|d| ONES[(d - b'0') as usize]);
                words.collect::<Vec<_>>().join(" ")
            };
            let ordinal = ["ST", "ND", "RD", "TH"].iter().any(|s| leads(after, s));
            let time = whole.split_once(':').is_some_and(|(_, m)| m.len() == 2);
            let mut words = match whole.replace([',', ':'], "").parse::<u64>() {
                // A time, or a year unless it ends 00 to 09: those are said as plain numbers.
                Ok(n)
                    if time
                        || whole.len() == 4
                            && fraction.is_empty()
                            && !sign.contains('$')
                            && n % 1000 >= 10 =>
                {
                    let tail = match n % 100 {
                        0 => if time { "O'CLOCK" } else { "HUNDRED" }.into(),
                        low @ 1..10 => format!("O {}", number(low)),
                        low => number(low),
                    };
                    format!("{} {tail}", number(n / 100))
                }
                Ok(n) if !whole.contains(':') => number(n),
                _ => spell(whole),
            };
            let swap = ORDINALS
                .iter()
                .find_map(|(from, to)| Some((words.strip_suffix(from)?, to)));
            if let Some((stem, to)) = swap.filter(|_| ordinal) {
                words = format!("{stem}{to}");
            }
            if !fraction.is_empty() {
                words = format!("{words} POINT {}", spell(fraction));
            }
            if sign.contains('-') {
                words = format!("MINUS {words}");
            }
            if percent {
                words += " PERCENT";
            }
            if sign.contains('$') {
                words += &format!(" DOLLAR{}", if words == "ONE" { "" } else { "S" });
            }
            out.push(say(&words));
            rest.len() - after.len() + if ordinal { 2 } else { usize::from(percent) }
        } else if c.is_ascii_alphabetic() || c == '\'' {
            let len = rest
                .find(|c: char| !c.is_ascii_alphabetic() && c != '\'')
                .unwrap_or(rest.len());
            let word = &rest[..len];
            let caps = word.bytes().all(|b| b.is_ascii_uppercase());
            let unsayable = !word.contains(VOWELS) && !word.contains('Y');
            if caps && (unsayable || len <= 4 && !shout && !LITTLE.contains(&word)) {
                out.extend(word.bytes().map(|b| stress(LETTERS[(b - b'A') as usize])));
            } else {
                out.push(say(word));
            }
            len
        } else {
            match c {
                ',' | ';' | ':' => out.push(",".into()),
                '.' | '!' | '?' => out.push(c.into()),
                _ => {}
            }
            c.len_utf8()
        };
        rest = &rest[len..];
    }
    out.retain(|p| !p.is_empty());
    out.join(" ")
}

fn leads(text: &str, key: &str) -> bool {
    text.get(..key.len())
        .is_some_and(|s| s.eq_ignore_ascii_case(key))
        && !text[key.len()..].starts_with(|c: char| c.is_ascii_alphanumeric())
}

fn say(words: &str) -> String {
    let said = words.split_whitespace().map(|word| {
        let word = word.to_ascii_uppercase();
        let phonemes = rules(&word);
        if LITTLE.contains(&word.as_str()) {
            phonemes
        } else {
            stress(&phonemes)
        }
    });
    said.collect::<Vec<_>>().join(" ")
}

fn stress(phonemes: &str) -> String {
    let mut done = false;
    let marked = phonemes.split_whitespace().map(|p| {
        let first = !done && p.starts_with(VOWELS) && !["AX", "IX"].contains(&p);
        done |= first;
        p.to_owned() + if first { "1" } else { "" }
    });
    marked.collect::<Vec<_>>().join(" ")
}

fn number(n: u64) -> String {
    let tail = |rest: u64| match rest {
        0 => String::new(),
        rest => format!(" {}", number(rest)),
    };
    if let Some((scale, name)) = SCALES.into_iter().find(|(scale, _)| n >= *scale) {
        return format!("{} {name}{}", number(n / scale), tail(n % scale));
    }
    match n as usize {
        n @ 0..20 => ONES[n].into(),
        n => format!("{}{}", TENS[n / 10], tail(n as u64 % 10)),
    }
}

fn rules(word: &str) -> String {
    let text = format!(" {word} ");
    let text = text.as_bytes();
    let (mut at, mut out) = (1, vec![]);
    while at + 1 < text.len() {
        let rule = RULES.iter().find(|(left, letters, right, _)| {
            text[at..].starts_with(letters.as_bytes())
                && fits(left.bytes().rev(), text, at as isize - 1, -1)
                && fits(right.bytes(), text, (at + letters.len()) as isize, 1)
        });
        let (len, phonemes) = rule.map_or((1, &word[at - 1..at]), |(_, letters, _, phonemes)| {
            (letters.len(), *phonemes)
        });
        out.extend(phonemes.split_whitespace());
        at += len;
    }
    out.dedup_by(|a, b| a == b && !a.starts_with(VOWELS));
    out.join(" ")
}

fn fits(context: impl Iterator<Item = u8>, text: &[u8], mut at: isize, step: isize) -> bool {
    let get = |at: isize| *text.get(at as usize).unwrap_or(&b' ');
    let vowel = |c: u8| b"AEIOUY".contains(&c);
    let consonant = |c: u8| c.is_ascii_uppercase() && !vowel(c);
    let mut context = context.peekable();
    while let Some(want) = context.next() {
        let c = get(at);
        let fits = match want {
            b' ' => !c.is_ascii_uppercase(),
            // `##` is two or more vowels.
            b'#' if context.peek() == Some(&b'#') => vowel(c),
            b'#' => {
                while vowel(get(at + step)) {
                    at += step;
                }
                vowel(c)
            }
            b':' => {
                while consonant(get(at)) {
                    at += step;
                }
                at -= step;
                true
            }
            b'^' => consonant(c),
            b'.' => b"BDVGJLMNRWZ".contains(&c),
            b'+' => b"EIY".contains(&c),
            // Left contexts only: an H counts when it ends TH, CH or SH.
            b'@' | b'&' => {
                let (letters, pairs): (&[u8], &[u8]) = match want {
                    b'@' => (b"TSRDLZNJ", b"TCS"),
                    _ => (b"SCGZXJ", b"CS"),
                };
                let pair = c == b'H' && pairs.contains(&get(at + step));
                at += if pair { step } else { 0 };
                pair || letters.contains(&c)
            }
            b'%' => ["ING", "ELY", "ER", "ES", "ED", "E"]
                .iter()
                .find(|s| {
                    text.get(at as usize..)
                        .is_some_and(|t| t.starts_with(s.as_bytes()))
                })
                .map(|s| at += s.len() as isize - 1)
                .is_some(),
            _ => c == want,
        };
        if !fits {
            return false;
        }
        at += step;
    }
    true
}

#[cfg(test)]
pub(crate) fn sounds(name: &str) -> Vec<[f32; 8]> {
    match PARTS.iter().find(|(part, _)| *part == name) {
        Some((_, parts)) => parts
            .split(' ')
            .flat_map(sounds)
            .map(|mut s| {
                s[0] *= 0.7;
                s
            })
            .collect(),
        None if name.starts_with(PAUSES) => {
            let ms = if name == "," { 150.0 } else { 400.0 };
            vec![[ms, 500.0, 1500.0, 2500.0, 0.0, 0.0, 0.0, 0.0]]
        }
        None => SOUNDS
            .iter()
            .filter(|(sound, _)| *sound == name)
            .map(|(_, s)| *s)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spells_by_rule() {
        assert_eq!(
            phonemes("The cat, a dog. Is 2 you?"),
            "DH AX K AE1 T , AX D AA1 G . IH Z T UW1 Y UW ?"
        );
    }

    #[test]
    fn spells_endings() {
        for (word, end) in [
            ("radios", "D IY OW Z"),
            ("status", "T AH S"),
            ("genius", "IY AH S"),
            ("lion", "L AY1 AX N"),
            ("Jane's", "N Z"),
            ("Joe's", "OW1 Z"),
            ("Mike's", "K S"),
            ("clothes", "TH S"),
            ("hue", "HH Y UW1"),
        ] {
            assert!(phonemes(word).ends_with(end), "{word}");
        }
    }

    #[test]
    fn reads_numbers() {
        for (written, spoken) in [
            ("1,250", "one thousand two hundred fifty"),
            ("21st", "twenty first"),
            ("30th", "thirtieth"),
            ("9th", "ninth"),
            ("1984", "nineteen eighty four"),
            ("1905", "nineteen o five"),
            ("2005", "two thousand five"),
            ("3:30", "three thirty"),
            ("12:00", "twelve o'clock"),
            ("3:1", "three one"),
            ("$1", "one dollar"),
            ("-$5", "minus five dollars"),
            ("3.14%", "three point one four percent"),
            ("-3 5-10", "minus three five ten"),
        ] {
            assert_eq!(phonemes(written), phonemes(spoken), "{written}");
        }
        assert_eq!(phonemes("ninth"), "N AY1 N TH");
    }

    #[test]
    fn expands_short_forms() {
        assert_eq!(
            phonemes("Mr. Smith, 5km"),
            phonemes("mister Smith, five kilometers")
        );
        assert_eq!(phonemes("The FBI"), "DH AX EH1 F B IY1 AY1");
        assert_eq!(phonemes("IT is"), phonemes("it is"));
        assert_eq!(phonemes("NO STOP HELP"), phonemes("no stop help"));
        assert_eq!(phonemes("use HTTPS"), phonemes("use H T T P S"));
        assert_eq!(phonemes("St. Louis"), phonemes("saint Louis"));
        assert_eq!(phonemes("Main St."), phonemes("Main street"));
    }

    #[test]
    fn every_phoneme_sounds() {
        let used = RULES
            .iter()
            .map(|(.., phonemes)| *phonemes)
            .chain(LETTERS)
            .flat_map(str::split_whitespace);
        for name in used.chain([",", ".", "!", "?"]) {
            assert!(!sounds(name).is_empty(), "{name}");
        }
    }
}
