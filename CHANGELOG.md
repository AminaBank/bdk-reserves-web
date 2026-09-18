# Changelog 

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v0.3.0]

### Summary

The app now runs entirely in the browser. There is no backend left.

### Changed

- Verification is compiled to WebAssembly and runs in the page. The PSBT is no
  longer uploaded to a server.
- UTXO lookups moved from Electrum to Esplora over HTTP, because a browser
  cannot open the TCP connection Electrum needs. The server is configurable.
- The network is worked out by parsing the addresses instead of guessing from
  the first character, so testnet bech32 and signet addresses now work.
- Amounts come from the transactions that created each UTXO, and each one is
  checked against the txid requested, so the Esplora server cannot inflate the
  reported total.
- Confirmation depth is configurable in the UI, still defaulting to 3.
- `bdk-reserves` comes from crates.io (3.0.0) now that its bdk_wallet 1.0 port
  has been merged upstream, instead of a git branch on a fork. That brings
  `bdk_wallet` 3 and `bitcoinconsensus` 0.105, so signatures are checked by the
  script interpreter from Bitcoin Core 25.1, and lifts the minimum Rust
  version to 1.85.
- SEBA Bank is AMINA Bank now, and the branding follows. The example proof's
  message still names SEBA, because it is signed into the proof and cannot
  change.
- The page has a "Load example" button that fills in a real testnet proof,
  along with a note explaining that its UTXOs have since been spent.
- Verification failures are explained in plain language rather than only as the
  enum variant `bdk-reserves` returns. The raw message is still shown.
- The build output is a folder of static files, so `make serve` is enough to
  run it locally and any static host can serve it. The Dockerfile and the
  Heroku deployment are gone.

### Fixed

- Confirmation depth was off by one. The block a UTXO is mined in counts as its
  first confirmation, but the old rule treated it as the zeroth, so asking for
  3 confirmations quietly required 4.

### Added

- A GitHub Pages workflow that publishes the site on every push to `master`,
  gated on the test suite passing.

### Removed

- The actix-web HTTP service and its `/proof` endpoint.
- The `libc` stand-in crate for the wasm build. `bitcoinconsensus` 0.105 does
  not need it, and without the patch the wasm build no longer rewrites
  `Cargo.lock`.

## [v0.1.10]

### Summary

Updated dependencies

### Changed

- upgraded bdk-reserves with improved signature verification
- upgraded other dependencies

## [v0.1.9]

### Summary

Nicer frontend

### Changed

- Nicer frontend website
- Added a Dockerfile

## [v0.1.7]

### Summary

Heroku

### Changed

- Deploy to heroku

## [v0.1.0]

### Summary

Initial release

### Changed

- Built a simple web app to verify BIP-127 proof of reserves BSBTs

