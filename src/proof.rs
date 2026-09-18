//! The parts of proof verification that do not touch the network.
//!
//! Keeping these separate from the Esplora client means they can be unit tested
//! with a plain `cargo test` on the host, without a browser or a live server.

use bdk_reserves::reserves::verify_proof;
use bdk_wallet::bitcoin::{
    base64::{engine::general_purpose::STANDARD, Engine as _},
    psbt::Psbt,
    Address, Amount, Network, OutPoint, TxOut,
};
use std::str::FromStr;

/// Networks we are willing to resolve an address against, most likely first.
const SUPPORTED_NETWORKS: [Network; 4] = [
    Network::Bitcoin,
    Network::Testnet,
    Network::Signet,
    Network::Regtest,
];

pub fn decode_psbt(proof_psbt: &str) -> Result<Psbt, String> {
    let raw = STANDARD
        .decode(proof_psbt.trim())
        .map_err(|e| format!("Base64 decode error: {:?}", e))?;
    Psbt::deserialize(&raw).map_err(|e| format!("PSBT deserialization error: {:?}", e))
}

/// Work out which network the proof is for by seeing which one every address
/// parses against.
///
/// The addresses themselves are the only hint available, so rather than guess
/// from a prefix character we take the first network that accepts all of them.
/// Mainnet and testnet share `tb1`/`bcrt1`-style ambiguity in places, hence the
/// fixed precedence in `SUPPORTED_NETWORKS`.
pub fn detect_network(addresses: &[String]) -> Result<Network, String> {
    if addresses.is_empty() {
        return Err("No address provided".to_string());
    }

    let unchecked = addresses
        .iter()
        .map(|address| {
            Address::from_str(address.trim())
                .map_err(|e| format!("Invalid address {}: {:?}", address, e))
        })
        .collect::<Result<Vec<_>, String>>()?;

    SUPPORTED_NETWORKS
        .into_iter()
        .find(|network| {
            unchecked
                .iter()
                .all(|address| address.is_valid_for_network(*network))
        })
        .ok_or_else(|| {
            "The addresses do not all belong to the same network, or the network is not supported"
                .to_string()
        })
}

pub fn parse_addresses(addresses: &[String], network: Network) -> Result<Vec<Address>, String> {
    addresses
        .iter()
        .map(|address| {
            Address::from_str(address.trim())
                .map_err(|e| format!("Invalid address {}: {:?}", address, e))?
                .require_network(network)
                .map_err(|e| format!("Address {} is not valid on {}: {:?}", address, network, e))
        })
        .collect()
}

/// Highest block height a UTXO may sit at and still count as confirmed.
///
/// A UTXO mined in block `h` has `tip - h + 1` confirmations, the block it
/// landed in being the first, so asking for at least `confirmations` of them
/// means `h <= tip + 1 - confirmations`.
pub fn confirmation_cutoff(tip_height: usize, confirmations: usize) -> Result<usize, String> {
    if confirmations > tip_height {
        return Err(format!(
            "The chain is only {} blocks long, which cannot satisfy {} confirmations",
            tip_height, confirmations
        ));
    }
    Ok(tip_height + 1 - confirmations)
}

/// Verify the proof against the UTXOs that were found to be unspent.
pub fn verify(
    psbt: &Psbt,
    message: &str,
    outpoints: Vec<(OutPoint, TxOut)>,
) -> Result<Amount, String> {
    verify_proof(psbt, message, outpoints).map_err(|e| format!("{:?}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    // A real proof, the same one the HTTP service used as its test fixture.
    const PROOF_PSBT: &str = include_str!("../web/example/proof.psbt.base64");
    const MESSAGE: &str = "Stored in SEBA Bank AG cold storage";
    const ADDRESS: &str = "2Mtkk3kjyN8hgdGXPuJCNnwS3BBY4K2frhY";

    #[test]
    fn decodes_a_real_proof() {
        let psbt = decode_psbt(PROOF_PSBT).expect("fixture should decode");
        // A BIP-127 proof is the challenge input plus one input per UTXO,
        // collapsing into a single unspendable output.
        assert!(psbt.inputs.len() > 1);
        assert_eq!(psbt.unsigned_tx.output.len(), 1);
    }

    #[test]
    fn rejects_malformed_base64() {
        assert!(decode_psbt("not a psbt").is_err());
    }

    #[test]
    fn detects_testnet_from_a_p2sh_address() {
        let addresses = vec![ADDRESS.to_string()];
        assert_eq!(detect_network(&addresses).unwrap(), Network::Testnet);
    }

    #[test]
    fn detects_mainnet_from_a_bech32_address() {
        let addresses = vec!["bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq".to_string()];
        assert_eq!(detect_network(&addresses).unwrap(), Network::Bitcoin);
    }

    #[test]
    fn refuses_addresses_from_different_networks() {
        let addresses = vec![
            "bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq".to_string(),
            ADDRESS.to_string(),
        ];
        assert!(detect_network(&addresses).is_err());
    }

    #[test]
    fn refuses_an_empty_address_list() {
        assert!(detect_network(&[]).is_err());
    }

    #[test]
    fn parses_addresses_for_the_detected_network() {
        let addresses = vec![ADDRESS.to_string()];
        let network = detect_network(&addresses).unwrap();
        let parsed = parse_addresses(&addresses, network).unwrap();
        assert_eq!(parsed.len(), 1);
    }

    /// The block a UTXO lands in counts as its first confirmation, so at a tip
    /// of 100 the shallowest block with 3 confirmations is 98, not 97.
    #[test]
    fn confirmation_cutoff_counts_the_including_block() {
        assert_eq!(confirmation_cutoff(100, 3).unwrap(), 98);
    }

    #[test]
    fn one_confirmation_admits_the_tip_block() {
        assert_eq!(confirmation_cutoff(100, 1).unwrap(), 100);
    }

    #[test]
    fn zero_confirmations_admits_anything_already_mined() {
        assert_eq!(confirmation_cutoff(100, 0).unwrap(), 101);
    }

    #[test]
    fn confirmation_cutoff_refuses_a_depth_the_chain_cannot_reach() {
        assert!(confirmation_cutoff(2, 3).is_err());
        assert!(confirmation_cutoff(2, 5).is_err());
    }

    /// Without the UTXOs backing it, a proof has to be rejected rather than
    /// quietly reporting a smaller balance. This is what the HTTP service's
    /// test asserted, except it needed a live Electrum server to get there.
    #[test]
    fn a_proof_with_no_matching_utxos_is_not_spendable() {
        let psbt = decode_psbt(PROOF_PSBT).unwrap();
        let err = verify(&psbt, MESSAGE, Vec::new()).unwrap_err();
        assert_eq!(err, "NonSpendableInput(1)");
    }

    #[test]
    fn a_proof_checked_against_the_wrong_message_is_rejected() {
        let psbt = decode_psbt(PROOF_PSBT).unwrap();
        let err = verify(&psbt, "some other message", Vec::new()).unwrap_err();
        assert_eq!(err, "ChallengeInputMismatch");
    }
}
