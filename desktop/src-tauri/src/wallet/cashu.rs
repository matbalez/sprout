use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use cdk::{
    amount::SplitTarget,
    nuts::{CurrencyUnit, MeltOptions, MeltQuoteState, PaymentMethod},
    types::FinalizedMelt,
    wallet::{MeltOutcome, MintQuote, Wallet},
};
use cdk_redb::WalletRedbDatabase;
use redb::TableDefinition;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    storage::{write_atomic_secret_text, write_atomic_text, WalletStorage},
    types::{validate_cashu_mint_url, WalletTransaction, WALLET_BOLT12_OFFER_DESCRIPTION},
};

pub(crate) const CASHU_BOLT12_RECEIVE_RAIL: &str = "cashu-mint-bolt12";
pub(crate) const CASHU_BOLT12_SEND_RAIL: &str = "cashu-melt-bolt12";
pub(crate) const CASHU_BOLT11_SEND_RAIL: &str = "cashu-melt-bolt11";
const CASHU_RECEIVE_QUOTE_FILE_NAME: &str = "bolt12_receive_quote.json";
const CASHU_OUTBOUND_PAYMENTS_FILE_NAME: &str = "bolt12_outbound_payments.json";
const CASHU_PAYMENT_STATUS_COMPLETED: &str = "completed";
const CASHU_PAYMENT_STATUS_FAILED: &str = "failed";
const CASHU_PAYMENT_STATUS_PENDING: &str = "pending";
const CASHU_PAYMENT_PENDING_MESSAGE: &str = "Payment is pending at the Cashu mint";
const CASHU_WALLET_SAGAS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("wallet_sagas");

