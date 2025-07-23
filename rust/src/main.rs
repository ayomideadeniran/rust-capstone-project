use bitcoincore_rpc::bitcoin::{Address, Amount, Network, SignedAmount};
use bitcoincore_rpc::bitcoin::consensus::deserialize;
use bitcoincore_rpc::{jsonrpc, Auth, Client, Error, RpcApi};
use serde_json::Value;
use std::fs::File;
use std::io::Write;

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "bitcoin"; // from bitcoin.conf
const RPC_PASS: &str = "secret";  // from bitcoin.conf
const MINER_WALLET: &str = "Miner";
const TRADER_WALLET: &str = "Trader";

/// Helper function to create a wallet if it doesn't exist, or load it if it does.
/// This makes the script idempotent and safe to run multiple times.
fn create_or_load_wallet(rpc: &Client, wallet_name: &str) -> Result<(), Error> {
    // The `createwallet` RPC will fail if the wallet already exists.
    // We can ignore that specific error and proceed to load it.
    match rpc.create_wallet(wallet_name, None, None, None, None) {
        Ok(_) => {
            println!("Wallet '{}' created.", wallet_name);
        }
        Err(Error::JsonRpc(jsonrpc::Error::Rpc(json_rpc_err))) => {
            // Error code -4 means wallet already exists.
            if json_rpc_err.code != -4 {
                return Err(Error::JsonRpc(jsonrpc::Error::Rpc(json_rpc_err)));
            }
            println!("Wallet '{}' already exists, loading it.", wallet_name);
        }
        Err(e) => return Err(e),
    }
    // Ensure the wallet is loaded. It might have just been created, or it might
    // already exist (and could be loaded or unloaded).
    match rpc.load_wallet(wallet_name) {
        Ok(_) => {} // Wallet loaded successfully.
        Err(Error::JsonRpc(jsonrpc::Error::Rpc(json_rpc_err))) => {
            // Error code -35 means wallet is already loaded. This is not an error for us.
            if json_rpc_err.code != -35 {
                return Err(Error::JsonRpc(jsonrpc::Error::Rpc(json_rpc_err)));
            }
        }
        Err(e) => return Err(e),
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Connect to Bitcoin Core RPC
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_string(), RPC_PASS.to_string()),
    )?;

    // 2. Create/Load the wallets: 'Miner' and 'Trader'
    create_or_load_wallet(&rpc, MINER_WALLET)?;
    create_or_load_wallet(&rpc, TRADER_WALLET)?;

    // Create wallet-specific clients to interact with each wallet
    let miner_rpc = Client::new(
        &format!("{}/wallet/{}", RPC_URL, MINER_WALLET),
        Auth::UserPass(RPC_USER.to_string(), RPC_PASS.to_string()),
    )?;
    let trader_rpc = Client::new(
        &format!("{}/wallet/{}", RPC_URL, TRADER_WALLET),
        Auth::UserPass(RPC_USER.to_string(), RPC_PASS.to_string()),
    )?;

    // 3. Generate a new address for the Miner
    let miner_address_unchecked = miner_rpc.get_new_address(Some("Mining Reward"), None)?;
    let miner_address = miner_address_unchecked.require_network(Network::Regtest)?;
    println!("Miner address for rewards: {}", miner_address);

    // 4. Mine blocks to make the coinbase reward spendable
    // A coinbase transaction (block reward) is only spendable after 100 confirmations.
    // To get N spendable coinbase rewards, we need to mine 100 + N blocks.
    // Let's mine 110 blocks to get 10 spendable rewards, giving the Miner a
    // much larger starting balance.
    println!("Mining 110 blocks to mature coinbase rewards...");
    miner_rpc.generate_to_address(110, &miner_address.clone().into())?;

    // 5. Print the Miner's balance
    let miner_balance = miner_rpc.get_balance(None, None)?;
    println!("Miner wallet balance: {} BTC", miner_balance.to_btc());

    // 6. Create a receiving address for the Trader
    let trader_address_unchecked = trader_rpc.get_new_address(Some("Received"), None)?;
    let trader_address = trader_address_unchecked.require_network(Network::Regtest)?;
    println!("Trader receiving address: {}", trader_address);

    // 7. Send 0.1 BTC from Miner to Trader.
    // Note: We send a small amount to ensure there are sufficient funds,
    // as the wallet balance can vary between runs.
    let txid = miner_rpc.send_to_address(
        &trader_address.clone().into(),
        Amount::from_btc(0.1)?,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;
    println!("Transaction sent! TXID: {}", txid);

    // 8. Fetch the unconfirmed transaction from the mempool
    let mempool_entry = rpc.call::<Value>("getmempoolentry", &[txid.to_string().into()])?;
    println!(
        "Mempool entry for tx {}:\n{}",
        txid,
        serde_json::to_string_pretty(&mempool_entry)?
    );

    // 9. Confirm the transaction by mining 1 block
    println!("Mining 1 block to confirm the transaction...");
    miner_rpc.generate_to_address(1, &miner_address.clone().into())?;

    // 10. Fetch the confirmed transaction details
    let tx_info = miner_rpc.get_transaction(&txid, Some(true))?;
    let decoded_tx: bitcoincore_rpc::bitcoin::Transaction = deserialize(&tx_info.hex)?;

    // --- Extract details for out.txt ---

    // a. Transaction ID
    let final_txid = tx_info.info.txid;

    // b. Miner's Input Address & Amount
    // For simplicity, we'll display the address from the first input.
    // Note: A transaction can have multiple inputs.
    let previous_outpoint = decoded_tx.input[0].previous_output;
    let input_txid = previous_outpoint.txid;
    let input_vout_n = previous_outpoint.vout;
    let prev_tx_info = miner_rpc.get_transaction(&input_txid, Some(true))?;
    let prev_decoded_tx: bitcoincore_rpc::bitcoin::Transaction = deserialize(&prev_tx_info.hex)?;
    let input_utxo = &prev_decoded_tx.output[input_vout_n as usize];
    let miner_input_address = Address::from_script(input_utxo.script_pubkey.as_ref(), Network::Regtest)?;
    // The total input amount is the sum of all outputs plus the fee.
    let fee = tx_info.fee.unwrap_or(SignedAmount::from_sat(0)).abs();
    let total_output_amount: Amount = decoded_tx.output.iter().map(|o| o.value).sum();
    let miner_input_amount = total_output_amount + fee.to_unsigned()?;

    // c. Trader's Output Address & Amount
    let trader_output = decoded_tx
        .output
        .iter()
        .find(|vout| {
            Address::from_script(vout.script_pubkey.as_ref(), Network::Regtest)
                .map_or(false, |addr| addr == trader_address)
        })
        .ok_or("Trader output not found")?;
    let trader_output_address = Address::from_script(trader_output.script_pubkey.as_ref(), Network::Regtest)?;
    let trader_output_amount = trader_output.value;

    // d. Miner's Change Address & Amount
    let miner_change_output = decoded_tx
        .output
        .iter()
        // A more robust way to find the change output is to find an output
        // that is a valid address but is not the trader's address.
        .find(|vout| {
            if let Ok(addr) = Address::from_script(vout.script_pubkey.as_ref(), Network::Regtest) {
                addr != trader_address
            } else {
                false
            }
        });

    // f. Block height and hash
    let block_height = tx_info.info.blockheight.ok_or("Block height not found")?;
    let block_hash = tx_info.info.blockhash.ok_or("Block hash not found")?;





    // 11. Write the data to ../out.txt
    let mut file = File::create("../out.txt")?;

// a. Transaction ID
writeln!(file, "{}", final_txid)?;
// b. Miner's Input Address & Amount
writeln!(file, "{}", miner_input_address)?;
writeln!(file, "{}", miner_input_amount.to_btc())?;
// c. Trader's Output Address & Amount
writeln!(file, "{}", trader_output_address)?;
writeln!(file, "{}", trader_output_amount.to_btc())?;

// d. Miner's Change Address & Amount
let (miner_change_address_str, miner_change_amount_btc) = if let Some(change_output) = miner_change_output {
    let miner_change_address = Address::from_script(change_output.script_pubkey.as_ref(), Network::Regtest)?;
    let miner_change_amount = change_output.value; // This is of type bitcoin::Amount
    (miner_change_address.to_string(), miner_change_amount.to_btc().to_string())
} else {
    ("None".to_string(), "0".to_string())
};
writeln!(file, "{}", miner_change_address_str)?;
writeln!(file, "{}", miner_change_amount_btc)?;

// e. Transaction Fee
writeln!(file, "{}", fee.to_btc())?;
// f. Block height and hash
writeln!(file, "{}", block_height)?;
writeln!(file, "{}", block_hash)?;

    println!("Successfully wrote transaction details to ../out.txt");

    Ok(())
}