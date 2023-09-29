<div align="center">
  <h1>BDK-RESERVES-WEB</h1>

  <img src="./static/bdk.png" width="220" />
  <br>
  <a href="https://seba.swiss"><img src="./static/seba-bank-logo-bank-lange-green-ohne-tagline.png" width="250" /></a>

  <p>
    <strong>Proof of reserves for Bitcoin dev kit - web app</strong>
  </p>

  <p>
    <a href="https://github.com/bitcoindevkit/bdk-reserves/blob/master/LICENSE"><img alt="MIT or Apache-2.0 Licensed" src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg"/></a>
    <a href="https://github.com/weareseba/bdk-reserves-web/actions?query=workflow%3ACI"><img alt="CI Status" src="https://github.com/weareseba/bdk-reserves-web/workflows/CI/badge.svg"></a>
    <a href="https://bdk-reserves-web-de8e62f67d92.herokuapp.com"><img alt="Heroku" src="https://heroku-badge.herokuapp.com/?app=bdk-reserves-web"></a>
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
The `bdk-reserves-web` is a simple web app to validate the proofs.

* It validates proofs in the form of PSBT's.
* The implementation was inspired by <a href="https://github.com/bitcoin/bips/blob/master/bip-0127.mediawiki">BIP-0127</a> and <a href="https://github.com/bitcoin/bips/blob/master/bip-0322.mediawiki">BIP-0322</a>.

## Heroku
The web app is currently deployed to heroku, and can be reached here:
<a href="https://bdk-reserves-web-de8e62f67d92.herokuapp.com">bdk-reserves-web</a>

## Sponsorship
The implementation of <b>bdk-reserves-web</b> was sponsored by <a href="https://seba.swiss">SEBA Bank</a>.


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
