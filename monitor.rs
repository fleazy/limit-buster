use std::env;
use std::str::FromStr;
use helios_sdk::{
    client::Client,
    types::{Pubkey, Transaction},
};
use tokio::{self, time};
use notify_rust::Notification;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize Helios client with free plan
    let client = Client::new("https://rpc.helius.xyz/c63ca63e-2486-47fd-b159-13e7ca3f45c6")?;

    // Get wallet address from command line args
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Please provide a wallet address");
        std::process::exit(1);
    }
    let wallet = Pubkey::from_str(&args[1])?;

    println!("Monitoring wallet: {}", wallet);

    // Poll for new transactions every 5 seconds
    let mut interval = time::interval(time::Duration::from_secs(5));
    loop {
        interval.tick().await;

        // Get recent transactions
        let txs = client.get_transactions(&wallet, None).await?;

        for tx in txs {
            // Check if transaction is a buy (contains token swap)
            if is_buy_transaction(&tx) {
                Notification::new()
                    .summary("New Buy Transaction")
                    .body(&format!("Wallet {} made a purchase", wallet))
                    .show()?;
            }
        }
    }
}

fn is_buy_transaction(tx: &Transaction) -> bool {
    // Look for token swap program calls
    tx.instructions.iter().any(|ix| {
        // Check for Jupiter or Raydium program IDs
        ix.program_id == "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB" || 
        ix.program_id == "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8"
    })
}
