//! A small Esplora client built on the browser's `fetch`.
//!
//! The service this replaces talked to Electrum over TCP, which a page cannot
//! do. Esplora exposes the same information over plain HTTP with CORS enabled,
//! so the lookups can run in the tab with no server of our own in between.

use bdk_wallet::bitcoin::{
    consensus::deserialize, hashes::Hash, Address, OutPoint, Transaction, Txid,
};
use futures_util::{stream, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, Response};

/// Look up `fetch` on the global object rather than going through `Window`, so
/// the same module works in a page, in a worker, and under Node.
fn fetch(request: &Request) -> Result<js_sys::Promise, String> {
    let global = js_sys::global();
    let fetch = js_sys::Reflect::get(&global, &JsValue::from_str("fetch"))
        .map_err(|_| "This environment provides no fetch()".to_string())?;
    let fetch: js_sys::Function = fetch
        .dyn_into()
        .map_err(|_| "This environment provides no fetch()".to_string())?;
    fetch
        .call1(&global, request)
        .map_err(|e| format!("Call to fetch() failed: {}", describe(&e)))?
        .dyn_into()
        .map_err(|_| "fetch() did not return a promise".to_string())
}

/// How many transaction fetches to keep in flight. Enough to hide latency on a
/// proof covering many UTXOs, low enough to stay polite to a public server.
const MAX_CONCURRENT_REQUESTS: usize = 8;

pub struct Esplora {
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct Utxo {
    txid: String,
    vout: u32,
    status: UtxoStatus,
}

#[derive(Debug, Deserialize)]
struct UtxoStatus {
    confirmed: bool,
    block_height: Option<usize>,
}

impl Esplora {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    async fn get(&self, path: &str) -> Result<String, String> {
        let url = format!("{}{}", self.base_url, path);

        let opts = RequestInit::new();
        opts.set_method("GET");

        let request = Request::new_with_str_and_init(&url, &opts)
            .map_err(|e| format!("Could not build a request for {}: {}", url, describe(&e)))?;

        let response: Response = JsFuture::from(fetch(&request)?)
            .await
            .map_err(|e| format!("Request to {} failed: {}", url, describe(&e)))?
            .dyn_into()
            .map_err(|_| format!("Unexpected response type from {}", url))?;

        if !response.ok() {
            return Err(format!(
                "{} returned HTTP {} {}",
                url,
                response.status(),
                response.status_text()
            ));
        }

        let text =
            JsFuture::from(response.text().map_err(|e| {
                format!("Could not read the response from {}: {}", url, describe(&e))
            })?)
            .await
            .map_err(|e| format!("Could not read the response from {}: {}", url, describe(&e)))?;

        text.as_string()
            .ok_or_else(|| format!("Response from {} was not text", url))
    }

    pub async fn tip_height(&self) -> Result<usize, String> {
        let body = self.get("/blocks/tip/height").await?;
        body.trim()
            .parse()
            .map_err(|e| format!("Could not read the chain tip height {:?}: {:?}", body, e))
    }

    /// Fetch the confirmed, sufficiently deep UTXOs of `address`.
    ///
    /// Only the outpoints are taken from this response. The amounts come from
    /// the transactions themselves in `outpoints_for_addresses`, so a server
    /// that misreports a value cannot inflate the total.
    async fn confirmed_outpoints(
        &self,
        address: &Address,
        max_confirmation_height: usize,
    ) -> Result<Vec<OutPoint>, String> {
        let body = self.get(&format!("/address/{}/utxo", address)).await?;
        let utxos: Vec<Utxo> = serde_json::from_str(&body)
            .map_err(|e| format!("Could not read the UTXOs of {}: {:?}", address, e))?;

        utxos
            .into_iter()
            .filter(|utxo| {
                utxo.status.confirmed
                    && utxo
                        .status
                        .block_height
                        .is_some_and(|height| height > 0 && height <= max_confirmation_height)
            })
            .map(|utxo| {
                Ok(OutPoint {
                    txid: Txid::from_str(&utxo.txid)
                        .map_err(|e| format!("Invalid txid {}: {:?}", utxo.txid, e))?,
                    vout: utxo.vout,
                })
            })
            .collect()
    }

