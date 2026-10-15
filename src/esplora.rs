//! A small Esplora client built on the browser's `fetch`.
//!
//! The service this replaces talked to Electrum over TCP, which a page cannot
//! do. Esplora exposes the same information over plain HTTP with CORS enabled,
//! so the lookups can run in the tab with no server of our own in between.

use bdk_wallet::bitcoin::{
    block::Version, consensus::deserialize, hashes::Hash, psbt::Psbt, BlockHash, CompactTarget,
    OutPoint, Transaction, TxMerkleNode, TxOut, Txid, Weight,
};
use chrono::{DateTime, Utc};
use futures_util::{stream, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, Response};

pub type BlockHeight = u32;

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

// The status of a [`TxOut`].
#[allow(dead_code)]
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OutputStatus {
    /// Whether the [`TxOut`] is spent.
    pub spent: bool,
    /// The [`Txid`] of the [`Transaction`] that spent this [`TxOut`].
    pub txid: Option<Txid>,
    /// The input index of this [`TxOut`] in the [`Transaction`] that spent it.
    pub vin: Option<u64>,
    /// Information about the [`Transaction`] that spent this [`TxOut`].
    pub status: Option<TxStatus>,
}

/// The confirmation status of a [`Transaction`].
#[allow(dead_code)]
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TxStatus {
    /// Whether the [`Transaction`] is confirmed or not.
    pub confirmed: bool,
    /// The block height that confirmed the [`Transaction`].
    pub block_height: Option<BlockHeight>,
    /// The [`BlockHash`] of the block that confirmed the [`Transaction`].
    ///
    /// `None` if the [`Transaction`] was confirmed by the genesis block.
    pub block_hash: Option<BlockHash>,
    /// The UNIX timestamp of the block that confirmed the [`Transaction`], as claimed by the
    /// miner.
    pub block_time: Option<u64>,
}

