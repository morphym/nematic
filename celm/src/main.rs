use celm::{
    ChoiceState, Entity, IntentFrame, Polarity, Predicate, SmcChoiceSource, Tense, decode,
    recover_integer, verify,
};
use sm_c::Smc;

fn usage() -> ! {
    eprintln!(
        "usage:\n  celm explicit <bits> <hex> <entity> <predicate> [tense] [polarity]\n  celm smc <bits> <entity> <predicate> [tense] [polarity]\n\nentities: door, gate, window, system, engine\npredicates: open, closed, ready, active\ntenses: past, present, future (default: present)\npolarities: positive, negative (default: positive)"
    );
    std::process::exit(2);
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

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_else(|| usage());
    let bits: u32 = args
        .next()
        .unwrap_or_else(|| usage())
        .parse()
        .unwrap_or_else(|_| usage());

    let choice = match mode.as_str() {
        "explicit" => {
            let hex = args.next().unwrap_or_else(|| usage());
            ChoiceState::from_hex(bits, &hex).unwrap_or_else(|error| {
                eprintln!("error: {error}");
                std::process::exit(1);
            })
        }
        "smc" => SmcChoiceSource::new(Smc::default_fast()).next_choice(bits),
        _ => usage(),
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