    async fn transaction(&self, txid: Txid) -> Result<Transaction, String> {
        let hex = self.get(&format!("/tx/{}/hex", txid)).await?;
        let raw = hex_decode(hex.trim())
            .map_err(|e| format!("Transaction {} came back malformed: {}", txid, e))?;
        let tx: Transaction = deserialize(&raw)
            .map_err(|e| format!("Could not parse transaction {}: {:?}", txid, e))?;

        // The server told us this outpoint is unspent; check it actually handed
        // back the transaction we asked for rather than taking its word for it.
        if tx.compute_txid() != txid {
            return Err(format!(
                "Server returned a transaction whose txid is {} but {} was requested",
                tx.compute_txid(),
                txid
            ));
        }

        Ok(tx)
    }

    /// Collect every spendable UTXO across `addresses`, paired with the output
    /// it refers to, ready to hand to `verify_proof`.
    pub async fn outpoints_for_addresses(
        &self,
        addresses: &[Address],
        max_confirmation_height: usize,
    ) -> Result<Vec<(OutPoint, bdk_wallet::bitcoin::TxOut)>, String> {
        let per_address: Vec<Result<Vec<OutPoint>, String>> = stream::iter(addresses)
            .map(|address| self.confirmed_outpoints(address, max_confirmation_height))
            .buffer_unordered(MAX_CONCURRENT_REQUESTS)
            .collect()
            .await;

        let outpoints: Vec<OutPoint> = per_address
            .into_iter()
            .collect::<Result<Vec<_>, String>>()?
            .concat();

        // Several outpoints often share a transaction, so fetch each one once.
        let mut wanted: Vec<Txid> = outpoints.iter().map(|outpoint| outpoint.txid).collect();
        wanted.sort_unstable_by_key(|txid| txid.to_byte_array());
        wanted.dedup();

        let fetched: Vec<Result<(Txid, Transaction), String>> = stream::iter(wanted)
            .map(|txid| async move { self.transaction(txid).await.map(|tx| (txid, tx)) })
            .buffer_unordered(MAX_CONCURRENT_REQUESTS)
            .collect()
            .await;

        let transactions: HashMap<Txid, Transaction> =
            fetched.into_iter().collect::<Result<_, String>>()?;

        outpoints
            .into_iter()
            .map(|outpoint| {
                let tx = transactions
                    .get(&outpoint.txid)
                    .ok_or_else(|| format!("Missing transaction {}", outpoint.txid))?;
                let txout = tx
                    .output
                    .get(outpoint.vout as usize)
                    .ok_or_else(|| {
                        format!(
                            "Transaction {} has no output {}",
                            outpoint.txid, outpoint.vout
                        )
                    })?
                    .clone();
                Ok((outpoint, txout))
            })
            .collect()
    }
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
    let mut digits = hex.as_bytes().chunks_exact(2);
    if !digits.remainder().is_empty() {
        return Err("odd number of hex digits".to_string());
    }
    digits
        .by_ref()
        .map(|pair| {
            let pair = std::str::from_utf8(pair).map_err(|e| format!("{:?}", e))?;
            u8::from_str_radix(pair, 16).map_err(|e| format!("{:?}", e))
        })
        .collect()
}

/// Turn a thrown JS value into something worth showing a user.
fn describe(value: &JsValue) -> String {
    value
        .as_string()
        .or_else(|| {
            value
                .dyn_ref::<js_sys::Error>()
                .map(|error| String::from(error.message()))
        })
        .unwrap_or_else(|| {
            "the request was blocked or the server is unreachable (check CORS and the URL)"
                .to_string()
        })
}
