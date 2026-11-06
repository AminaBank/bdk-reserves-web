//! Prints the outpoints a proof PSBT claims, one `<txid> <vout>` per line.
//!
//! This is how `tests/fixtures/outpoints.txt` was generated, and how to
//! regenerate it if the proof fixture is ever replaced. The transactions
//! themselves come from an Esplora server:
//!
//! ```text
//! cargo run --example dump_outpoints > tests/fixtures/outpoints.txt
//! mkdir -p "tests/fixtures/transactions" "tests/fixtures/tx_status" "tests/fixtures/outspend" "tests/fixtures/blocks"
//! cut -d' ' -f1 tests/fixtures/outpoints.txt | sort -u | while read txid; do
//!   curl -s "https://blockstream.info/testnet/api/tx/$txid/hex" \
//!     -o "tests/fixtures/transactions/$txid"
//!   curl -s "https://blockstream.info/testnet/api/tx/$txid/status" \
//!     -o "tests/fixtures/tx_status/$txid"
//!   mkdir -p "tests/fixtures/outspend/$txid"
//!   curl -s "https://blockstream.info/testnet/api/tx/$txid/outspend/$vout" \
//!     -o "tests/fixtures/outspend/$txid_$vout"
//! done
//! curl -s "https://blockstream.info/testnet/api/blocks/2133023" \
//!  -o "tests/fixtures/blocks/2133023"
//! curl -s "https://blockstream.info/testnet/api/blocks/5194000" \
//!  -o "tests/fixtures/blocks/5194000"
//! ```

use bdk_reserves_web::proof;

fn main() {
    for psbt in [
        include_str!("../web/example/proof_ok.psbt.base64"),
        include_str!("../web/example/proof_expired.psbt.base64"),
    ] {
        let psbt = proof::decode_psbt(psbt).expect("the fixture should be a valid PSBT");

        // Input 0 is the challenge input, which has no UTXO behind it.
        for txin in psbt.unsigned_tx.input.iter().skip(1) {
            println!(
                "{} {}",
                txin.previous_output.txid, txin.previous_output.vout
            );
        }
    }
}
