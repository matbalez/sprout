use std::{collections::HashMap, path::Path, str::FromStr, time::Duration};

use cdk::{
    amount::SplitTarget,
    nuts::{CurrencyUnit, MeltOptions, MeltQuoteState, PaymentMethod},
    wallet::{MeltConfirmOptions, MeltOutcome, Wallet},
};
use cdk_redb::WalletRedbDatabase;
use lexe::{
    config::{Network, WalletEnvConfig},
    types::{
        auth::{CredentialsRef, RootSeed},
        bitcoin::{Amount, Offer},
        command::{CreateInvoiceRequest, PayRequest},
        payment::PaymentStatus,
    },
    wallet::LexeWallet,
};
use serde::Deserialize;

const DEFAULT_CASHU_MINT_URL: &str = "https://m7.mountainlake.io/";

#[derive(Deserialize)]
struct ReceiveQuoteRecord {
    quote_id: String,
    offer: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutboundPaymentRecord {
    quote_id: String,
    offer: String,
    amount_sats: u64,
    status: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("summary") if args.len() == 2 => {
            let wallet = load_wallet(Path::new(&args[1]))?;
            print_summary(&wallet).await?;
        }
        Some("offer") if args.len() == 2 => {
            print_offer(&read_arg_or_file(&args[1])?)?;
        }
        Some("receive-offer") if args.len() == 2 => {
            let record: ReceiveQuoteRecord =
                serde_json::from_str(&std::fs::read_to_string(&args[1])?)?;
            println!("receive_quote_id={}", record.quote_id);
            print_offer(&record.offer)?;
        }
        Some("mint-quote") if args.len() == 4 => {
            let wallet = load_wallet(Path::new(&args[1]))?;
            let method = payment_method_arg(&args[2])?;
            let amount_sats = args[3].parse::<u64>()?;
            create_mint_quote(&wallet, method, amount_sats).await?;
        }
        Some("claim-mint") if args.len() == 3 => {
            let wallet = load_wallet(Path::new(&args[1]))?;
            claim_mint_quote(&wallet, &args[2]).await?;
        }
        Some("outbound-offers") if args.len() == 2 => {
            let records: Vec<OutboundPaymentRecord> =
                serde_json::from_str(&std::fs::read_to_string(&args[1])?)?;
            for record in records {
                println!(
                    "outbound_quote_id={} amount_sats={} status={}",
                    record.quote_id, record.amount_sats, record.status
                );
                if record.offer.trim().is_empty() {
                    println!("offer=empty");
                } else {
                    print_offer(&record.offer)?;
                }
                println!();
            }
        }
        Some("lexe-invoice") if args.len() == 3 => {
            let wallet = load_lexe_wallet(Path::new(&args[1]))?;
            let amount_sats = args[2].parse::<u64>()?;
            let amount = Amount::try_from_sats_u64(amount_sats)?;
            let response = wallet
                .create_invoice(CreateInvoiceRequest {
                    expiration_secs: Some(600),
                    amount: Some(amount),
                    description: Some("cashu_diag invoice".to_string()),
                    personal_note: None,
                    partner_pk: None,
                    partner_prop_fee: None,
                    partner_base_fee: None,
                })
                .await?;
            println!("{}", response.invoice);
        }
        Some("fund-from-lexe") if args.len() == 4 => {
            let cashu_wallet = load_wallet(Path::new(&args[1]))?;
            let lexe_wallet = load_lexe_wallet(Path::new(&args[2]))?;
            let amount_sats = args[3].parse::<u64>()?;
            fund_cashu_from_lexe(&cashu_wallet, &lexe_wallet, amount_sats).await?;
        }
        Some("send") if args.len() >= 4 => {
            let skip_swap = args.iter().any(|arg| arg == "--skip-swap");
            let wallet = load_wallet(Path::new(&args[1]))?;
            let offer = read_arg_or_file(&args[2])?;
            let amount_sats = args[3].parse::<u64>()?;
            send_and_poll(&wallet, &offer, amount_sats, skip_swap).await?;
        }
        _ => {
            anyhow::bail!(
                "usage:\n  cashu_diag summary <cashu-dir>\n  cashu_diag offer <offer-or-file>\n  cashu_diag mint-quote <cashu-dir> <bolt11|bolt12> <amount-sats>\n  cashu_diag claim-mint <cashu-dir> <quote-id>\n  cashu_diag fund-from-lexe <cashu-dir> <lexe-root> <amount-sats>\n  cashu_diag send <cashu-dir> <offer-or-file> <amount-sats> [--skip-swap]"
            );
        }
    }

