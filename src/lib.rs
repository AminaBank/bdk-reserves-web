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
    use crate::{
        esplora::{BlockHeight, Esplora},
        proof,
    };
    use serde::{Deserialize, Serialize};
    use wasm_bindgen::prelude::*;

    /// The depth the HTTP service asked for, and a sensible default here too.
    const DEFAULT_CONFIRMATIONS: BlockHeight = 3;

    #[derive(Debug, Deserialize)]
    pub struct VerifyRequest {
        message: String,
        proof_psbt: String,
        #[serde(default)]
        confirmations: Option<BlockHeight>,
        #[serde(default)]
        esplora_url: Option<String>,
    }

    #[derive(Debug, Serialize)]
    pub struct VerifyResponse {
        /// Total value the proof demonstrates control of, in satoshis.
        spendable: u64,
        expired: bool,
        valid_until: String,
        /// How many UTXOs were deep enough to count towards the total.
        utxos: usize,
    }

    #[wasm_bindgen(start)]
    pub fn start() {
        console_error_panic_hook::set_once();
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

        let psbt = proof::decode_psbt(&request.proof_psbt)?;

        let esplora_url = request
            .esplora_url
            .as_deref()
            .map(str::trim)
            .ok_or("Esplora server is missing")?;
        let esplora = Esplora::new(&esplora_url);

        let confirmations = request.confirmations.unwrap_or(DEFAULT_CONFIRMATIONS);
        let tip_height = esplora.tip_height().await?;
        let cutoff = proof::confirmation_cutoff(tip_height, confirmations)?;

        let (outpoints, unspent_height) = esplora.outpoints_for_psbt(&psbt, cutoff).await?;
        let valid_until = esplora
            .time_for_block(if let Some(unspent_height) = unspent_height {
                unspent_height
            } else {
                tip_height
            })
            .await?;

        let utxos = outpoints.len();
        let spendable = proof::verify(&psbt, &request.message, outpoints)?;

        serde_wasm_bindgen::to_value(&VerifyResponse {
            spendable: spendable.to_sat(),
            expired: unspent_height.is_some(),
            valid_until: format!("{:?}", valid_until),
            utxos,
        })
        .map_err(|e| format!("Could not build the response: {}", e))
    }
}