/// A summary of a [`Block`].
///
/// Contains additional metadata about a [`Block`], but not the whole [`Block`].
///
/// For the complete [`Block`] contents, use the `get_block_by_hash` client method.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct BlockInfo {
    /// The [`BlockHash`] of this [`Block`].
    pub id: BlockHash,
    /// The block height of this [`Block`].
    pub height: u32,
    /// The version of this [`Block`].
    pub version: Version,
    /// The UNIX timestamp of this [`Block`], as claimed by the miner.
    pub timestamp: u64,
    /// The [`Transaction`] count for this [`Block`].
    pub tx_count: u64,
    /// The size of this [`Block`], in bytes.
    pub size: usize,
    /// The [`Weight`] of this [`Block`].
    pub weight: Weight,
    /// The Merkle root of this [`Block`].
    pub merkle_root: TxMerkleNode,
    /// The [`BlockHash`] of the previous [`Block`].
    ///
    /// `None` for the genesis block.
    pub previousblockhash: Option<BlockHash>,
    /// The Median Time Past value for this [`Block`].
    pub mediantime: u64,
    /// This [`Block`]'s nonce.
    pub nonce: u32,
    /// The [`Block`]'s `bits` value, encoded as a [`CompactTarget`].
    pub bits: CompactTarget,
    /// The [`Block`]'s difficulty target value.
    pub difficulty: f64,
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

    pub async fn tip_height(&self) -> Result<BlockHeight, String> {
        let body = self.get("/blocks/tip/height").await?;
        body.trim()
            .parse()
            .map_err(|e| format!("Could not read the chain tip height {:?}: {:?}", body, e))
    }

    /// returns the block height, where the transaction confirmed
    async fn tx_height(&self, txid: &Txid) -> Result<Option<BlockHeight>, String> {
        let body = self.get(&format!("/tx/{}/status", txid)).await?;
        let stat: TxStatus = serde_json::from_str(&body)
            .map_err(|e| format!("Could not read the tx status of {}: {:?}", txid, e))?;
        Ok(if !stat.confirmed {
            None
        } else {
            stat.block_height
        })
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

    /// returns the block height, just before the output was spent
    /// if it was spent but not confirmed, returning the current tip
    /// None, if it is unspent
    async fn output_status(
        &self,
        out: &OutPoint,
        tip_height: BlockHeight,
    ) -> Result<Option<BlockHeight>, String> {
        let body = self
            .get(&format!("/tx/{}/outspend/{}", out.txid, out.vout))
            .await?;
        let stat: OutputStatus = serde_json::from_str(&body).map_err(|e| {
            format!(
                "Could not read the output status of {}/{}: {:?}",
                out.txid, out.vout, e
            )
        })?;

        if stat.spent {
            let spent_state = stat.status.ok_or(format!(
                "Missing confirmation status for {}/{}",
                out.txid, out.vout
            ))?;
            if spent_state.confirmed {
                let confirmed_height = spent_state.block_height.ok_or(format!(
                    "Missing confirmation height for {}/{}",
                    out.txid, out.vout
                ))?;
                return Ok(Some(confirmed_height - 1));
            } else {
                return Ok(Some(tip_height));
            }
        }

        Ok(None)
    }

    async fn block_infos(&self, height: BlockHeight) -> Result<BlockInfo, String> {
        let body = self.get(&format!("/blocks/{}", height)).await?;
        let blocks: Vec<BlockInfo> = serde_json::from_str(&body)
            .map_err(|e| format!("Could not read the block infos: {:?}", e))?;

        Ok(blocks[0].clone())
    }

    /// Collect every spendable UTXO across `addresses`, paired with the output
    /// it refers to, ready to hand to `verify_proof`.
    pub async fn outpoints_for_psbt(
        &self,
        psbt: &Psbt,
        max_confirmation_height: BlockHeight,
    ) -> Result<(Vec<(OutPoint, TxOut)>, Option<BlockHeight>), String> {
        let tip_height = self.tip_height().await?;

        // Several outpoints often share a transaction, so fetch each one once.
        let mut wanted = psbt
            .unsigned_tx
            .input
            .iter()
            .skip(1)
            .map(|txin| txin.previous_output.txid)
            .collect::<Vec<_>>();
        wanted.sort_unstable_by_key(|txid| txid.to_byte_array());
        wanted.dedup();
        let fetched = stream::iter(wanted)
            .map(|txid| async move {
                let tx = self.transaction(txid).await?;
                let tx_height = self.tx_height(&txid).await?.unwrap_or(tip_height);
                Ok((txid, (tx, tx_height)))
            })
            .buffer_unordered(MAX_CONCURRENT_REQUESTS / 2)
            .collect::<Vec<Result<(Txid, (Transaction, BlockHeight)), String>>>()
            .await;

        let transactions = fetched
            .into_iter()
            .collect::<Result<HashMap<Txid, (Transaction, BlockHeight)>, String>>()?;

        // then check if and when the UTXOs were spent
        let spent_heights = stream::iter(&psbt.unsigned_tx.input)
            .skip(1)
            .map(|txin| async move { self.output_status(&txin.previous_output, tip_height).await })
            .buffer_unordered(MAX_CONCURRENT_REQUESTS)
            .collect::<Vec<Result<Option<BlockHeight>, String>>>()
            .await;
        let spent_heights = spent_heights.into_iter().collect::<Result<Vec<_>, _>>()?;
        let unspent_height = spent_heights.iter().fold(None, |acc, spent_height| {
            let acc = acc.map(|sh: BlockHeight| if sh > 1 { sh - 1 } else { sh });
            match (spent_height, acc) {
                (Some(spent_height), Some(acc)) => Some(std::cmp::min(*spent_height, acc)),
                (None, Some(acc)) => Some(acc),
                (Some(spent_height), None) => Some(*spent_height),
                (None, None) => None,
            }
        });

        let augmented_prevouts = psbt
            .unsigned_tx
            .input
            .iter()
            .skip(1)
            .map(|txin| {
                let (tx, tx_height) = transactions
                    .get(&txin.previous_output.txid)
                    .ok_or(format!("Missing transaction {}", txin.previous_output.txid))?;
                Ok::<_, String>((txin, tx, tx_height))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let utxos = augmented_prevouts
            .into_iter()
            .filter(|(_txin, _tx, tx_height)| **tx_height <= max_confirmation_height)
            .map(|(txin, tx, _tx_height)| {
                let txout = tx
                    .output
                    .get(txin.previous_output.vout as usize)
                    .ok_or_else(|| {
                        format!(
                            "Transaction {} has no output {}",
                            txin.previous_output.txid, txin.previous_output.vout
                        )
                    })?
                    .clone();

                Ok::<_, String>((txin.previous_output, txout))
            })
            .collect::<Result<_, _>>()?;

        Ok((utxos, unspent_height))
    }

    /// Returns a timestamp when the miner reported a block to be mined
    pub async fn time_for_block(&self, height: BlockHeight) -> Result<DateTime<Utc>, String> {
        let blockinfo = self.block_infos(height).await?;
        let timestamp = DateTime::from_timestamp(
            blockinfo
                .timestamp
                .try_into()
                .map_err(|e| format!("Failed to convert timestamp: {}", e))?,
            0,
        )
        .ok_or(format!("Failed to construct timestamp"))?;

        Ok(timestamp)
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
