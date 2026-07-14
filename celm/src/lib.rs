//! CELM-EN1: deterministic selection inside a formally constrained language.
//!
//! A [`ChoiceState`] is supplied data. The exact decoder does not generate
//! entropy, infer agency, or assign meaning to a number. It chooses one
//! derivation from the semantic fiber selected by an [`IntentFrame`].

use sm_c::Smc;
use std::fmt;

pub const SPEC_VERSION: &str = "CELM-EN1-0.1";
pub const GRAMMAR_VERSION: &str = "celm-en1-grammar-0.1";
pub const LEXICON_VERSION: &str = "celm-en1-lexicon-0.1";

/// An arbitrary-precision unsigned integer stored in base 2^32.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BigNat {
    // Little-endian limbs; zero has no limbs.
    limbs: Vec<u32>,
}

impl BigNat {
    pub fn zero() -> Self {
        Self::default()
    }

    pub fn from_u64(value: u64) -> Self {
        if value == 0 {
            return Self::zero();
        }
        let mut limbs = vec![value as u32];
        if value >> 32 != 0 {
            limbs.push((value >> 32) as u32);
        }
        Self { limbs }
    }

    pub fn from_hex(digits: &str) -> Result<Self, Error> {
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::InvalidNumber(
                "hex digits must be non-empty and contain no prefix or separators",
            ));
        }
        let mut number = Self::zero();
        for byte in digits.bytes() {
            let digit = match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                b'A'..=b'F' => byte - b'A' + 10,
                _ => unreachable!(),
            };
            number.mul_add_small(16, digit as u32);
        }
        Ok(number)
    }

    pub fn bit_len(&self) -> u32 {
        self.limbs.last().map_or(0, |last| {
            ((self.limbs.len() - 1) as u32) * 32 + (32 - last.leading_zeros())
        })
    }

    pub fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    pub fn to_hex(&self) -> String {
        let Some(last) = self.limbs.last() else {
            return "0".into();
        };
        let mut output = format!("{last:X}");
        for limb in self.limbs[..self.limbs.len() - 1].iter().rev() {
            output.push_str(&format!("{limb:08X}"));
        }
        output
    }

    pub fn div_rem_small(&mut self, divisor: u32) -> u32 {
        assert!(divisor > 0, "division by zero");
        let mut remainder = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let value = (remainder << 32) | *limb as u64;
            *limb = (value / divisor as u64) as u32;
            remainder = value % divisor as u64;
        }
        self.normalize();
        remainder as u32
    }

    fn mul_add_small(&mut self, multiplier: u32, addend: u32) {
        let mut carry = addend as u64;
        for limb in &mut self.limbs {
            let value = *limb as u64 * multiplier as u64 + carry;
            *limb = value as u32;
            carry = value >> 32;
        }
        if carry != 0 {
            self.limbs.push(carry as u32);
        }
        self.normalize();
    }

    fn normalize(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
    }
}

impl fmt::Display for BigNat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_zero() {
            return write!(formatter, "0");
        }
        let mut number = self.clone();
        let mut groups = Vec::new();
        while !number.is_zero() {
            groups.push(number.div_rem_small(1_000_000_000));
        }
        write!(formatter, "{}", groups.pop().unwrap())?;
        for group in groups.iter().rev() {
            write!(formatter, "{group:09}")?;
        }
        Ok(())
    }
}

/// Canonical framing preserves leading-zero capacity through `declared_bits`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChoiceState {
    pub declared_bits: u32,
    pub integer: BigNat,
}

impl ChoiceState {
    pub fn new(declared_bits: u32, integer: BigNat) -> Result<Self, Error> {
        if integer.bit_len() > declared_bits {
            return Err(Error::InvalidNumber("integer exceeds declared bit length"));
        }
        Ok(Self {
            declared_bits,
            integer,
        })
    }

