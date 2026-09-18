//! Browser-side verifier for BIP-127 proof of reserves PSBTs.
//!
//! This used to be an HTTP service that verified proofs on behalf of whoever
//! asked. Everything it did now runs in the page instead: the PSBT never leaves
//! the browser, and the only network traffic is the UTXO lookups against an
//! Esplora server of the user's choosing.

pub mod proof;

#[cfg(target_arch = "wasm32")]
mod csupport;

#[cfg(target_arch = "wasm32")]
mod esplora;

#[cfg(target_arch = "wasm32")]
mod bindings {
    use crate::{esplora::Esplora, proof};
    use bdk_wallet::bitcoin::Network;
    use serde::{Deserialize, Serialize};
    use wasm_bindgen::prelude::*;

    /// The depth the HTTP service asked for, and a sensible default here too.
    const DEFAULT_CONFIRMATIONS: usize = 3;

    #[derive(Debug, Deserialize)]
    pub struct VerifyRequest {
        addresses: Vec<String>,
        message: String,
        proof_psbt: String,
        #[serde(default)]
        confirmations: Option<usize>,
        #[serde(default)]
        esplora_url: Option<String>,
    }

    #[derive(Debug, Serialize)]
    pub struct VerifyResponse {
        /// Total value the proof demonstrates control of, in satoshis.
        spendable: u64,
        network: String,
        tip_height: usize,
        /// How many UTXOs were deep enough to count towards the total.
        utxos: usize,
        esplora_url: String,
    }

    #[wasm_bindgen(start)]
    pub fn start() {
        console_error_panic_hook::set_once();
    }

    /// Default Esplora endpoint for a network, used when the caller does not
    /// name one. These are Blockstream's public servers, which send CORS
    /// headers; any other Esplora instance works just as well.
    fn default_esplora_url(network: Network) -> Result<&'static str, String> {
        match network {
            Network::Bitcoin => Ok("https://blockstream.info/api"),
            Network::Testnet => Ok("https://blockstream.info/testnet/api"),
            Network::Signet => Ok("https://blockstream.info/signet/api"),
            other => Err(format!(
                "No public Esplora server is known for {}, please give one explicitly",
                other
            )),
        }
    }

    /// Verify a proof of reserves.
    ///
    /// Resolves to a `VerifyResponse`, or rejects with a message describing why
    /// the proof was not accepted.
    #[wasm_bindgen(js_name = verifyProofOfReserves)]
    pub async fn verify_proof_of_reserves(request: JsValue) -> Result<JsValue, JsValue> {
        run(request).await.map_err(|e| JsValue::from_str(&e))
    }

    async fn run(request: JsValue) -> Result<JsValue, String> {
        let request: VerifyRequest = serde_wasm_bindgen::from_value(request)
            .map_err(|e| format!("Could not read the request: {}", e))?;

        let addresses: Vec<String> = request
            .addresses
            .iter()
            .map(|address| address.trim().to_string())
            .filter(|address| !address.is_empty())
            .collect();

        let psbt = proof::decode_psbt(&request.proof_psbt)?;
        let network = proof::detect_network(&addresses)?;
        let parsed = proof::parse_addresses(&addresses, network)?;

        let esplora_url = match request.esplora_url.as_deref().map(str::trim) {
            Some(url) if !url.is_empty() => url.to_string(),
            _ => default_esplora_url(network)?.to_string(),
        };
        let esplora = Esplora::new(&esplora_url);

        let confirmations = request.confirmations.unwrap_or(DEFAULT_CONFIRMATIONS);
        let tip_height = esplora.tip_height().await?;
        let cutoff = proof::confirmation_cutoff(tip_height, confirmations)?;

        let outpoints = esplora.outpoints_for_addresses(&parsed, cutoff).await?;
        let utxos = outpoints.len();
        let spendable = proof::verify(&psbt, &request.message, outpoints)?;

        serde_wasm_bindgen::to_value(&VerifyResponse {
            spendable: spendable.to_sat(),
            network: network.to_string(),
            tip_height,
            utxos,
            esplora_url,
        })
        .map_err(|e| format!("Could not build the response: {}", e))
    }
}
