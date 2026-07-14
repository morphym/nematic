# CELM

CELM is a deterministic Rust machine that maps a canonically framed integer into English through frozen lexical and corpus data.

The CELM core does not create entropy or infer agency. Its input is a `(declared_bits, integer)` choice state. An optional adapter obtains that state from the sibling [`smc`](../smc) crate; explicit test vectors produce reproducible output without SMC.

## Current CELM-EN1 slice

- Arbitrary-precision hexadecimal choice states with declared bit lengths.
- Typed intent frames covering five entities, four state predicates, three tenses, and two polarities.
- Immutable, ID-ordered lexical and grammar profile data.
- Mixed-radix decoding with a complete decision trace.
- Ranked finite semantic fibers and a residual integer.
- Exact integer recovery from `(derivation, residual)`.
- Structural semantic verification by replaying the typed derivation.
- Explicit-number and SMC-source CLI modes.
- Free generation where the number selects both meaning and wording.
- Exact requested sentence lengths from 3 through 4,096 words.
- Princeton WordNet 3.1 nouns, verbs, adjectives, and adverbs.
- Complete POS patterns and lexical frequencies learned from the tagged Brown corpus.
- Every accepted WordNet entry remains reachable; corpus-observed words receive higher integer weight.

## Run

```sh
cargo run -- generate 12
cargo run -- generate 40 512
cargo run -- generate-explicit 12 256 8F2A00000000000000000000000000000000000000000000000000000000D91C
cargo run -- explicit 8 1D door open
cargo run -- explicit 256 8F2A00000000000000000000000000000000000000000000000000000000D91C system ready future negative
cargo run -- smc 256 engine active present positive
cargo test
```

`generate <words> [choice-bits]` requires no semantic intent from the terminal.
The default choice size is `max(256, words * 48)` bits. The reproducible
`generate-explicit` form accepts a framed hexadecimal choice instead.

Every successful run prints the sentence, choice state, rank, fiber size, residual, exact-verification result, reconstructed integer, frozen versions, and choice trace.

## How free generation works

For a requested length represented in Brown, the number first selects a complete observed part-of-speech pattern. Corpus function and inflected words preserve the observed grammatical structure. Base-form nouns, verbs, adjectives, and adverbs are selected from frequency-weighted tables extended by WordNet. For lengths absent from the corpus, a weighted POS transition graph provides the fallback.

The source datasets and hashes are recorded in [`../data/README.md`](../data/README.md).

## Exactness boundary

“Exact” means deterministic replay, exact word count, profile membership, and integer recovery. Corpus-derived syntax can produce unusual or semantically surreal English, especially because all dictionary entries—including rare and archaic ones—remain selectable. It does not claim unrestricted semantic equivalence, cryptographic security, source independence, true randomness, consciousness, or communication by an external entity.
