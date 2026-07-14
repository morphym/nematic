use celm::{
    ChoiceState, EnglishOutput, EnglishProfile, Entity, IntentFrame, MAX_GENERATED_WORDS,
    MIN_GENERATED_WORDS, Polarity, Predicate, SmcChoiceSource, Tense, decode,
    recommended_choice_bits, recover_integer, verify,
};
use sm_c::Smc;

fn usage() -> ! {
    eprintln!(
        "usage:\n  celm generate <words> [choice-bits]\n  celm generate-explicit <words> <bits> <hex>\n  celm explicit <bits> <hex> <entity> <predicate> [tense] [polarity]\n  celm smc <bits> <entity> <predicate> [tense] [polarity]\n\n`generate` chooses both meaning and wording and guarantees the requested word count.\nword range: 3..=4096\nentities: door, gate, window, system, engine\npredicates: open, closed, ready, active\ntenses: past, present, future (default: present)\npolarities: positive, negative (default: positive)"
    );
    std::process::exit(2);
}

fn parse_u32(value: Option<String>) -> u32 {
    value
        .unwrap_or_else(|| usage())
        .parse()
        .unwrap_or_else(|_| usage())
}

fn parse_usize(value: Option<String>) -> usize {
    value
        .unwrap_or_else(|| usage())
        .parse()
        .unwrap_or_else(|_| usage())
}

fn parse_entity(value: &str) -> Entity {
    match value {
        "door" => Entity::Door,
        "gate" => Entity::Gate,
        "window" => Entity::Window,
        "system" => Entity::System,
        "engine" => Entity::Engine,
        _ => usage(),
    }
}

fn parse_predicate(value: &str) -> Predicate {
    match value {
        "open" => Predicate::Open,
        "closed" => Predicate::Closed,
        "ready" => Predicate::Ready,
        "active" => Predicate::Active,
        _ => usage(),
    }
}

fn parse_tense(value: Option<String>) -> Tense {
    match value.as_deref().unwrap_or("present") {
        "past" => Tense::Past,
        "present" => Tense::Present,
        "future" => Tense::Future,
        _ => usage(),
    }
}

fn parse_polarity(value: Option<String>) -> Polarity {
    match value.as_deref().unwrap_or("positive") {
        "positive" => Polarity::Positive,
        "negative" => Polarity::Negative,
        _ => usage(),
    }
}

fn generated_or_exit(
    profile: &EnglishProfile,
    choice: &ChoiceState,
    words: usize,
) -> EnglishOutput {
    profile.generate(choice, words).unwrap_or_else(|error| {
        eprintln!("error: {error}");
        std::process::exit(1);
    })
}

fn print_generated(profile: &EnglishProfile, choice: &ChoiceState, output: &EnglishOutput) {
    let stats = profile.stats();
    println!("sentence: {}", output.sentence);
    println!(
        "length: {} words (requested {})",
        output.sentence.split_whitespace().count(),
        output.requested_words
    );
    println!(
        "choice: 0x{} ({} bits declared)",
        choice.integer.to_hex(),
        choice.declared_bits
    );
    println!("residual: {}", output.residual);
    println!(
        "verification: {}",
        if profile.verify(output) {
            "exact_pass"
        } else {
            "fail"
        }
    );
    println!(
        "recovered: 0x{}",
        profile.recover_integer(output).unwrap().to_hex()
    );
    println!("choices consumed: {}", output.trace.len());
    println!(
        "language profile: {} lexical forms, {} corpus sentences, {} POS tags",
        stats.lexical_forms, stats.corpus_sentences, stats.tags
    );
    println!(
        "versions: {}, {}, {}",
        output.profile_version, output.grammar_version, output.lexicon_version
    );
}

fn run_generated(mode: &str, mut args: impl Iterator<Item = String>) {
    let words = parse_usize(args.next());
    if !(MIN_GENERATED_WORDS..=MAX_GENERATED_WORDS).contains(&words) {
        eprintln!(
            "error: requested word count must be between {MIN_GENERATED_WORDS} and {MAX_GENERATED_WORDS}"
        );
        std::process::exit(1);
    }
    let choice = match mode {
        "generate" => {
            let bits = args
                .next()
                .map(|value| value.parse().unwrap_or_else(|_| usage()))
                .unwrap_or_else(|| recommended_choice_bits(words));
            SmcChoiceSource::new(Smc::default_fast()).next_choice(bits)
        }
        "generate-explicit" => {
            let bits = parse_u32(args.next());
            let hex = args.next().unwrap_or_else(|| usage());
            ChoiceState::from_hex(bits, &hex).unwrap_or_else(|error| {
                eprintln!("error: {error}");
                std::process::exit(1);
            })
        }
        _ => unreachable!(),
    };
    let profile = EnglishProfile::load_default().unwrap_or_else(|error| {
        eprintln!("error: {error}");
        std::process::exit(1);
    });
    let output = generated_or_exit(&profile, &choice, words);
    print_generated(&profile, &choice, &output);
}

fn run_intent(mode: &str, mut args: impl Iterator<Item = String>) {
    let bits = parse_u32(args.next());
    let choice = match mode {
        "explicit" => {
            let hex = args.next().unwrap_or_else(|| usage());
            ChoiceState::from_hex(bits, &hex).unwrap_or_else(|error| {
                eprintln!("error: {error}");
                std::process::exit(1);
            })
        }
        "smc" => SmcChoiceSource::new(Smc::default_fast()).next_choice(bits),
        _ => unreachable!(),
    };

    let entity = parse_entity(&args.next().unwrap_or_else(|| usage()));
    let predicate = parse_predicate(&args.next().unwrap_or_else(|| usage()));
    let intent = IntentFrame::assertion(
        entity,
        predicate,
        parse_tense(args.next()),
        parse_polarity(args.next()),
    );
    let output = decode(&choice, &intent).unwrap_or_else(|error| {
        eprintln!("error: {error}");
        std::process::exit(1);
    });

    println!("sentence: {}", output.sentence);
    println!(
        "choice: 0x{} ({} bits declared)",
        choice.integer.to_hex(),
        bits
    );
    println!("fiber: rank {} of {}", output.rank, output.fiber_size);
    println!("residual: {}", output.residual);
    println!(
        "verification: {}",
        if verify(&output, &intent) {
            "exact_pass"
        } else {
            "fail"
        }
    );
    println!(
        "recovered: 0x{}",
        recover_integer(&output).unwrap().to_hex()
    );
    println!(
        "versions: {}, {}, {}",
        output.spec_version, output.grammar_version, output.lexicon_version
    );
    println!("trace:");
    for event in output.trace {
        println!(
            "  {} radix={} index={} candidate={} residual={} -> {}",
            event.field,
            event.radix,
            event.index,
            event.candidate_id,
            event.residual_before,
            event.residual_after
        );
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_else(|| usage());
    match mode.as_str() {
        "generate" | "generate-explicit" => run_generated(&mode, args),
        "explicit" | "smc" => run_intent(&mode, args),
        _ => usage(),
    }
}