#[derive(Clone)]
pub(crate) struct CashuWallet {
    wallet: Arc<Wallet>,
    mint_url: String,
    receive_quote_path: PathBuf,
    outbound_payments_path: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
struct CashuReceiveQuoteRecord {
    quote_id: String,
    offer: String,
    mint_url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CashuOutboundPaymentRecord {
    quote_id: String,
    offer: String,
    mint_url: String,
    #[serde(default = "default_cashu_outbound_rail")]
    rail: String,
    #[serde(default)]
    payment_method: String,
    amount_sats: u64,
    fees_sats: u64,
    #[serde(default)]
    fee_reserve_sats: Option<u64>,
    #[serde(default)]
    quote_state: Option<String>,
    #[serde(default)]
    quote_expiry: Option<u64>,
    status: String,
    status_message: String,
    created_at_ms: u64,
    updated_at_ms: u64,
    #[serde(default)]
    confirm_started_at_ms: Option<u64>,
    #[serde(default)]
    confirm_completed_at_ms: Option<u64>,
    #[serde(default)]
    last_mint_check_at_ms: Option<u64>,
    #[serde(default)]
    last_mint_state: Option<String>,
    #[serde(default)]
    diagnostics: Vec<CashuOutboundDiagnosticEvent>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CashuOutboundDiagnosticEvent {
    at_ms: u64,
    stage: String,
    detail: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CashuDiagnosticsReport {
    provider: String,
    mint_url: String,
    generated_at_ms: u64,
    cdk_version: String,
    mint_info: Option<CashuMintDiagnosticInfo>,
    mint_info_error: Option<String>,
    outbound_payments_path: String,
    outbound_payments: Vec<CashuOutboundDiagnosticPayment>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CashuMintDiagnosticInfo {
    name: Option<String>,
    version: Option<String>,
    nut04_bolt12_sat: bool,
    nut05_bolt12_sat: bool,
    nut05_bolt11_sat: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CashuOutboundDiagnosticPayment {
    quote_id: String,
    mint_url: String,
    rail: String,
    payment_method: String,
    offer_preview: String,
    offer_sha256: String,
    amount_sats: u64,
    fees_sats: u64,
    fee_reserve_sats: Option<u64>,
    quote_state: Option<String>,
    quote_expiry: Option<u64>,
    status: String,
    status_message: String,
    created_at_ms: u64,
    updated_at_ms: u64,
    confirm_started_at_ms: Option<u64>,
    confirm_completed_at_ms: Option<u64>,
    confirm_elapsed_ms: Option<u64>,
    last_mint_check_at_ms: Option<u64>,
    last_mint_state: Option<String>,
    diagnostics: Vec<CashuOutboundDiagnosticEvent>,
}

impl CashuWallet {
    pub(crate) async fn load_or_create(
        storage: &WalletStorage,
        mint_url: &str,
    ) -> Result<Self, String> {
        storage.ensure_dirs()?;
        let mint_url = validate_cashu_mint_url(mint_url)?;
        let cashu_storage = storage.cashu_storage_for_mint(&mint_url)?;
        ensure_private_dir(&cashu_storage.dir, "Cashu wallet directory")?;
        ensure_cashu_wallet_database_saga_table(&cashu_storage.db_path)?;

        let seed = load_or_create_seed(&cashu_storage.seed_path)?;
        let database = WalletRedbDatabase::new(&cashu_storage.db_path)
            .map_err(|error| format!("open Cashu wallet database: {error}"))?;
        let wallet = Wallet::new(&mint_url, CurrencyUnit::Sat, Arc::new(database), seed, None)
            .map_err(|error| format!("load Cashu wallet: {error}"))?;

        let wallet = Self {
            wallet: Arc::new(wallet),
            mint_url,
            receive_quote_path: cashu_storage.dir.join(CASHU_RECEIVE_QUOTE_FILE_NAME),
            outbound_payments_path: cashu_storage.dir.join(CASHU_OUTBOUND_PAYMENTS_FILE_NAME),
        };
        wallet.verify_bolt12_capabilities().await?;
        Ok(wallet)
    }

    pub(crate) async fn balance_sats(&self) -> Result<u64, String> {
        self.wallet
            .total_balance()
            .await
            .map(|amount| amount.to_u64())
            .map_err(|error| format!("load Cashu balance: {error}"))
    }

    pub(crate) async fn sync(&self) -> Result<(), String> {
        self.sync_tracked_receive_quote().await?;
        let finalized_melts = self
            .wallet
            .finalize_pending_melts()
            .await
            .map_err(|error| format!("finalize pending Cashu melts: {error}"))?;
        self.apply_finalized_melts(&finalized_melts)?;
        self.wallet
            .mint_unissued_quotes()
            .await
            .map(|_| ())
            .map_err(|error| format!("claim paid Cashu BOLT12 mint quotes: {error}"))
    }

    pub(crate) async fn ensure_bolt12_offer(
        &self,
        cached_offer: Option<&str>,
    ) -> Result<String, String> {
        self.verify_bolt12_receive_capability().await?;
        let receive_quote_record = self.load_receive_quote_record()?;
        let normalized_mint_url = self.normalized_mint_url();

        if let Some(record) = receive_quote_record.as_ref() {
            if record.mint_url == normalized_mint_url && !record.offer.trim().is_empty() {
                match self.wallet.check_mint_quote_status(&record.quote_id).await {
                    Ok(quote) => {
                        let offer = quote.request.trim();
                        if !offer.is_empty() {
                            self.save_receive_quote_record(&quote)?;
                            return Ok(offer.to_string());
                        }
                    }
                    Err(error) => {
                        eprintln!(
                            "buzz-desktop: failed to refresh cached Cashu BOLT12 quote {}: {error}",
                            record.quote_id
                        );
                    }
                }
            }
        }

        let can_reuse_cached_offer = receive_quote_record
            .as_ref()
            .is_none_or(|record| record.mint_url == normalized_mint_url);
        if can_reuse_cached_offer {
            if let Some(cached_offer) = cached_offer
                .map(str::trim)
                .filter(|offer| !offer.is_empty())
            {
                if let Some(quote) = self
                    .find_local_receive_quote_for_offer(cached_offer)
                    .await?
                {
                    self.save_receive_quote_record(&quote)?;
                    return Ok(cached_offer.to_string());
                }
            }
        }

        self.create_receive_quote().await
    }

    pub(crate) async fn generate_bolt12_offer(&self) -> Result<String, String> {
        self.verify_bolt12_receive_capability().await?;
        self.create_receive_quote().await
    }

    async fn create_receive_quote(&self) -> Result<String, String> {
        let quote = self
            .wallet
            .mint_quote(PaymentMethod::BOLT12, None, None, None)
            .await
            .map_err(|error| format!("create Cashu BOLT12 mint quote: {error}"))?;
        let offer = quote.request.trim();
        if offer.is_empty() {
            return Err("Cashu mint returned an empty BOLT12 offer".to_string());
        }
        self.save_receive_quote_record(&quote)?;
        Ok(offer.to_string())
    }

    pub(crate) async fn send(&self, destination: &str, amount_sats: u64) -> Result<String, String> {
        if amount_sats == 0 {
            return Err("Cashu payment amount must be greater than zero".to_string());
        }
        let target = cashu_send_target(destination, amount_sats)?;
        match target.payment_method {
            PaymentMethod::BOLT11 => self.verify_bolt11_send_capability().await?,
            PaymentMethod::BOLT12 => self.verify_bolt12_send_capability().await?,
            _ => return Err("Cashu payment method is not supported".to_string()),
        }

        let amount_msats = amount_sats
            .checked_mul(1_000)
            .ok_or_else(|| "Cashu payment amount is too large".to_string())?;
        let options = match target.payment_method {
            PaymentMethod::BOLT11 | PaymentMethod::BOLT12 => {
                Some(MeltOptions::new_amountless(amount_msats))
            }
            _ => None,
        };
        let quote = self
            .wallet
            .melt_quote(
                target.payment_method.clone(),
                target.destination,
                options,
                None,
            )
            .await
            .map_err(|error| format!("create Cashu {} melt quote: {error}", target.label))?;
        let quote_id = quote.id.clone();
        let quote_state = format!("{:?}", quote.state);
        let created_at_ms = unix_time_ms();
        let mut metadata = HashMap::new();
        metadata.insert("provider".to_string(), "cashu".to_string());
        metadata.insert("rail".to_string(), target.rail.to_string());
        metadata.insert(
            "description".to_string(),
            WALLET_BOLT12_OFFER_DESCRIPTION.to_string(),
        );
        let prepared = self
            .wallet
            .prepare_melt(&quote.id, metadata)
            .await
            .map_err(|error| format!("prepare Cashu BOLT12 payment: {error}"))?;

        let confirm_started_at_ms = unix_time_ms();
        self.upsert_outbound_payment(CashuOutboundPaymentRecord {
            quote_id: quote_id.clone(),
            offer: destination.trim().to_string(),
            mint_url: self.normalized_mint_url(),
            rail: target.rail.to_string(),
            payment_method: target.payment_method.to_string(),
            amount_sats: quote.amount.to_u64(),
            fees_sats: quote.fee_reserve.to_u64(),
            fee_reserve_sats: Some(quote.fee_reserve.to_u64()),
            quote_state: Some(quote_state.clone()),
            quote_expiry: Some(quote.expiry),
            status: CASHU_PAYMENT_STATUS_PENDING.to_string(),
            status_message: CASHU_PAYMENT_PENDING_MESSAGE.to_string(),
            created_at_ms,
            updated_at_ms: confirm_started_at_ms,
            confirm_started_at_ms: Some(confirm_started_at_ms),
            confirm_completed_at_ms: None,
            last_mint_check_at_ms: None,
            last_mint_state: None,
            diagnostics: vec![
                cashu_diagnostic_event(
                    created_at_ms,
                    "melt_quote_created",
                    format!(
                        "created {label} melt quote with state {state}, amount {amount_sats} sats, fee reserve {fee_sats} sats",
                        label = target.label,
                        state = quote_state,
                        amount_sats = quote.amount.to_u64(),
                        fee_sats = quote.fee_reserve.to_u64()
                    ),
                ),
                cashu_diagnostic_event(
                    confirm_started_at_ms,
                    "confirm_started",
                    "submitted melt with Prefer: respond-async".to_string(),
                ),
            ],
        })?;

        match prepared.confirm_prefer_async().await {
            Ok(MeltOutcome::Paid(finalized)) => {
                self.append_outbound_payment_diagnostic(
                    &quote_id,
                    "confirm_completed",
                    format!("mint finalized quote with state {:?}", finalized.state()),
                    Some(format!("{:?}", finalized.state())),
                    Some(unix_time_ms()),
                )?;
                self.mark_outbound_payment_finalized(&finalized)?;
                if finalized.state() != MeltQuoteState::Paid {
                    return Err(format!(
                        "Cashu {} payment {quote_id} ended with status {}",
                        target.label,
                        finalized.state(),
                    ));
                }
            }
            Ok(MeltOutcome::Pending(_pending)) => {
                self.append_outbound_payment_diagnostic(
                    &quote_id,
                    "confirm_pending",
                    "mint accepted async melt request; quote remains pending locally".to_string(),
                    Some("PENDING".to_string()),
                    Some(unix_time_ms()),
                )?;
                self.refresh_outbound_melt_quote_status(&quote_id).await?;
            }
            Err(error) => {
                self.append_outbound_payment_diagnostic(
                    &quote_id,
                    "confirm_error",
                    format!("confirm_prefer_async returned error: {error}"),
                    None,
                    Some(unix_time_ms()),
                )?;
                self.update_outbound_payment_status(
                    &quote_id,
                    CASHU_PAYMENT_STATUS_FAILED,
                    &format!("Cashu mint rejected the {} payment: {error}", target.label),
                    Some(quote.amount.to_u64()),
                    Some(quote.fee_reserve.to_u64()),
                )?;
                return Err(format!("send Cashu {} payment: {error}", target.label));
            }
        }
        Ok(quote_id)
    }

    pub(crate) async fn diagnostics_report(&self) -> Result<CashuDiagnosticsReport, String> {
        let (mint_info, mint_info_error) = match self.fetch_mint_info().await {
            Ok(info) => (Some(cashu_mint_diagnostic_info(info)), None),
            Err(error) => (None, Some(error)),
        };
        let mut outbound_payments = self.load_outbound_payments()?;
        outbound_payments.sort_by(|left, right| right.created_at_ms.cmp(&left.created_at_ms));

        Ok(CashuDiagnosticsReport {
            provider: "cashu".to_string(),
            mint_url: self.normalized_mint_url(),
            generated_at_ms: unix_time_ms(),
            cdk_version: "0.17.0".to_string(),
            mint_info,
            mint_info_error,
            outbound_payments_path: self.outbound_payments_path.to_string_lossy().to_string(),
            outbound_payments: outbound_payments
                .into_iter()
                .map(cashu_outbound_diagnostic_payment)
                .collect(),
        })
    }

    pub(crate) async fn transactions(
        &self,
        limit: usize,
        sync_first: bool,
    ) -> Result<Vec<WalletTransaction>, String> {
        if sync_first {
            self.sync().await?;
        }
        self.record_untracked_pending_melts().await?;

        let mut outbound_payments = self.load_outbound_payments()?;
        let outbound_fee_reserves = outbound_payments
            .iter()
            .filter_map(|payment| {
                cashu_record_fee_reserve_sats(payment)
                    .map(|fee_reserve_sats| (payment.quote_id.clone(), fee_reserve_sats))
            })
            .collect::<HashMap<_, _>>();
        let mut wallet_transactions: Vec<WalletTransaction> = self
            .wallet
            .list_transactions(None)
            .await
            .map_err(|error| format!("list Cashu transactions: {error}"))
            .map(|transactions| {
                transactions
                    .into_iter()
                    .take(limit)
                    .map(|transaction| {
                        let direction = format!("{:?}", transaction.direction).to_ascii_lowercase();
                        let rail =
                            cashu_payment_rail(&direction, transaction.payment_method.as_ref());
                        let amount_sats = transaction.amount.to_u64();
                        let quote_id = transaction.quote_id.clone();
                        let fees_sats = quote_id
                            .as_ref()
                            .and_then(|quote_id| outbound_fee_reserves.get(quote_id))
                            .map(|fee_reserve_sats| {
                                cashu_display_fee_sats(transaction.fee.to_u64(), *fee_reserve_sats)
                            })
                            .unwrap_or_else(|| transaction.fee.to_u64());
                        let created_at_ms = transaction.timestamp.saturating_mul(1_000);

                        WalletTransaction {
                            id: quote_id.unwrap_or_else(|| transaction.id().to_string()),
                            rail,
                            kind: "offer".to_string(),
                            direction,
                            status: "completed".to_string(),
                            status_message: String::new(),
                            amount_sats: Some(amount_sats),
                            fees_sats,
                            message: transaction.memo,
                            personal_note: transaction.payment_request,
                            created_at_ms,
                            updated_at_ms: created_at_ms,
                            agent_payment: None,
                        }
                    })
                    .collect()
            })?;

        let existing_quote_ids = wallet_transactions
            .iter()
            .map(|transaction| transaction.id.clone())
            .collect::<std::collections::HashSet<_>>();
        outbound_payments.retain(|payment| !existing_quote_ids.contains(&payment.quote_id));
        wallet_transactions.extend(
            outbound_payments
                .into_iter()
                .map(cashu_outbound_record_transaction),
        );
        wallet_transactions.sort_by(|left, right| right.created_at_ms.cmp(&left.created_at_ms));
        wallet_transactions.truncate(limit);

        Ok(wallet_transactions)
    }

    async fn verify_bolt12_capabilities(&self) -> Result<(), String> {
        self.verify_bolt12_receive_capability().await?;
        self.verify_bolt12_send_capability().await
    }

    async fn verify_bolt12_receive_capability(&self) -> Result<(), String> {
        let info = self.fetch_mint_info().await?;
        if !info.nuts.nut04.disabled
            && info
                .nuts
                .nut04
                .get_settings(&CurrencyUnit::Sat, &PaymentMethod::BOLT12)
                .is_some()
        {
            return Ok(());
        }
        Err(format!(
            "Cashu mint {} does not advertise BOLT12 mint support for sats",
            self.mint_url
        ))
    }

    async fn verify_bolt12_send_capability(&self) -> Result<(), String> {
        let info = self.fetch_mint_info().await?;
        if !info.nuts.nut05.disabled
            && info
                .nuts
                .nut05
                .get_settings(&CurrencyUnit::Sat, &PaymentMethod::BOLT12)
                .is_some()
        {
            return Ok(());
        }
        Err(format!(
            "Cashu mint {} does not advertise BOLT12 melt support for sats",
            self.mint_url
        ))
    }

    async fn verify_bolt11_send_capability(&self) -> Result<(), String> {
        let info = self.fetch_mint_info().await?;
        if !info.nuts.nut05.disabled
            && info
                .nuts
                .nut05
                .get_settings(&CurrencyUnit::Sat, &PaymentMethod::BOLT11)
                .is_some()
        {
            return Ok(());
        }
        Err(format!(
            "Cashu mint {} does not advertise BOLT11 melt support for sats",
            self.mint_url
        ))
    }

    async fn fetch_mint_info(&self) -> Result<cdk::nuts::MintInfo, String> {
        self.wallet
            .fetch_mint_info()
            .await
            .map_err(|error| format!("fetch Cashu mint info from {}: {error}", self.mint_url))?
            .ok_or_else(|| format!("Cashu mint {} did not return mint info", self.mint_url))
    }

    async fn sync_tracked_receive_quote(&self) -> Result<(), String> {
        let Some(record) = self.load_receive_quote_record()? else {
            return Ok(());
        };
        if record.mint_url != self.normalized_mint_url() {
            return Ok(());
        }

        let quote = self
            .wallet
            .check_mint_quote_status(&record.quote_id)
            .await
            .map_err(|error| {
                format!(
                    "check Cashu BOLT12 receive quote {} with mint: {error}",
                    record.quote_id
                )
            })?;
        if quote.amount_mintable().to_u64() > 0 {
            self.wallet
                .mint(&record.quote_id, SplitTarget::default(), None)
                .await
                .map_err(|error| {
                    format!(
                        "claim paid Cashu BOLT12 receive quote {}: {error}",
                        record.quote_id
                    )
                })?;
        }
        Ok(())
    }

    async fn find_local_receive_quote_for_offer(
        &self,
        offer: &str,
    ) -> Result<Option<MintQuote>, String> {
        let quotes = self
            .wallet
            .get_unissued_mint_quotes()
            .await
            .map_err(|error| format!("load local Cashu BOLT12 mint quotes: {error}"))?;
        Ok(quotes.into_iter().find(|quote| {
            quote.payment_method == PaymentMethod::BOLT12 && quote.request.trim() == offer
        }))
    }

    fn load_receive_quote_record(&self) -> Result<Option<CashuReceiveQuoteRecord>, String> {
        match std::fs::read_to_string(&self.receive_quote_path) {
            Ok(value) => serde_json::from_str(&value)
                .map(Some)
                .map_err(|error| format!("parse Cashu BOLT12 receive quote metadata: {error}")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("read Cashu BOLT12 receive quote metadata: {error}")),
        }
    }

    fn save_receive_quote_record(&self, quote: &MintQuote) -> Result<(), String> {
        let record = CashuReceiveQuoteRecord {
            quote_id: quote.id.clone(),
            offer: quote.request.trim().to_string(),
            mint_url: self.normalized_mint_url(),
        };
        let content = serde_json::to_string_pretty(&record)
            .map_err(|error| format!("serialize Cashu BOLT12 receive quote metadata: {error}"))?;
        write_atomic_text(&self.receive_quote_path, &content)
    }

    fn load_outbound_payments(&self) -> Result<Vec<CashuOutboundPaymentRecord>, String> {
        match std::fs::read_to_string(&self.outbound_payments_path) {
            Ok(value) => serde_json::from_str(&value)
                .map_err(|error| format!("parse Cashu BOLT12 outbound payment records: {error}")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(format!(
                "read Cashu BOLT12 outbound payment records: {error}"
            )),
        }
    }

    fn save_outbound_payments(&self, records: &[CashuOutboundPaymentRecord]) -> Result<(), String> {
        let content = serde_json::to_string_pretty(records)
            .map_err(|error| format!("serialize Cashu BOLT12 outbound payment records: {error}"))?;
        write_atomic_text(&self.outbound_payments_path, &content)
    }

    fn upsert_outbound_payment(&self, record: CashuOutboundPaymentRecord) -> Result<(), String> {
        let mut records = self.load_outbound_payments()?;
        match records
            .iter_mut()
            .find(|existing| existing.quote_id == record.quote_id)
        {
            Some(existing) => *existing = record,
            None => records.push(record),
        }
        self.save_outbound_payments(&records)
    }

    fn apply_finalized_melts(&self, melts: &[FinalizedMelt]) -> Result<(), String> {
        for melt in melts {
            self.mark_outbound_payment_finalized(melt)?;
        }
        Ok(())
    }

    fn mark_outbound_payment_finalized(&self, melt: &FinalizedMelt) -> Result<(), String> {
        let status = if melt.state() == MeltQuoteState::Paid {
            CASHU_PAYMENT_STATUS_COMPLETED
        } else {
            CASHU_PAYMENT_STATUS_FAILED
        };
        let status_message = cashu_melt_status_message(melt.state());
        let fees_sats = self
            .load_outbound_payments()?
            .into_iter()
            .find(|record| record.quote_id == melt.quote_id())
            .and_then(|record| cashu_record_fee_reserve_sats(&record))
            .map(|fee_reserve_sats| {
                cashu_display_fee_sats(melt.fee_paid().to_u64(), fee_reserve_sats)
            })
            .unwrap_or_else(|| melt.fee_paid().to_u64());
        self.update_outbound_payment_status(
            melt.quote_id(),
            status,
            &status_message,
            Some(melt.amount().to_u64()),
            Some(fees_sats),
        )
    }

    fn update_outbound_payment_status(
        &self,
        quote_id: &str,
        status: &str,
        status_message: &str,
        amount_sats: Option<u64>,
        fees_sats: Option<u64>,
    ) -> Result<(), String> {
        let mut records = self.load_outbound_payments()?;
        let now = unix_time_ms();
        if let Some(record) = records
            .iter_mut()
            .find(|record| record.quote_id == quote_id)
        {
            if let Some(amount_sats) = amount_sats {
                record.amount_sats = amount_sats;
            }
            if let Some(fees_sats) = fees_sats {
                record.fees_sats = fees_sats;
            }
            record.status = status.to_string();
            record.status_message = status_message.to_string();
            record.updated_at_ms = now;
        } else if let Some(amount_sats) = amount_sats {
            records.push(CashuOutboundPaymentRecord {
                quote_id: quote_id.to_string(),
                offer: String::new(),
                mint_url: self.normalized_mint_url(),
                rail: default_cashu_outbound_rail(),
                payment_method: String::new(),
                amount_sats,
                fees_sats: fees_sats.unwrap_or(0),
                fee_reserve_sats: fees_sats,
                quote_state: None,
                quote_expiry: None,
                status: status.to_string(),
                status_message: status_message.to_string(),
                created_at_ms: now,
                updated_at_ms: now,
                confirm_started_at_ms: None,
                confirm_completed_at_ms: None,
                last_mint_check_at_ms: None,
                last_mint_state: None,
                diagnostics: vec![cashu_diagnostic_event(
                    now,
                    "status_updated",
                    status_message.to_string(),
                )],
            });
        }
        self.save_outbound_payments(&records)
    }

    fn append_outbound_payment_diagnostic(
        &self,
        quote_id: &str,
        stage: &str,
        detail: String,
        last_mint_state: Option<String>,
        confirm_completed_at_ms: Option<u64>,
    ) -> Result<(), String> {
        let mut records = self.load_outbound_payments()?;
        let now = unix_time_ms();
        if let Some(record) = records
            .iter_mut()
            .find(|record| record.quote_id == quote_id)
        {
            if let Some(last_mint_state) = last_mint_state {
                record.last_mint_state = Some(last_mint_state);
                record.last_mint_check_at_ms = Some(now);
            }
            if let Some(confirm_completed_at_ms) = confirm_completed_at_ms {
                record.confirm_completed_at_ms = Some(confirm_completed_at_ms);
            }
            record.updated_at_ms = now;
            record
                .diagnostics
                .push(cashu_diagnostic_event(now, stage, detail));
        }
        self.save_outbound_payments(&records)
    }

    async fn refresh_outbound_melt_quote_status(&self, quote_id: &str) -> Result<(), String> {
        match self.wallet.check_melt_quote_status(quote_id).await {
            Ok(quote) => {
                let state = format!("{:?}", quote.state);
                self.append_outbound_payment_diagnostic(
                    quote_id,
                    "mint_status_checked",
                    format!("mint status check returned {state}"),
                    Some(state.clone()),
                    None,
                )?;
                match quote.state {
                    MeltQuoteState::Paid | MeltQuoteState::Pending | MeltQuoteState::Failed => self
                        .update_outbound_payment_status(
                            quote_id,
                            cashu_payment_status_for_melt_state(quote.state),
                            &cashu_melt_status_message(quote.state),
                            Some(quote.amount.to_u64()),
                            Some(quote.fee_reserve.to_u64()),
                        ),
                    MeltQuoteState::Unpaid | MeltQuoteState::Unknown => Ok(()),
                }
            }
            Err(error) => self.append_outbound_payment_diagnostic(
                quote_id,
                "mint_status_error",
                format!("mint status check failed: {error}"),
                None,
                None,
            ),
        }
    }

    async fn record_untracked_pending_melts(&self) -> Result<(), String> {
        let pending_melts = self
            .wallet
            .get_pending_melt_quotes()
            .await
            .map_err(|error| format!("load pending Cashu BOLT12 melt quotes: {error}"))?;
        if pending_melts.is_empty() {
            return Ok(());
        }

        let mut records = self.load_outbound_payments()?;
        let mut changed = false;
        for quote in pending_melts {
            if quote.payment_method != PaymentMethod::BOLT12 {
                continue;
            }
            if records.iter().any(|record| record.quote_id == quote.id) {
                continue;
            }
            let now = unix_time_ms();
            records.push(CashuOutboundPaymentRecord {
                quote_id: quote.id,
                offer: quote.request,
                mint_url: self.normalized_mint_url(),
                rail: cashu_payment_rail("outgoing", Some(&quote.payment_method)),
                payment_method: quote.payment_method.to_string(),
                amount_sats: quote.amount.to_u64(),
                fees_sats: quote.fee_reserve.to_u64(),
                fee_reserve_sats: Some(quote.fee_reserve.to_u64()),
                quote_state: Some(format!("{:?}", quote.state)),
                quote_expiry: Some(quote.expiry),
                status: CASHU_PAYMENT_STATUS_PENDING.to_string(),
                status_message: CASHU_PAYMENT_PENDING_MESSAGE.to_string(),
                created_at_ms: now,
                updated_at_ms: now,
                confirm_started_at_ms: None,
                confirm_completed_at_ms: None,
                last_mint_check_at_ms: None,
                last_mint_state: None,
                diagnostics: vec![cashu_diagnostic_event(
                    now,
                    "recovered_local_pending_quote",
                    "found pending melt quote in local Cashu wallet store".to_string(),
                )],
            });
            changed = true;
        }

        if changed {
            self.save_outbound_payments(&records)?;
        }
        Ok(())
    }

    fn normalized_mint_url(&self) -> String {
        self.mint_url.trim_end_matches('/').to_string()
    }
}

fn cashu_outbound_record_transaction(record: CashuOutboundPaymentRecord) -> WalletTransaction {
    let fees_sats = cashu_record_display_fee_sats(&record);
    WalletTransaction {
        id: record.quote_id,
        rail: record.rail,
        kind: "offer".to_string(),
        direction: "outgoing".to_string(),
        status: record.status,
        status_message: record.status_message,
        amount_sats: Some(record.amount_sats),
        fees_sats,
        message: None,
        personal_note: None,
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
        agent_payment: None,
    }
}

fn cashu_outbound_diagnostic_payment(
    record: CashuOutboundPaymentRecord,
) -> CashuOutboundDiagnosticPayment {
    let fee_reserve_sats = cashu_record_fee_reserve_sats(&record);
    let fees_sats = cashu_record_display_fee_sats(&record);
    CashuOutboundDiagnosticPayment {
        quote_id: record.quote_id,
        mint_url: record.mint_url,
        rail: record.rail,
        payment_method: record.payment_method,
        offer_preview: cashu_offer_preview(&record.offer),
        offer_sha256: sha256_hex(record.offer.trim()),
        amount_sats: record.amount_sats,
        fees_sats,
        fee_reserve_sats,
        quote_state: record.quote_state,
        quote_expiry: record.quote_expiry,
        status: record.status,
        status_message: record.status_message,
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
        confirm_started_at_ms: record.confirm_started_at_ms,
        confirm_completed_at_ms: record.confirm_completed_at_ms,
        confirm_elapsed_ms: match (record.confirm_started_at_ms, record.confirm_completed_at_ms) {
            (Some(start), Some(end)) => Some(end.saturating_sub(start)),
            _ => None,
        },
        last_mint_check_at_ms: record.last_mint_check_at_ms,
        last_mint_state: record.last_mint_state,
        diagnostics: record.diagnostics,
    }
}

fn cashu_record_display_fee_sats(record: &CashuOutboundPaymentRecord) -> u64 {
    cashu_record_fee_reserve_sats(record)
        .map(|fee_reserve_sats| cashu_display_fee_sats(record.fees_sats, fee_reserve_sats))
        .unwrap_or(record.fees_sats)
}

fn cashu_record_fee_reserve_sats(record: &CashuOutboundPaymentRecord) -> Option<u64> {
    record
        .fee_reserve_sats
        .or_else(|| cashu_fee_reserve_from_diagnostics(&record.diagnostics))
}

fn cashu_fee_reserve_from_diagnostics(diagnostics: &[CashuOutboundDiagnosticEvent]) -> Option<u64> {
    diagnostics.iter().rev().find_map(|event| {
        if event.stage == "melt_quote_created" {
            parse_cashu_fee_reserve_sats(&event.detail)
        } else {
            None
        }
    })
}

fn parse_cashu_fee_reserve_sats(detail: &str) -> Option<u64> {
    let marker = "fee reserve ";
    let start = detail.find(marker)? + marker.len();
    let digits = detail[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

struct CashuSendTarget<'a> {
    destination: &'a str,
    payment_method: PaymentMethod,
    rail: &'static str,
    label: &'static str,
}

fn cashu_send_target(destination: &str, amount_sats: u64) -> Result<CashuSendTarget<'_>, String> {
    let destination = destination.trim();
    if destination.is_empty() {
        return Err("Cashu payment target is empty".to_string());
    }
    let lower = destination.to_ascii_lowercase();
    if lower.starts_with("lnbc") || lower.starts_with("lntb") || lower.starts_with("lnbcrt") {
        return Ok(CashuSendTarget {
            destination,
            payment_method: PaymentMethod::BOLT11,
            rail: CASHU_BOLT11_SEND_RAIL,
            label: "BOLT11",
        });
    }
    if lower.starts_with("lno1") {
        return Ok(CashuSendTarget {
            destination,
            payment_method: PaymentMethod::BOLT12,
            rail: CASHU_BOLT12_SEND_RAIL,
            label: "BOLT12",
        });
    }
    Err(format!(
        "Cashu can only pay BOLT11 invoices or BOLT12 offers; got target for {}",
        format_cashu_target_preview(destination, amount_sats)
    ))
}

fn format_cashu_target_preview(destination: &str, amount_sats: u64) -> String {
    let mut preview = destination.chars().take(24).collect::<String>();
    if destination.chars().count() > 24 {
        preview.push_str("...");
    }
    format!("{amount_sats} sats to {preview}")
}

fn default_cashu_outbound_rail() -> String {
    CASHU_BOLT12_SEND_RAIL.to_string()
}

fn cashu_payment_status_for_melt_state(state: MeltQuoteState) -> &'static str {
    match state {
        MeltQuoteState::Paid => CASHU_PAYMENT_STATUS_COMPLETED,
        MeltQuoteState::Pending => CASHU_PAYMENT_STATUS_PENDING,
        MeltQuoteState::Unpaid | MeltQuoteState::Failed | MeltQuoteState::Unknown => {
            CASHU_PAYMENT_STATUS_FAILED
        }
    }
}

fn cashu_melt_status_message(state: MeltQuoteState) -> String {
    match state {
        MeltQuoteState::Paid => String::new(),
        MeltQuoteState::Pending => CASHU_PAYMENT_PENDING_MESSAGE.to_string(),
        MeltQuoteState::Unpaid => {
            "Cashu mint did not complete this BOLT12 payment; quote state is UNPAID".to_string()
        }
        MeltQuoteState::Failed => "Cashu mint marked this BOLT12 payment failed".to_string(),
        MeltQuoteState::Unknown => {
            "Cashu mint returned an unknown BOLT12 payment state".to_string()
        }
    }
}

fn cashu_display_fee_sats(finalized_fee_sats: u64, reserved_fee_sats: u64) -> u64 {
    if reserved_fee_sats == 0 {
        finalized_fee_sats
    } else {
        finalized_fee_sats.min(reserved_fee_sats)
    }
}

fn cashu_diagnostic_event(
    at_ms: u64,
    stage: impl Into<String>,
    detail: impl Into<String>,
) -> CashuOutboundDiagnosticEvent {
    CashuOutboundDiagnosticEvent {
        at_ms,
        stage: stage.into(),
        detail: detail.into(),
    }
}

fn cashu_mint_diagnostic_info(info: cdk::nuts::MintInfo) -> CashuMintDiagnosticInfo {
    CashuMintDiagnosticInfo {
        name: info.name,
        version: info.version.map(|version| version.to_string()),
        nut04_bolt12_sat: !info.nuts.nut04.disabled
            && info
                .nuts
                .nut04
                .get_settings(&CurrencyUnit::Sat, &PaymentMethod::BOLT12)
                .is_some(),
        nut05_bolt12_sat: !info.nuts.nut05.disabled
            && info
                .nuts
                .nut05
                .get_settings(&CurrencyUnit::Sat, &PaymentMethod::BOLT12)
                .is_some(),
        nut05_bolt11_sat: !info.nuts.nut05.disabled
            && info
                .nuts
                .nut05
                .get_settings(&CurrencyUnit::Sat, &PaymentMethod::BOLT11)
                .is_some(),
    }
}

fn cashu_offer_preview(offer: &str) -> String {
    let offer = offer.trim();
    if offer.chars().count() <= 24 {
        return offer.to_string();
    }
    let prefix = offer.chars().take(12).collect::<String>();
    let suffix = offer
        .chars()
        .rev()
        .take(8)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("{prefix}...{suffix}")
}

fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

fn cashu_payment_rail(direction: &str, method: Option<&PaymentMethod>) -> String {
    match (direction, method) {
        ("incoming", Some(method)) if method == &PaymentMethod::BOLT12 => {
            CASHU_BOLT12_RECEIVE_RAIL.to_string()
        }
        ("outgoing", Some(method)) if method == &PaymentMethod::BOLT12 => {
            CASHU_BOLT12_SEND_RAIL.to_string()
        }
        ("incoming", Some(method)) if method == &PaymentMethod::BOLT11 => {
            "cashu-mint-bolt11".to_string()
        }
        ("outgoing", Some(method)) if method == &PaymentMethod::BOLT11 => {
            "cashu-melt-bolt11".to_string()
        }
        _ => "cashu-token".to_string(),
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn load_or_create_seed(path: &Path) -> Result<[u8; 64], String> {
    match std::fs::read_to_string(path) {
        Ok(value) => seed_from_hex(&value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut seed = [0u8; 64];
            getrandom::getrandom(&mut seed)
                .map_err(|error| format!("generate Cashu seed: {error}"))?;
            write_atomic_secret_text(path, &hex::encode(seed))?;
            Ok(seed)
        }
        Err(error) => Err(format!("read Cashu seed: {error}")),
    }
}

fn seed_from_hex(value: &str) -> Result<[u8; 64], String> {
    let bytes = hex::decode(value.trim()).map_err(|error| format!("parse Cashu seed: {error}"))?;
    if bytes.len() != 64 {
        return Err(format!(
            "Cashu seed must be 64 bytes, got {} bytes",
            bytes.len()
        ));
    }
    let mut seed = [0u8; 64];
    seed.copy_from_slice(&bytes);
    Ok(seed)
}

fn ensure_private_dir(path: &Path, label: &str) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|error| format!("create {label}: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("set {label} permissions: {error}"))?;
    }
    Ok(())
}

fn ensure_cashu_wallet_database_saga_table(db_path: &Path) -> Result<(), String> {
    let database = redb::Database::create(db_path)
        .map_err(|error| format!("open Cashu wallet database for initialization: {error}"))?;
    let write_txn = database
        .begin_write()
        .map_err(|error| format!("begin Cashu wallet database initialization: {error}"))?;
    {
        let _ = write_txn
            .open_table(CASHU_WALLET_SAGAS_TABLE)
            .map_err(|error| format!("initialize Cashu wallet saga table: {error}"))?;
    }
    write_txn
        .commit()
        .map_err(|error| format!("commit Cashu wallet database initialization: {error}"))
}

#[cfg(test)]
mod tests {
    use super::super::types::DEFAULT_CASHU_MINT_URL;
    use super::*;
    use redb::ReadableDatabase;

    #[test]
    fn cashu_seed_requires_64_bytes() {
        let seed = seed_from_hex(&"ab".repeat(64)).unwrap();
        assert_eq!(seed.len(), 64);
        assert!(seed_from_hex("ab").is_err());
    }

    #[test]
    fn cashu_wallet_database_saga_table_initializes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("wallet.redb");
        ensure_cashu_wallet_database_saga_table(&db_path).unwrap();
        ensure_cashu_wallet_database_saga_table(&db_path).unwrap();

        let database = redb::Database::create(&db_path).unwrap();
        let read_txn = database.begin_read().unwrap();
        read_txn.open_table(CASHU_WALLET_SAGAS_TABLE).unwrap();
    }

    #[test]
    fn cashu_rails_are_directional_for_bolt12() {
        assert_eq!(
            cashu_payment_rail("incoming", Some(&PaymentMethod::BOLT12)),
            CASHU_BOLT12_RECEIVE_RAIL
        );
        assert_eq!(
            cashu_payment_rail("outgoing", Some(&PaymentMethod::BOLT12)),
            CASHU_BOLT12_SEND_RAIL
        );
    }

    #[test]
    fn cashu_outbound_records_project_to_wallet_transactions() {
        let transaction = cashu_outbound_record_transaction(CashuOutboundPaymentRecord {
            quote_id: "quote-1".to_string(),
            offer: "lno1example".to_string(),
            mint_url: DEFAULT_CASHU_MINT_URL.trim_end_matches('/').to_string(),
            rail: CASHU_BOLT12_SEND_RAIL.to_string(),
            payment_method: "bolt12".to_string(),
            amount_sats: 44,
            fees_sats: 2,
            fee_reserve_sats: Some(3),
            quote_state: Some("Pending".to_string()),
            quote_expiry: Some(1_234),
            status: CASHU_PAYMENT_STATUS_PENDING.to_string(),
            status_message: "Payment is pending at the Cashu mint".to_string(),
            created_at_ms: 1_000,
            updated_at_ms: 2_000,
            confirm_started_at_ms: Some(1_100),
            confirm_completed_at_ms: None,
            last_mint_check_at_ms: None,
            last_mint_state: None,
            diagnostics: vec![cashu_diagnostic_event(
                1_100,
                "confirm_started",
                "submitted melt with Prefer: respond-async",
            )],
        });

        assert_eq!(transaction.id, "quote-1");
        assert_eq!(transaction.rail, CASHU_BOLT12_SEND_RAIL);
        assert_eq!(transaction.kind, "offer");
        assert_eq!(transaction.direction, "outgoing");
        assert_eq!(transaction.status, CASHU_PAYMENT_STATUS_PENDING);
        assert_eq!(transaction.amount_sats, Some(44));
        assert_eq!(transaction.fees_sats, 2);
        assert_eq!(transaction.personal_note, None);
        assert_eq!(transaction.created_at_ms, 1_000);
        assert_eq!(transaction.updated_at_ms, 2_000);
    }

    #[test]
    fn cashu_unpaid_melt_status_message_is_actionable() {
        assert_eq!(
            cashu_melt_status_message(MeltQuoteState::Unpaid),
            "Cashu mint did not complete this BOLT12 payment; quote state is UNPAID"
        );
    }

    #[test]
    fn cashu_display_fee_is_capped_by_quote_reserve() {
        assert_eq!(cashu_display_fee_sats(16, 3), 3);
        assert_eq!(cashu_display_fee_sats(2, 3), 2);
        assert_eq!(cashu_display_fee_sats(4, 0), 4);
    }

    #[test]
    fn cashu_record_fee_reserve_can_migrate_from_diagnostics() {
        let record = CashuOutboundPaymentRecord {
            quote_id: "quote-1".to_string(),
            offer: "lno1example".to_string(),
            mint_url: DEFAULT_CASHU_MINT_URL.trim_end_matches('/').to_string(),
            rail: CASHU_BOLT12_SEND_RAIL.to_string(),
            payment_method: "bolt12".to_string(),
            amount_sats: 10,
            fees_sats: 16,
            fee_reserve_sats: None,
            quote_state: Some("Paid".to_string()),
            quote_expiry: Some(1_234),
            status: CASHU_PAYMENT_STATUS_COMPLETED.to_string(),
            status_message: String::new(),
            created_at_ms: 1_000,
            updated_at_ms: 2_000,
            confirm_started_at_ms: Some(1_100),
            confirm_completed_at_ms: Some(1_200),
            last_mint_check_at_ms: None,
            last_mint_state: None,
            diagnostics: vec![cashu_diagnostic_event(
                1_000,
                "melt_quote_created",
                "created BOLT12 melt quote with state Unpaid, amount 10 sats, fee reserve 3 sats",
            )],
        };

        assert_eq!(cashu_record_display_fee_sats(&record), 3);
    }
}
