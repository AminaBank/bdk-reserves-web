use actix_web::{get, middleware, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use bdk::bitcoin::consensus::encode::deserialize;
use bdk::bitcoin::util::psbt::PartiallySignedTransaction;
use bdk::bitcoin::{Address, Network, OutPoint, TxOut};
use bdk::electrum_client::{Client, ElectrumApi};
use bdk_reserves::reserves::verify_proof;
use lazy_static::lazy_static;
use prometheus::{self, register_int_counter, Encoder, IntCounter, TextEncoder};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{env, io, str::FromStr};

#[derive(Debug, Serialize, Deserialize)]
struct ProofOfReserves {
    addresses: Vec<String>,
    message: String,
    proof_psbt: String,
}

lazy_static! {
    static ref POR_SUCCESS_COUNTER: IntCounter =
        register_int_counter!("POR_success", "Successfully validated proof of reserves").unwrap();
}

lazy_static! {
    static ref POR_INVALID_COUNTER: IntCounter =
        register_int_counter!("POR_invalid", "Invalid proof of reserves").unwrap();
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    let address = env::var("BIND_ADDRESS").unwrap_or_else(|_err| match env::var("PORT") {
        Ok(p) => format!("0.0.0.0:{}", p),
        Err(_e) => "localhost:8087".to_string(),
    });

    println!("Starting HTTP server at http://{}.", address);
    println!("You can choose a different address through the BIND_ADDRESS env var.");
    println!("You can choose a different port through the PORT env var.");
    POR_INVALID_COUNTER.reset();
    POR_SUCCESS_COUNTER.reset();

    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Logger::default()) // <- enable logger
            .app_data(web::JsonConfig::default().limit(40960)) // <- limit size of the payload (global configuration)
            .service(web::resource("/proof").route(web::post().to(check_proof)))
            .service(web::resource("/prometheus").route(web::get().to(prometheus)))
            .service(index)
    })
    .bind(address)?
    .run()
    .await
}

#[get("/")]
async fn index() -> impl Responder {
    let html = include_str!("../res/index.html");
    HttpResponse::Ok().content_type("text/html").body(html)
}

async fn prometheus() -> HttpResponse {
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();

    let output = String::from_utf8(buffer.clone()).unwrap();
    //println!("************************\nprometheus stats:\n{}", output);
    HttpResponse::Ok().content_type("text/plain").body(output)
}

async fn check_proof(item: web::Json<ProofOfReserves>, req: HttpRequest) -> HttpResponse {
    println!("request: {:?}", req);
    println!("model: {:?}", item);

    let proof_result =
        handle_ext_reserves(&item.message, &item.proof_psbt, 3, item.addresses.clone());

    let answer = match proof_result {
        Err(e) => {
            POR_INVALID_COUNTER.inc();
            json!({ "error": e })
        }
        Ok(res) => {
            POR_SUCCESS_COUNTER.inc();
            res
        }
    }
    .to_string();
    HttpResponse::Ok().content_type("text/json").body(answer)
}

fn handle_ext_reserves(
    message: &str,
    psbt: &str,
    confirmations: usize,
    addresses: Vec<String>,
) -> Result<serde_json::Value, String> {
    let psbt = base64::decode(psbt).map_err(|e| format!("Base64 decode error: {:?}", e))?;
    let psbt: PartiallySignedTransaction =
        deserialize(&psbt).map_err(|e| format!("PSBT deserialization error: {:?}", e))?;
    if addresses.is_empty() {
        return Err("No address provided".to_string());
    }
    let (server, network) = if addresses[0].starts_with('2') {
        ("ssl://electrum.blockstream.info:60002", Network::Testnet)
    } else {
        ("ssl://electrum.blockstream.info:50002", Network::Bitcoin)
    };
    let client =
        Client::new(server).map_err(|e| format!("Failed to create Electrum client: {:?}", e))?;

    let current_block_height = client
        .block_headers_subscribe()
        .map(|data| data.height)
        .map_err(|e| format!("Failed to get block height: {:?}", e))?;
    let max_confirmation_height = Some(current_block_height - confirmations);

    let outpoints_per_addr = addresses
        .iter()
        .map(|address| {
            let address =
                Address::from_str(address).map_err(|e| format!("Invalid address: {:?}", e))?;
            get_outpoints_for_address(&address, &client, max_confirmation_height)
        })
        .collect::<Result<Vec<Vec<_>>, String>>()?;
    let outpoints_combined = outpoints_per_addr
        .iter()
        .fold(Vec::new(), |mut outpoints, outs| {
            outpoints.append(&mut outs.clone());
            outpoints
        });

    let spendable = verify_proof(&psbt, message, outpoints_combined, network)
        .map_err(|e| format!("{:?}", e))?;

    Ok(json!({ "spendable": spendable }))
}

