use axum::{routing::post, Router, extract::State, response::IntoResponse, body::Bytes, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
    commitment_config::CommitmentConfig,
};
use reqwest::Client;
use std::sync::Arc;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct WebhookPayload {
    #[serde(rename = "blockTime")]
    block_time: Option<u64>,
    #[serde(default)]
    #[serde(rename = "indexWithinBlock")]
    index_within_block: Option<u64>,
    meta: Option<Meta>,
    slot: Option<u64>,
    transaction: TransactionData,
}

#[derive(Deserialize, Debug)]
struct TransactionData {
    signatures: Vec<String>,
    message: Message,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct Message {
    #[serde(rename = "accountKeys")]
    account_keys: Vec<String>,
    instructions: Vec<Instruction>,
    #[serde(default)]
    #[serde(rename = "addressTableLookups")]
    address_table_lookups: Option<serde_json::Value>,
    header: Header,
    #[serde(rename = "recentBlockhash")]
    recent_blockhash: String,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct Instruction {
    #[serde(rename = "programIdIndex")]
    program_id_index: usize,
    #[serde(default)]
    accounts: Vec<usize>,
    #[serde(default)]
    data: Option<String>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct Header {
    #[serde(rename = "numReadonlySignedAccounts")]
    num_readonly_signed_accounts: u8,
    #[serde(rename = "numReadonlyUnsignedAccounts")]
    num_readonly_unsigned_accounts: u8,
    #[serde(rename = "numRequiredSignatures")]
    num_required_signatures: u8,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Default)]
struct Meta {
    err: Option<serde_json::Value>,
    fee: Option<u64>,
    #[serde(default)]
    #[serde(rename = "innerInstructions")]
    inner_instructions: Vec<InnerInstruction>,
    #[serde(default)]
    #[serde(rename = "loadedAddresses")]
    loaded_addresses: LoadedAddresses,
    #[serde(default)]
    #[serde(rename = "logMessages")]
    log_messages: Vec<String>,
    #[serde(default)]
    #[serde(rename = "postBalances")]
    post_balances: Vec<u64>,
    #[serde(default)]
    #[serde(rename = "postTokenBalances")]
    post_token_balances: Vec<TokenBalance>,
    #[serde(default)]
    #[serde(rename = "preBalances")]
    pre_balances: Vec<u64>,
    #[serde(default)]
    #[serde(rename = "preTokenBalances")]
    pre_token_balances: Vec<TokenBalance>,
    #[serde(default)]
    rewards: Vec<serde_json::Value>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Default)]
struct InnerInstruction {
    index: u64,
    instructions: Vec<Instruction>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Default)]
struct LoadedAddresses {
    #[serde(default)]
    readonly: Vec<String>,
    #[serde(default)]
    writable: Vec<String>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Default)]
struct TokenBalance {
    #[serde(rename = "accountIndex")]
    account_index: u64,
    mint: String,
    owner: String,
    #[serde(rename = "programId")]
    program_id: String,
    #[serde(rename = "uiTokenAmount")]
    ui_token_amount: UiTokenAmount,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Default)]
struct UiTokenAmount {
    amount: String,
    decimals: u8,
    #[serde(rename = "uiAmount")]
    ui_amount: Option<f64>,
    #[serde(rename = "uiAmountString")]
    ui_amount_string: String,
}

#[derive(Deserialize, Debug)]
struct JupiterQuoteResponse {
    data: Vec<JupiterRoute>,
}

#[derive(Deserialize, Debug, Serialize)]
struct JupiterRoute {
    in_amount: String,
    out_amount: String,
    #[serde(rename = "marketInfos")]
    market_infos: Vec<MarketInfo>,
}

#[derive(Deserialize, Debug, Serialize)]
struct MarketInfo {
    id: String,
    label: String,
}

#[derive(Serialize, Debug)]
struct JupiterSwapRequest {
    route: JupiterRoute,
    user_public_key: String,
    wrap_and_unwrap_sol: bool,
}

#[derive(Clone)]
struct AppState {
    wallet: String,
    rpc_client: Arc<RpcClient>,
    keypair: Arc<Keypair>,
    http_client: Arc<Client>,
    helius_api_key: String,
}

async fn webhook_handler(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
    let raw_payload = String::from_utf8_lossy(&body);
    println!("Raw payload: {}", raw_payload);

    match serde_json::from_slice::<Vec<WebhookPayload>>(&body) {
        Ok(payload) => {
            println!("Deserialized payload: {:?}", payload);
            for tx in &payload {
                if is_buy_transaction(tx) {
                    let signature = tx.transaction.signatures.get(0).map(|s| s.as_str()).unwrap_or("unknown");
                    println!("Buy detected for tx: {}", signature);

                    let log_message = format!(
                        "Buy detected: Wallet {} made a purchase - Tx: {}\n",
                        state.wallet, signature
                    );
                    let mut file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("/var/log/wallet-monitor.log")
                        .unwrap_or_else(|e| {
                            eprintln!("Failed to open log file: {}", e);
                            std::fs::File::create("/var/log/wallet-monitor.log").unwrap()
                        });
                    if let Err(e) = writeln!(file, "{}", log_message) {
                        eprintln!("Failed to write to log file: {}", e);
                    }

                    if let Some(token_mint) = extract_token_mint(tx) {
                        match perform_copytrade_swap(&state, &token_mint).await {
                            Ok(tx_signature) => println!("Copytrade swap executed: {}", tx_signature),
                            Err(e) => eprintln!("Failed to execute copytrade swap: {}", e),
                        }
                    } else {
                        println!("Could not determine token mint for tx: {}", signature);
                    }
                } else {
                    println!("No buy detected for tx: {:?}", tx.transaction.signatures.get(0));
                }
            }
        }
        Err(e) => println!("Deserialization failed: {}", e),
    }
    StatusCode::OK
}

