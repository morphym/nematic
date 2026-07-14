# CELM

CELM is a deterministic Rust machine that maps a canonically framed integer to a controlled-English realization of an explicit semantic intent.

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

## Run

```sh
cargo run -- explicit 8 1D door open
cargo run -- explicit 256 8F2A00000000000000000000000000000000000000000000000000000000D91C system ready future negative
cargo run -- smc 256 engine active present positive
cargo test
```

Every successful run prints the sentence, choice state, rank, fiber size, residual, exact-verification result, reconstructed integer, frozen versions, and choice trace.

## Exactness boundary

“Exact” means equality inside the frozen CELM-EN1 formal profile. It does not claim unrestricted English equivalence, cryptographic security, source independence, true randomness, consciousness, or communication by an external entity.