    Ok(())
}

async fn fund_cashu_from_lexe(
    cashu_wallet: &Wallet,
    lexe_wallet: &LexeWallet,
    amount_sats: u64,
) -> anyhow::Result<()> {
    anyhow::ensure!(amount_sats > 0, "amount must be greater than zero");
    let quote = cashu_wallet
        .mint_quote(
            PaymentMethod::BOLT12,
            Some(cdk::Amount::from(amount_sats)),
            None,
            None,
        )
        .await?;
    println!("mint_quote_id={}", quote.id);
    println!("mint_quote_state={:?}", quote.state);
    println!("mint_quote_request_len={}", quote.request.len());

    let amount = Amount::try_from_sats_u64(amount_sats)?;
    let response = lexe_wallet
        .pay(PayRequest {
            payable: quote.request.clone(),
            amount: Some(amount),
            message: Some("cashu_diag fund".to_string()),
            personal_note: Some("cashu_diag fund M7 Cashu test wallet".to_string()),
        })
        .await?;
    println!("lexe_payment_status={:?}", response.status);
    println!("lexe_payment_status_msg={}", response.status_msg);
    anyhow::ensure!(
        response.status == PaymentStatus::Completed,
        "Lexe payment did not complete"
    );

    for attempt in 1..=30 {
        let status = cashu_wallet.check_mint_quote_status(&quote.id).await?;
        println!(
            "mint_status_attempt={} state={:?} mintable={}",
            attempt,
            status.state,
            status.amount_mintable()
        );
        if status.amount_mintable().to_u64() > 0 {
            cashu_wallet
                .mint(&quote.id, SplitTarget::default(), None)
                .await?;
            println!("minted_quote_id={}", quote.id);
            println!("balance_sats={}", cashu_wallet.total_balance().await?);
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    anyhow::bail!("mint quote did not become mintable before timeout")
}

async fn create_mint_quote(
    wallet: &Wallet,
    method: PaymentMethod,
    amount_sats: u64,
) -> anyhow::Result<()> {
    anyhow::ensure!(amount_sats > 0, "amount must be greater than zero");
    let quote = wallet
        .mint_quote(method, Some(cdk::Amount::from(amount_sats)), None, None)
        .await?;
    println!("mint_quote_id={}", quote.id);
    println!("mint_quote_state={:?}", quote.state);
    println!("mint_quote_request={}", quote.request);
    Ok(())
}

async fn claim_mint_quote(wallet: &Wallet, quote_id: &str) -> anyhow::Result<()> {
    for attempt in 1..=30 {
        let status = wallet.check_mint_quote_status(quote_id).await?;
        println!(
            "mint_status_attempt={} state={:?} mintable={}",
            attempt,
            status.state,
            status.amount_mintable()
        );
        if status.amount_mintable().to_u64() > 0 {
            wallet.mint(quote_id, SplitTarget::default(), None).await?;
            println!("minted_quote_id={quote_id}");
            println!("balance_sats={}", wallet.total_balance().await?);
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    anyhow::bail!("mint quote did not become mintable before timeout")
}

fn load_wallet(cashu_dir: &Path) -> anyhow::Result<Wallet> {
    let seed_text = std::fs::read_to_string(cashu_dir.join("seed.hex"))?;
    let seed_bytes = hex::decode(seed_text.trim())?;
    anyhow::ensure!(seed_bytes.len() == 64, "seed must be 64 bytes");
    let mut seed = [0u8; 64];
    seed.copy_from_slice(&seed_bytes);

    let database = WalletRedbDatabase::new(&cashu_dir.join("wallet.redb"))?;
    let mint_url =
        std::env::var("CASHU_DIAG_MINT_URL").unwrap_or_else(|_| DEFAULT_CASHU_MINT_URL.to_string());
    Ok(Wallet::new(
        &mint_url,
        CurrencyUnit::Sat,
        std::sync::Arc::new(database),
        seed,
        None,
    )?)
}

fn payment_method_arg(value: &str) -> anyhow::Result<PaymentMethod> {
    match value.to_ascii_lowercase().as_str() {
        "bolt11" => Ok(PaymentMethod::BOLT11),
        "bolt12" => Ok(PaymentMethod::BOLT12),
        other => anyhow::bail!("unsupported payment method: {other}"),
    }
}

fn load_lexe_wallet(wallet_root: &Path) -> anyhow::Result<LexeWallet> {
    let Some(root_seed) = RootSeed::read_from_path(&wallet_root.join("seedphrase.txt"))? else {
        anyhow::bail!("missing Lexe seedphrase.txt");
    };
    Ok(LexeWallet::load_or_fresh(
        WalletEnvConfig::mainnet(),
        CredentialsRef::from(&root_seed),
        Some(wallet_root.join("data")),
    )?)
}

fn read_arg_or_file(value: &str) -> anyhow::Result<String> {
    let path = Path::new(value);
    if path.exists() {
        Ok(std::fs::read_to_string(path)?.trim().to_string())
    } else {
        Ok(value.trim().to_string())
    }
}

fn print_offer(offer_text: &str) -> anyhow::Result<()> {
    let offer = Offer::from_str(offer_text)?;
    println!("offer_len={}", offer_text.len());
    println!("offer_id={}", offer.id());
    println!("description={:?}", offer.description());
    println!("payee={:?}", offer.payee());
    println!("payee_node_pk={:?}", offer.payee_node_pk());
    println!(
        "supports_mainnet={}",
        offer.supports_network(Network::Mainnet)
    );
    println!(
        "min_amount_sats={:?}",
        offer.min_amount().map(|amount| amount.sats_u64())
    );
    println!("expires_at={:?}", offer.expires_at());
    println!("expects_quantity={}", offer.expects_quantity());
    Ok(())
}

async fn print_summary(wallet: &Wallet) -> anyhow::Result<()> {
    let finalized = wallet.finalize_pending_melts().await?;
    println!("finalized_pending_melts={}", finalized.len());
    for melt in finalized {
        println!(
            "finalized quote={} state={} amount={} fee_paid={}",
            melt.quote_id(),
            melt.state(),
            melt.amount(),
            melt.fee_paid()
        );
    }
    wallet.mint_unissued_quotes().await?;
    println!("balance_sats={}", wallet.total_balance().await?);

    let active = wallet.get_active_melt_quotes().await?;
    println!("active_melt_quotes={}", active.len());
    for quote in active {
        print_melt_quote("active", &quote);
    }

    Ok(())
}

async fn send_and_poll(
    wallet: &Wallet,
    offer: &str,
    amount_sats: u64,
    skip_swap: bool,
) -> anyhow::Result<()> {
    if offer.to_ascii_lowercase().starts_with("lno1") {
        print_offer(offer)?;
    }
    print_summary(wallet).await?;

    let amount_msats = amount_sats
        .checked_mul(1_000)
        .ok_or_else(|| anyhow::anyhow!("amount too large"))?;
    let (payment_method, options) = if offer.to_ascii_lowercase().starts_with("lnbc") {
        (PaymentMethod::BOLT11, None)
    } else {
        (
            PaymentMethod::BOLT12,
            Some(MeltOptions::new_amountless(amount_msats)),
        )
    };
    let quote = wallet
        .melt_quote(payment_method, offer, options, None)
        .await?;
    print_melt_quote("created", &quote);

    let mut metadata = HashMap::new();
    metadata.insert("source".to_string(), "cashu_diag".to_string());
    let prepared = wallet.prepare_melt(&quote.id, metadata).await?;
    let outcome = if skip_swap {
        prepared
            .confirm_prefer_async_with_options(MeltConfirmOptions::skip_swap())
            .await?
    } else {
        prepared.confirm_prefer_async().await?
    };

    match outcome {
        MeltOutcome::Paid(finalized) => {
            println!(
                "confirm=paid quote={} state={} amount={} fee_paid={} proof_present={}",
                finalized.quote_id(),
                finalized.state(),
                finalized.amount(),
                finalized.fee_paid(),
                finalized.payment_proof().is_some()
            );
        }
        MeltOutcome::Pending(_) => {
            println!("confirm=pending quote={}", quote.id);
        }
    }

    for attempt in 1..=18 {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let status = wallet.check_melt_quote_status(&quote.id).await?;
        print_melt_quote(&format!("poll_{attempt}"), &status);
        if matches!(
            status.state,
            MeltQuoteState::Paid | MeltQuoteState::Failed | MeltQuoteState::Unpaid
        ) {
            break;
        }
    }

    print_summary(wallet).await?;
    Ok(())
}

fn print_melt_quote(label: &str, quote: &cdk::wallet::MeltQuote) {
    println!(
        "{label} quote={} state={} amount={} fee_reserve={} expiry={} proof_present={}",
        quote.id,
        quote.state,
        quote.amount,
        quote.fee_reserve,
        quote.expiry,
        quote.payment_proof.is_some()
    );
}