/// Fetch all the utxos, for a given address.
fn get_outpoints_for_address(
    address: &Address,
    client: &Client,
    max_confirmation_height: Option<usize>,
) -> Result<Vec<(OutPoint, TxOut)>, String> {
    let unspents = client
        .script_list_unspent(&address.script_pubkey())
        .map_err(|e| format!("{:?}", e))?;

    unspents
        .iter()
        .filter(|utxo| {
            utxo.height > 0 && utxo.height <= max_confirmation_height.unwrap_or(usize::MAX)
        })
        .map(|utxo| {
            let tx = match client.transaction_get(&utxo.tx_hash) {
                Ok(tx) => tx,
                Err(e) => {
                    return Err(e).map_err(|e| format!("{:?}", e))?;
                }
            };

            Ok((
                OutPoint {
                    txid: utxo.tx_hash,
                    vout: utxo.tx_pos as u32,
                },
                tx.output[utxo.tx_pos].clone(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{body::to_bytes, dev::Service, http, test, web, App, Error};

    #[actix_web::test]
    async fn test_index() -> Result<(), Error> {
        let app = App::new()
            .route("/proof", web::post().to(check_proof))
            .route("/prometheus", web::get().to(prometheus));
        let app = test::init_service(app).await;

        let req = test::TestRequest::post().uri("/proof")
            .set_json(ProofOfReserves {
                addresses: vec!["2Mtkk3kjyN8hgdGXPuJCNnwS3BBY4K2frhY".to_owned()],
                message: "Stored in SEBA Bank AG cold storage".to_owned(),
                proof_psbt: "cHNidP8BAP03AwEAAAATfUqjtTyZAxfGOsqFi93k3ToGtPZ0E/BZWFlBgAFlt1kAAAAAAP////8VZFle1kNhN87Ee3jTlpqzhPY3376Bee8gryZ4EP0QxQAAAAAA/////xdqWOlIfYFpbDM+ZuBHu05GiQz+EKK/ebafYy50BPwqAAAAAAD/////K6q1ppFH2Ai6FYgXhqAP/i25RVrCNl7/LKkDKAfBedkAAAAAAP////8rqrWmkUfYCLoViBeGoA/+LblFWsI2Xv8sqQMoB8F52QEAAAAA/////yvmR/yPrZNvLPEWPdteixrpIrSe+mjGV0PRHwQvJ3skAAAAAAD/////cuwKmKQFtYW/+/3y8/ePnheAut3yDHv0R7HV22UhJX0AAAAAAP////9y7AqYpAW1hb/7/fLz94+eF4C63fIMe/RHsdXbZSElfQEAAAAA/////4ygvq0AS059XinGKxwy8SqKjRANTF6dU+CDPXemeDqVAAAAAAD/////jKC+rQBLTn1eKcYrHDLxKoqNEA1MXp1T4IM9d6Z4OpUBAAAAAP////+3xGKCPa4t1MGlkJ9jznWYBGdP9XZNMKbW+t7UvnNxzAAAAAAA/////7fEYoI9ri3UwaWQn2POdZgEZ0/1dk0wptb63tS+c3HMAQAAAAD/////wyKNGqQJpgaNszr5mLLEYQV6+lAMfXNndS/mn8PkXJ0AAAAAAP/////DIo0apAmmBo2zOvmYssRhBXr6UAx9c2d1L+afw+RcnQEAAAAA/////9Fninwz/x77J2ghJX0wcVNLRI3f3wMIlh5kePz8l2ZuAAAAAAD/////1AUOKakFoN1BqrDomHASI0VFsLtskXVQpPljoDU8zWsBAAAAAP/////swob+WCNq5562PWB+Z5JOFFogd/20GAr4Vyra6oOIAAAAAAAA/////+zChv5YI2rnnrY9YH5nkk4UWiB3/bQYCvhXKtrqg4gAAQAAAAD/////8pkjhcQSFD62iDk1sC4WLBUPcpKNoeup0O98xe4MF+kAAAAAAP////8BbOw1AwAAAAAZdqkUn3/QltN+0sDj9/DPySS+70/862iIrAAAAAAAAQEKAAAAAAAAAAABUQEHAAABASAoOPwCAAAAABepFBCNSAfpaNUWLsnOLKCLqO4EAl4UhyICAyS3XurSwfnGDore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".to_owned(),
            })
            .to_request();
        let resp = app.call(req).await?;

        assert_eq!(resp.status(), http::StatusCode::OK);

        let response_body = resp.into_body();
        let resp = r#"{"error":"NonSpendableInput(1)"}"#;
        assert_eq!(to_bytes(response_body).await?, resp);

        let req = test::TestRequest::get().uri("/prometheus").to_request();
        let resp = app.call(req).await?;

        assert_eq!(resp.status(), http::StatusCode::OK);

        let response_body = resp.into_body();
        let resp = "# HELP POR_invalid Invalid proof of reserves\n# TYPE POR_invalid counter\nPOR_invalid 1\n";
        assert_eq!(to_bytes(response_body).await?, resp);

        Ok(())
    }
}
