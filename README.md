# SMC + CELM Rust workspace

This workspace contains two deliberately separated machines:

- [`smc`](smc) supplies numeric choice material through its `Smc` stream API.
- [`celm`](celm) deterministically maps an explicit choice state into a verified controlled-English semantic fiber.

The boundary matters: SMC output is data supplied to CELM. CELM does not infer the data's origin, agency, or entropy quality, and explicit numeric test vectors bypass SMC entirely.

```text
SMC or explicit test vector
            |
            v
  ChoiceState (L, N) + IntentFrame
            |
            v
 deterministic mixed-radix decoder
            |
            v
 sentence + typed derivation + rank + residual + audit trace
            |
            v
 exact profile verifier
```

Run all tests:

```sh
cargo test --workspace
cargo clippy -p celm --all-targets -- -D warnings
```

Run CELM with a fixed choice or SMC input:

```sh
cargo run -p celm -- explicit 8 1D door open
cargo run -p celm -- smc 256 engine active present positive
```
