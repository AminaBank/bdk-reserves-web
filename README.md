<div align="center">
  <h1>BDK-RESERVES-WEB</h1>

  <img src="./static/bdk.png" width="220" />
  <br>
  <a href="https://www.aminagroup.com"><img src="./static/amina_logo.png" width="250" /></a>

  <p>
    <strong>Proof of reserves for Bitcoin dev kit - browser app</strong>
  </p>

  <p>
    <a href="https://github.com/bitcoindevkit/bdk-reserves/blob/master/LICENSE"><img alt="MIT or Apache-2.0 Licensed" src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg"/></a>
    <a href="https://github.com/AminaBank/bdk-reserves-web/actions?query=workflow%3ACI"><img alt="CI Status" src="https://github.com/AminaBank/bdk-reserves-web/workflows/CI/badge.svg"></a>
    <a href="https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html"><img alt="Rustc Version 1.85.0+" src="https://img.shields.io/badge/rustc-1.85.0%2B-lightgrey.svg"/></a>
  </p>

  <h4>
    <a href="https://bitcoindevkit.org">Project Homepage</a>
    <span> | </span>
    <a href="https://docs.rs/bdk">Documentation</a>
  </h4>
</div>

## About

The `bdk` library aims to be the core building block for Bitcoin wallets of any kind.
The `bdk-reserves` library provides an implementation of `proof-of-reserves` for bdk.
The `bdk-reserves-web` is a web app to validate the proofs.

* It validates proofs in the form of PSBT's.
* The implementation was inspired by <a href="https://github.com/bitcoin/bips/blob/master/bip-0127.mediawiki">BIP-0127</a> and <a href="https://github.com/bitcoin/bips/blob/master/bip-0322.mediawiki">BIP-0322</a>.

Verification runs entirely in the browser, compiled to WebAssembly. There is no
backend: `dist/` is a folder of static files that any web server, CDN or
GitHub Pages site can host. The PSBT you paste in is never uploaded anywhere.

The only network traffic is UTXO lookups against an [Esplora](https://github.com/Blockstream/esplora)
server, which default to Blockstream's public instances. You can point the app
at your own Esplora instead, and should if you do not want to tell a third party
which addresses you are checking.

## Verification

The proof itself is checked by `bdk-reserves`, unchanged from the server version.
Script signatures are validated by `bitcoinconsensus`, which is Bitcoin Core's
own script interpreter, so a signature accepted here is one the network would
accept. Amounts are taken from the transactions that created each UTXO, and each
transaction is checked against the txid that was requested, so an Esplora server
cannot inflate the reported total by misreporting a value.

What you still trust the Esplora server for is which outputs are *unspent*. A
server that hides a UTXO makes a valid proof fail, which is the safe direction.

## Building

Needs a Rust toolchain, `curl` and `node` (for the end to end test). Everything
else is fetched into `.tools/` on first build. The fetched toolchain is the
Linux x86_64 build, so that is currently the only host `build.sh` supports.

```shell
make build     # produces dist/
make serve     # builds, then serves dist/ on http://localhost:8087
make test      # verification logic on the host, plus the built module end to end
```

`make serve` runs `python3 -m http.server` over `dist/`, which is all a dev
build needs. To publish it elsewhere, copy `dist/` anywhere that serves static
files: S3, a CDN, or any web server. The `.wasm` file should be served as
`application/wasm`, which most of them do already.

### Why a C++ toolchain gets downloaded

`bitcoinconsensus` compiles Bitcoin Core's script interpreter, which is C++, so
building for wasm needs a C++ standard library for that target. `build.sh`
fetches the [wasi-sdk](https://github.com/WebAssembly/wasi-sdk) and uses its
clang and libc++.

It deliberately does not link wasi-libc, because that would put a second
allocator in the module next to Rust's and add WASI imports the browser would
have to fill in. `src/csupport.rs` supplies the dozen or so C symbols libc++
actually needs instead, with `malloc` and `free` forwarding to Rust's allocator.
The module that comes out imports nothing but its own JS glue.

Set `WASI_SDK_PATH` to reuse an SDK you already have.

## Deployment

Every push to `master` publishes to
<a href="https://aminabank.github.io/bdk-reserves-web/">aminabank.github.io/bdk-reserves-web</a>
via `.github/workflows/pages.yml`. The workflow runs the full test suite first,
so a broken verifier does not reach the published site.

This needs **Settings > Pages > Build and deployment > Source** set to
**GitHub Actions**, once. Until that is done the deploy step fails.

Pages serves the app from a subdirectory rather than a domain root, which is why
everything in `web/` refers to its assets relatively.

## Sponsorship
The implementation of <b>bdk-reserves-web</b> was sponsored by <a href="https://www.aminagroup.com">AMINA Bank</a>.

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