    pub fn from_hex(declared_bits: u32, digits: &str) -> Result<Self, Error> {
        Self::new(declared_bits, BigNat::from_hex(digits)?)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SpeechAct {
    Assert,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Entity {
    Door,
    Gate,
    Window,
    System,
    Engine,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Predicate {
    Open,
    Closed,
    Ready,
    Active,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Tense {
    Past,
    Present,
    Future,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Polarity {
    Positive,
    Negative,
}

/// The normalized meaning used by the first CELM-EN1 profile.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct IntentFrame {
    pub speech_act: SpeechAct,
    pub subject: Entity,
    pub predicate: Predicate,
    pub tense: Tense,
    pub polarity: Polarity,
}

impl IntentFrame {
    pub fn assertion(
        subject: Entity,
        predicate: Predicate,
        tense: Tense,
        polarity: Polarity,
    ) -> Self {
        Self {
            speech_act: SpeechAct::Assert,
            subject,
            predicate,
            tense,
            polarity,
        }
    }

    pub fn door_open() -> Self {
        Self::assertion(
            Entity::Door,
            Predicate::Open,
            Tense::Present,
            Polarity::Positive,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Token {
    pub id: u32,
    pub text: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Derivation {
    pub meaning: IntentFrame,
    pub subject_token: Token,
    pub verb_token: Token,
    pub adverb_token: Token,
    pub predicate_token: Token,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceEvent {
    pub field: &'static str,
    pub radix: u32,
    pub index: u32,
    pub candidate_id: u32,
    pub residual_before: String,
    pub residual_after: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Output {
    pub sentence: String,
    pub derivation: Derivation,
    pub residual: BigNat,
    pub rank: u64,
    pub fiber_size: u64,
    pub declared_bits: u32,
    pub trace: Vec<TraceEvent>,
    pub spec_version: &'static str,
    pub grammar_version: &'static str,
    pub lexicon_version: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidNumber(&'static str),
    NoRealization(&'static str),
    VerificationFailure,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNumber(message) => write!(formatter, "invalid choice state: {message}"),
            Self::NoRealization(message) => write!(formatter, "no realization: {message}"),
            Self::VerificationFailure => {
                write!(formatter, "generated derivation failed exact verification")
            }
        }
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy)]
struct SubjectForm {
    entity: Entity,
    token: Token,
}

#[derive(Clone, Copy)]
struct PredicateForm {
    predicate: Predicate,
    token: Token,
}

#[derive(Clone, Copy)]
struct VerbForm {
    tense: Tense,
    token: Token,
}

const SUBJECT_FORMS: &[SubjectForm] = &[
    SubjectForm {
        entity: Entity::Door,
        token: Token {
            id: 2871,
            text: "the door",
        },
    },
    SubjectForm {
        entity: Entity::Door,
        token: Token {
            id: 3011,
            text: "the doorway",
        },
    },
    SubjectForm {
        entity: Entity::Door,
        token: Token {
            id: 3010,
            text: "the entrance",
        },
    },
    SubjectForm {
        entity: Entity::Gate,
        token: Token {
            id: 3100,
            text: "the gate",
        },
    },
    SubjectForm {
        entity: Entity::Gate,
        token: Token {
            id: 3101,
            text: "the gateway",
        },
    },
    SubjectForm {
        entity: Entity::Window,
        token: Token {
            id: 3200,
            text: "the window",
        },
    },
    SubjectForm {
        entity: Entity::System,
        token: Token {
            id: 3300,
            text: "the system",
        },
    },
    SubjectForm {
        entity: Entity::System,
        token: Token {
            id: 3301,
            text: "the service",
        },
    },
    SubjectForm {
        entity: Entity::Engine,
        token: Token {
            id: 3400,
            text: "the engine",
        },
    },
    SubjectForm {
        entity: Entity::Engine,
        token: Token {
            id: 3401,
            text: "the runtime",
        },
    },
];

const PREDICATE_FORMS: &[PredicateForm] = &[
    PredicateForm {
        predicate: Predicate::Open,
        token: Token {
            id: 422,
            text: "open",
        },
    },
    PredicateForm {
        predicate: Predicate::Closed,
        token: Token {
            id: 423,
            text: "closed",
        },
    },
    PredicateForm {
        predicate: Predicate::Ready,
        token: Token {
            id: 424,
            text: "ready",
        },
    },
    PredicateForm {
        predicate: Predicate::Active,
        token: Token {
            id: 425,
            text: "active",
        },
    },
];

const VERB_FORMS: &[VerbForm] = &[
    VerbForm {
        tense: Tense::Present,
        token: Token {
            id: 1041,
            text: "is",
        },
    },
    VerbForm {
        tense: Tense::Present,
        token: Token {
            id: 991,
            text: "remains",
        },
    },
    VerbForm {
        tense: Tense::Past,
        token: Token {
            id: 1043,
            text: "was",
        },
    },
    VerbForm {
        tense: Tense::Past,
        token: Token {
            id: 992,
            text: "remained",
        },
    },
    VerbForm {
        tense: Tense::Future,
        token: Token {
            id: 1044,
            text: "will be",
        },
    },
    VerbForm {
        tense: Tense::Future,
        token: Token {
            id: 993,
            text: "will remain",
        },
    },
];

const ADVERB_FORMS: &[Token] = &[
    Token { id: 0, text: "" },
    // CELM-EN1 interprets "indeed" as an emphasis-only identity modifier.
    Token {
        id: 804,
        text: "indeed",
    },
];

const FREE_SUBJECTS: &[Token] = &[
    Token { id: 5001, text: "the engine" },
    Token { id: 5002, text: "the system" },
    Token { id: 5003, text: "the runtime" },
    Token { id: 5004, text: "the service" },
    Token { id: 5005, text: "the network" },
    Token { id: 5006, text: "the process" },
    Token { id: 5007, text: "the device" },
    Token { id: 5008, text: "the mechanism" },
];

const FREE_VERBS: &[Token] = &[
    Token { id: 5101, text: "operates" },
    Token { id: 5102, text: "functions" },
    Token { id: 5103, text: "responds" },
    Token { id: 5104, text: "continues" },
];

const FREE_ADVERBS: &[Token] = &[
    Token { id: 5201, text: "steadily" },
    Token { id: 5202, text: "reliably" },
    Token { id: 5203, text: "quietly" },
    Token { id: 5204, text: "independently" },
    Token { id: 5205, text: "continuously" },
    Token { id: 5206, text: "efficiently" },
];

// Every adjunct is exactly two orthographic words. This gives the grammar an
// exact length construction without filler tokens or truncation.
const FREE_ADJUNCTS: &[Token] = &[
    Token { id: 5301, text: "with precision" },
    Token { id: 5302, text: "during daylight" },
    Token { id: 5303, text: "under supervision" },
    Token { id: 5304, text: "without interruption" },
    Token { id: 5305, text: "across regions" },
    Token { id: 5306, text: "through coordination" },
    Token { id: 5307, text: "after inspection" },
    Token { id: 5308, text: "before sunrise" },
    Token { id: 5309, text: "inside facilities" },
    Token { id: 5310, text: "near headquarters" },
    Token { id: 5311, text: "within parameters" },
    Token { id: 5312, text: "beside operators" },
    Token { id: 5313, text: "using safeguards" },
    Token { id: 5314, text: "toward completion" },
    Token { id: 5315, text: "despite pressure" },
    Token { id: 5316, text: "for stability" },
];

pub const MIN_GENERATED_WORDS: usize = 3;
pub const MAX_GENERATED_WORDS: usize = 4096;

/// A meaning chosen by the number rather than supplied by a controller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedMeaning {
    pub subject: Token,
    pub verb: Token,
    pub manner: Option<Token>,
    pub adjuncts: Vec<Token>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedOutput {
    pub sentence: String,
    pub meaning: GeneratedMeaning,
    pub requested_words: usize,
    pub declared_bits: u32,
    pub residual: BigNat,
    pub trace: Vec<TraceEvent>,
    pub spec_version: &'static str,
    pub grammar_version: &'static str,
    pub lexicon_version: &'static str,
}

/// The immutable CELM-EN1 package exposed as queryable profile data.
#[derive(Clone, Copy, Debug, Default)]
pub struct Profile;

impl Profile {
    pub fn en1() -> Self {
        Self
    }

    pub fn versions(&self) -> (&'static str, &'static str, &'static str) {
        (SPEC_VERSION, GRAMMAR_VERSION, LEXICON_VERSION)
    }

    pub fn fiber_size(&self, intent: &IntentFrame) -> Result<u64, Error> {
        self.validate_intent(intent)?;
        let subjects = self.subjects(intent.subject).count() as u64;
        let verbs = self.verbs(intent.tense).count() as u64;
        Ok(subjects * verbs * ADVERB_FORMS.len() as u64)
    }

    fn validate_intent(&self, intent: &IntentFrame) -> Result<(), Error> {
        if !self.subjects(intent.subject).any(|_| true) {
            return Err(Error::NoRealization("subject has no lexical form"));
        }
        if !self.verbs(intent.tense).any(|_| true) {
            return Err(Error::NoRealization("tense has no verb form"));
        }
        if self.predicate(intent.predicate).is_none() {
            return Err(Error::NoRealization("predicate has no lexical form"));
        }
        Ok(())
    }

    fn subjects(&self, entity: Entity) -> impl Iterator<Item = Token> {
        SUBJECT_FORMS
            .iter()
            .filter(move |form| form.entity == entity)
            .map(|form| form.token)
    }

    fn verbs(&self, tense: Tense) -> impl Iterator<Item = Token> {
        VERB_FORMS
            .iter()
            .filter(move |form| form.tense == tense)
            .map(|form| form.token)
    }

    fn predicate(&self, predicate: Predicate) -> Option<Token> {
        PREDICATE_FORMS
            .iter()
            .find(|form| form.predicate == predicate)
            .map(|form| form.token)
    }
}

fn select(
    residual: &mut BigNat,
    field: &'static str,
    candidates: &[Token],
    trace: &mut Vec<TraceEvent>,
) -> (Token, u32) {
    let before = residual.to_string();
    let index = residual.div_rem_small(candidates.len() as u32);
    let after = residual.to_string();
    let selected = candidates[index as usize];
    trace.push(TraceEvent {
        field,
        radix: candidates.len() as u32,
        index,
        candidate_id: selected.id,
        residual_before: before,
        residual_after: after,
    });
    (selected, index)
}

fn render(derivation: &Derivation) -> String {
    let negative = derivation.meaning.polarity == Polarity::Negative;
    let adverb = derivation.adverb_token.text;
    let modifier = if adverb.is_empty() {
        String::new()
    } else {
        format!(" {adverb}")
    };
    let verb_phrase = match (derivation.verb_token.id, negative) {
        (1041, false) => format!("is{modifier}"),
        (1041, true) => format!("is{modifier} not"),
        (991, false) => format!("{adverb} remains").trim_start().to_owned(),
        (991, true) => format!("{adverb} does not remain").trim_start().to_owned(),
        (1043, false) => format!("was{modifier}"),
        (1043, true) => format!("was{modifier} not"),
        (992, false) => format!("{adverb} remained").trim_start().to_owned(),
        (992, true) => format!("{adverb} did not remain").trim_start().to_owned(),
        (1044, false) => format!("will{modifier} be"),
        (1044, true) => format!("will{modifier} not be"),
        (993, false) => format!("will{modifier} remain"),
        (993, true) => format!("will{modifier} not remain"),
        _ => unreachable!("verifier rejects unknown verb IDs"),
    };
    let mut sentence = format!(
        "{} {} {}",
        derivation.subject_token.text, verb_phrase, derivation.predicate_token.text
    );
    if let Some(first) = sentence.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    sentence.push('.');
    sentence
}

/// Deterministically selects and renders one derivation in the semantic fiber.
pub fn decode(choice: &ChoiceState, intent: &IntentFrame) -> Result<Output, Error> {
    decode_with_profile(choice, intent, &Profile::en1())
}

pub fn decode_with_profile(
    choice: &ChoiceState,
    intent: &IntentFrame,
    profile: &Profile,
) -> Result<Output, Error> {
    profile.validate_intent(intent)?;
    let subjects: Vec<_> = profile.subjects(intent.subject).collect();
    let verbs: Vec<_> = profile.verbs(intent.tense).collect();
    let predicate = profile.predicate(intent.predicate).unwrap();

    let mut residual = choice.integer.clone();
    let mut trace = Vec::new();
    let (subject_token, subject_index) = select(&mut residual, "subject", &subjects, &mut trace);
    let (verb_token, verb_index) = select(&mut residual, "verb", &verbs, &mut trace);
    let (adverb_token, adverb_index) = select(&mut residual, "adverb", ADVERB_FORMS, &mut trace);

    // This is the rank induced by the normative least-significant-choice-first walk.
    let rank = subject_index as u64
        + subjects.len() as u64 * (verb_index as u64 + verbs.len() as u64 * adverb_index as u64);
    let fiber_size = subjects.len() as u64 * verbs.len() as u64 * ADVERB_FORMS.len() as u64;
    let derivation = Derivation {
        meaning: intent.clone(),
        subject_token,
        verb_token,
        adverb_token,
        predicate_token: predicate,
    };
    let sentence = render(&derivation);
    let output = Output {
        sentence,
        derivation,
        residual,
        rank,
        fiber_size,
        declared_bits: choice.declared_bits,
        trace,
        spec_version: SPEC_VERSION,
        grammar_version: GRAMMAR_VERSION,
        lexicon_version: LEXICON_VERSION,
    };
    if !verify_with_profile(&output, intent, profile) {
        return Err(Error::VerificationFailure);
    }
    Ok(output)
}

/// Replays the derivation against the frozen profile, then compares meanings.
pub fn verify(output: &Output, intent: &IntentFrame) -> bool {
    verify_with_profile(output, intent, &Profile::en1())
}

pub fn verify_with_profile(output: &Output, intent: &IntentFrame, profile: &Profile) -> bool {
    if output.spec_version != SPEC_VERSION
        || output.grammar_version != GRAMMAR_VERSION
        || output.lexicon_version != LEXICON_VERSION
        || output.derivation.meaning != *intent
    {
        return false;
    }
    let derivation = &output.derivation;
    let subject_valid = profile
        .subjects(intent.subject)
        .any(|token| token == derivation.subject_token);
    let verb_valid = profile
        .verbs(intent.tense)
        .any(|token| token == derivation.verb_token);
    let predicate_valid = profile.predicate(intent.predicate) == Some(derivation.predicate_token);
    let adverb_valid = ADVERB_FORMS.contains(&derivation.adverb_token);
    subject_valid
        && verb_valid
        && predicate_valid
        && adverb_valid
        && render(derivation) == output.sentence
}

/// Recovers the complete original integer from the residual and trace.
pub fn recover_integer(output: &Output) -> Result<BigNat, Error> {
    let mut number = output.residual.clone();
    for event in output.trace.iter().rev() {
        if event.index >= event.radix {
            return Err(Error::VerificationFailure);
        }
        number.mul_add_small(event.radix, event.index);
    }
    Ok(number)
}

/// Optional numeric-source adapter. SMC supplies data; it does not change the
/// deterministic decoder's contract or establish an entropy/agency claim.
pub struct SmcChoiceSource {
    smc: Smc,
}

impl SmcChoiceSource {
    pub fn new(smc: Smc) -> Self {
        Self { smc }
    }

    pub fn next_choice(&mut self, declared_bits: u32) -> ChoiceState {
        let limb_count = declared_bits.div_ceil(32) as usize;
        let mut limbs = Vec::with_capacity(limb_count);
        while limbs.len() < limb_count {
            let word = self.smc.next_u64();
            limbs.push(word as u32);
            if limbs.len() < limb_count {
                limbs.push((word >> 32) as u32);
            }
        }
        if let Some(last) = limbs.last_mut() {
            let used_bits = declared_bits % 32;
            if used_bits != 0 {
                *last &= (1u32 << used_bits) - 1;
            }
        }
        let mut integer = BigNat { limbs };
        integer.normalize();
        ChoiceState::new(declared_bits, integer).expect("masked SMC value fits its frame")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intents() -> impl Iterator<Item = IntentFrame> {
        [
            Entity::Door,
            Entity::Gate,
            Entity::Window,
            Entity::System,
            Entity::Engine,
        ]
        .into_iter()
        .flat_map(|subject| {
            [
                Predicate::Open,
                Predicate::Closed,
                Predicate::Ready,
                Predicate::Active,
            ]
            .into_iter()
            .flat_map(move |predicate| {
                [Tense::Past, Tense::Present, Tense::Future]
                    .into_iter()
                    .flat_map(move |tense| {
                        [Polarity::Positive, Polarity::Negative]
                            .into_iter()
                            .map(move |polarity| {
                                IntentFrame::assertion(subject, predicate, tense, polarity)
                            })
                    })
            })
        })
    }

    #[test]
    fn arbitrary_precision_hex_and_decimal() {
        let value = BigNat::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF").unwrap();
        assert_eq!(value.bit_len(), 128);
        assert_eq!(value.to_hex(), "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF");
        assert_eq!(value.to_string(), "340282366920938463463374607431768211455");
    }

    #[test]
    fn every_supported_frame_preserves_meaning() {
        for intent in intents() {
            for number in 0..64 {
                let choice = ChoiceState::new(8, BigNat::from_u64(number)).unwrap();
                let output = decode(&choice, &intent).unwrap();
                assert!(
                    verify(&output, &intent),
                    "failed for {intent:?} at {number}"
                );
                assert_eq!(recover_integer(&output).unwrap(), choice.integer);
            }
        }
    }

    #[test]
    fn ranked_fiber_and_residual_are_consistent() {
        let choice = ChoiceState::from_hex(8, "1D").unwrap();
        let intent = IntentFrame::door_open();
        let output = decode(&choice, &intent).unwrap();
        assert_eq!(output.fiber_size, 12);
        assert_eq!(output.rank, 5);
        assert_eq!(output.residual.to_string(), "2");
        assert_eq!(recover_integer(&output).unwrap(), choice.integer);
    }

    #[test]
    fn decode_is_deterministic() {
        let choice = ChoiceState::from_hex(
            256,
            "8F2A00000000000000000000000000000000000000000000000000000000D91C",
        )
        .unwrap();
        let intent = IntentFrame::assertion(
            Entity::System,
            Predicate::Ready,
            Tense::Future,
            Polarity::Negative,
        );
        assert_eq!(decode(&choice, &intent), decode(&choice, &intent));
    }

    #[test]
    fn surface_tampering_fails_verification() {
        let choice = ChoiceState::from_hex(8, "1D").unwrap();
        let intent = IntentFrame::door_open();
        let mut output = decode(&choice, &intent).unwrap();
        output.sentence = "The door is closed.".into();
        assert!(!verify(&output, &intent));
    }

    #[test]
    fn declared_length_is_checked() {
        assert!(ChoiceState::from_hex(3, "F").is_err());
        assert!(ChoiceState::from_hex(4, "F").is_ok());
    }

    #[test]
    fn smc_adapter_respects_declared_length() {
        let mut source = SmcChoiceSource::new(Smc::default_fast());
        for bits in [0, 1, 31, 32, 33, 63, 64, 65, 256] {
            let choice = source.next_choice(bits);
            assert_eq!(choice.declared_bits, bits);
            assert!(choice.integer.bit_len() <= bits);
        }
    }
}