fn is_buy_transaction(tx: &WebhookPayload) -> bool {
    let account_keys = &tx.transaction.message.account_keys;
    let default_key = String::new();
    tx.transaction.message.instructions.iter().any(|ix| {
        let program_id = account_keys.get(ix.program_id_index).unwrap_or(&default_key);
        program_id == "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB" ||
        program_id == "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8"
    })
}

fn extract_token_mint(tx: &WebhookPayload) -> Option<String> {
    let meta = tx.meta.as_ref()?;
    for token_balance in &meta.post_token_balances {
        if token_balance.ui_token_amount.ui_amount.unwrap_or(0.0) > 0.0 {
            let pre_balance = meta.pre_token_balances.iter().find(|pre| pre.mint == token_balance.mint);
            if pre_balance.is_none() || pre_balance.unwrap().ui_token_amount.ui_amount.unwrap_or(0.0) == 0.0 {
                return Some(token_balance.mint.clone());
            }
        }
    }
    None
}

async fn perform_copytrade_swap(state: &AppState, token_mint: &str) -> Result<String, Box<dyn std::error::Error>> {
    let quote_url = format!(
        "https://quote-api.jup.ag/v4/quote?inputMint=So11111111111111111111111111111111111111112&outputMint={}&amount=1000000&slippage=0.5",
        token_mint
    );

    let quote_response: JupiterQuoteResponse = state.http_client
        .get(&quote_url) // Fixed: Use quote_url instead of "e_url"
        .send()
        .await?
        .json()
        .await?;
    let route = quote_response.data.into_iter().next().ok_or("No swap route found")?;

    let swap_request = JupiterSwapRequest {
        route,
        user_public_key: state.keypair.pubkey().to_string(),
        wrap_and_unwrap_sol: true,
    };

    let swap_response: serde_json::Value = state.http_client
        .post("https://quote-api.jup.ag/v4/swap")
        .json(&swap_request)
        .send()
        .await?
        .json()
        .await?;
    let serialized_tx = swap_response["swapTransaction"]
        .as_str()
        .ok_or("No swap transaction returned")?;

    let decoded_tx = BASE64_STANDARD.decode(serialized_tx)?;
    let mut tx: Transaction = bincode::deserialize(&decoded_tx)?;
    tx.sign(&[state.keypair.as_ref()], tx.message.recent_blockhash);

    let signature = state.rpc_client.send_and_confirm_transaction_with_spinner_and_commitment(
        &tx,
        CommitmentConfig::confirmed(),
    )?;

    Ok(signature.to_string())
}

/*
// Commented out health check task causing 404
async fn health_check_task(http_client: Arc<Client>, helius_api_key: String) {
    let mut interval = interval(Duration::from_secs(1));
    let health_url = format!("https://mainnet.helius-rpc.com/health?api-key={}", helius_api_key);

    loop {
        interval.tick().await;
        match http_client.get(&health_url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    println!("RPC health check: OK");
                } else {
                    eprintln!("RPC health check failed: {}", response.status());
                }
            }
            Err(e) => eprintln!("Health check request failed: {}", e),
        }
    }
}
*/

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Please provide a wallet address");
        std::process::exit(1);
    }
    let wallet = args[1].clone();

    match dotenv::dotenv() {
        Ok(_) => println!("Loaded .env file"),
        Err(e) => eprintln!("Warning: Could not load .env file: {}", e),
    }

    let secret_key_str = env::var("SECRET_KEY").map_err(|e| {
        eprintln!("Error: SECRET_KEY not found in environment: {}", e);
        e
    })?;
    println!("SECRET_KEY read from env: {}", secret_key_str); // Debug output
    let secret_key: Vec<u8> = serde_json::from_str(&secret_key_str).map_err(|e| {
        eprintln!("Error parsing SECRET_KEY as JSON: {}", e);
        e
    })?;
    let helius_api_key = env::var("HELIUS_API_KEY").map_err(|e| {
        eprintln!("Error: HELIUS_API_KEY not found in environment: {}", e);
        e
    })?;
    let keypair = Arc::new(Keypair::from_bytes(&secret_key)?);

    let rpc_url = format!("https://mainnet.helius-rpc.com/?api-key={}", helius_api_key);
    let rpc_client = Arc::new(RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed()));
    let http_client = Arc::new(Client::new());

    let state = AppState {
        wallet,
        rpc_client,
        keypair,
        http_client: http_client.clone(),
        helius_api_key,
    };

    // Commented out spawning the health check task
    // tokio::spawn(health_check_task(http_client, state.helius_api_key.clone()));

    let app = Router::new()
        .route("/notify", post(webhook_handler))
        .with_state(state);

    let addr = "0.0.0.0:3000".parse()?;
    println!("Notification server listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}