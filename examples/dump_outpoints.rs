//! Prints the outpoints a proof PSBT claims, one `<txid> <vout>` per line.
//!
//! This is how `tests/fixtures/outpoints.txt` was generated, and how to
//! regenerate it if the proof fixture is ever replaced. The transactions
//! themselves come from an Esplora server:
//!
//! ```text
//! cargo run --example dump_outpoints > tests/fixtures/outpoints.txt
//! cut -d' ' -f1 tests/fixtures/outpoints.txt | sort -u | while read txid; do
//!   curl -s "https://blockstream.info/testnet/api/tx/$txid/hex" \
//!     -o "tests/fixtures/transactions/$txid"
//! done
//! ```

use bdk_reserves_web::proof;

fn main() {
    let psbt = proof::decode_psbt(include_str!("../web/example/proof.psbt.base64"))
        .expect("the fixture should be a valid PSBT");

    // Input 0 is the challenge input, which has no UTXO behind it.
    for txin in psbt.unsigned_tx.input.iter().skip(1) {
        println!(
            "{} {}",
            txin.previous_output.txid, txin.previous_output.vout
        );
    }
}
