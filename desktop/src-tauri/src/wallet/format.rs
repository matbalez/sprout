use lexe::types::{bitcoin::Amount, payment::Payment};

use super::types::{WalletAgentPaymentAnnotation, WalletTransaction};

pub(crate) fn amount_from_sats(amount_sats: u64) -> Result<Amount, String> {
    Amount::try_from_sats_u64(amount_sats)
        .map_err(|error| format!("invalid amount {}: {error}", format_amount(amount_sats)))
}

pub(crate) fn format_bolt12_offer_message(prefix: &str, offer: &str) -> String {
    format!("{prefix}\n{offer}")
}

pub(crate) fn format_amount(sats: u64) -> String {
    format!("₿{}", format_grouped_u64(sats))
}

fn format_grouped_u64(value: u64) -> String {
    let text = value.to_string();
    let mut output = String::with_capacity(text.len() + text.len() / 3);
    for (index, ch) in text.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            output.push(',');
        }
        output.push(ch);
    }
    output.chars().rev().collect()
}

pub(crate) fn wallet_transaction_with_annotation(
    payment: &Payment,
    agent_payment: Option<WalletAgentPaymentAnnotation>,
) -> WalletTransaction {
    WalletTransaction {
        id: payment.index.to_string(),
        rail: payment.rail.to_string(),
        kind: payment.kind.to_string(),
        direction: payment.direction.to_string(),
        status: payment.status.to_string(),
        status_message: payment.status_msg.clone(),
        amount_sats: payment.amount.map(|amount| amount.sats_u64()),
        fees_sats: payment.fees.sats_u64(),
        message: payment.message.clone(),
        personal_note: payment.personal_note.clone(),
        created_at_ms: payment.created_at.to_millis(),
        updated_at_ms: payment.updated_at.to_millis(),
        agent_payment,
    }
}

pub(crate) fn format_wallet_transaction(transaction: &WalletTransaction) -> String {
    let amount = transaction
        .amount_sats
        .map(format_amount)
        .unwrap_or_else(|| "amountless".to_string());
    let mut parts = vec![
        format_timestamp_ms(transaction.created_at_ms),
        transaction.direction.clone(),
        transaction.status.clone(),
        transaction.kind.clone(),
        amount,
    ];
    if transaction.fees_sats > 0 {
        parts.push(format!("fee {}", format_amount(transaction.fees_sats)));
    }
    if !transaction.status_message.trim().is_empty() {
        parts.push(transaction.status_message.trim().to_string());
    }
    parts.join(" - ")
}

fn format_timestamp_ms(ms_since_epoch: u64) -> String {
    match chrono::DateTime::from_timestamp((ms_since_epoch / 1000) as i64, 0) {
        Some(timestamp) => timestamp.to_rfc3339(),
        None => format!("{ms_since_epoch}ms since Unix epoch"),
    }
}

#[cfg(test)]
mod tests {
    use super::format_amount;

    #[test]
    fn formats_amounts_with_bitcoin_symbol() {
        assert_eq!(format_amount(0), "₿0");
        assert_eq!(format_amount(500), "₿500");
        assert_eq!(format_amount(12_345_678), "₿12,345,678");
    }
}
