use lexe::{
    types::{
        command::NodeInfo,
        payment::{Order, Payment, PaymentDirection, PaymentFilter, PaymentStatus},
    },
    wallet::LexeWallet,
};

use super::types::MAX_TRANSACTION_LIMIT;

pub(crate) struct WalletBalances {
    pub balance_sats: u64,
    pub lightning_balance_sats: u64,
    pub lightning_sendable_balance_sats: u64,
    pub lightning_max_sendable_balance_sats: u64,
    pub onchain_balance_sats: u64,
    pub onchain_trusted_balance_sats: u64,
}

pub(crate) async fn wallet_balances(wallet: &LexeWallet, info: &NodeInfo) -> WalletBalances {
    let mut balances = WalletBalances {
        balance_sats: info.balance.sats_u64(),
        lightning_balance_sats: info.lightning_balance.sats_u64(),
        lightning_sendable_balance_sats: info.lightning_sendable_balance.sats_u64(),
        lightning_max_sendable_balance_sats: info.lightning_max_sendable_balance.sats_u64(),
        onchain_balance_sats: info.onchain_balance.sats_u64(),
        onchain_trusted_balance_sats: info.onchain_trusted_balance.sats_u64(),
    };

    if balances.balance_sats > 0 {
        return balances;
    }

    let Some(imputed_balance) = imputed_balance_from_history(wallet).await else {
        return balances;
    };

    balances.balance_sats = imputed_balance;
    if balances.lightning_balance_sats == 0 {
        balances.lightning_balance_sats = imputed_balance;
    }
    if balances.lightning_sendable_balance_sats == 0 {
        balances.lightning_sendable_balance_sats = imputed_balance;
    }
    if balances.lightning_max_sendable_balance_sats == 0 {
        balances.lightning_max_sendable_balance_sats = imputed_balance;
    }
    balances
}

async fn imputed_balance_from_history(wallet: &LexeWallet) -> Option<u64> {
    if wallet.sync_payments().await.is_err() {
        return None;
    }

    let response = wallet
        .list_payments(
            &PaymentFilter::All,
            Some(Order::Asc),
            Some(MAX_TRANSACTION_LIMIT),
            None,
        )
        .ok()?;
    impute_balance_from_payments(&response.payments)
}

fn impute_balance_from_payments(payments: &[Payment]) -> Option<u64> {
    impute_balance_from_entries(payments.iter().map(BalanceEntry::from_payment))
}

#[derive(Clone, Copy)]
struct BalanceEntry {
    direction: BalanceDirection,
    is_completed: bool,
    amount_sats: Option<u64>,
    fees_sats: u64,
}

impl BalanceEntry {
    fn from_payment(payment: &Payment) -> Self {
        Self {
            direction: BalanceDirection::from_payment_direction(payment.direction),
            is_completed: payment.status == PaymentStatus::Completed,
            amount_sats: payment.amount.map(|amount| amount.sats_u64()),
            fees_sats: payment.fees.sats_u64(),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum BalanceDirection {
    Inbound,
    Outbound,
    Info,
}

impl BalanceDirection {
    fn from_payment_direction(direction: PaymentDirection) -> Self {
        match direction {
            PaymentDirection::Inbound => Self::Inbound,
            PaymentDirection::Outbound => Self::Outbound,
            PaymentDirection::Info => Self::Info,
        }
    }
}

fn impute_balance_from_entries(entries: impl IntoIterator<Item = BalanceEntry>) -> Option<u64> {
    let mut saw_balance_payment = false;
    let mut net_sats = 0i128;

    for entry in entries {
        if !entry.is_completed {
            continue;
        }
        let Some(amount_sats) = entry.amount_sats else {
            continue;
        };
        match entry.direction {
            BalanceDirection::Inbound => {
                saw_balance_payment = true;
                net_sats += i128::from(amount_sats);
            }
            BalanceDirection::Outbound => {
                saw_balance_payment = true;
                net_sats -= i128::from(amount_sats) + i128::from(entry.fees_sats);
            }
            BalanceDirection::Info => {}
        }
    }

    if saw_balance_payment && net_sats > 0 {
        Some(net_sats as u64)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{impute_balance_from_entries, BalanceDirection, BalanceEntry};

    fn entry(
        direction: BalanceDirection,
        is_completed: bool,
        amount_sats: Option<u64>,
        fees_sats: u64,
    ) -> BalanceEntry {
        BalanceEntry {
            direction,
            is_completed,
            amount_sats,
            fees_sats,
        }
    }

    #[test]
    fn imputes_balance_from_completed_history() {
        let balance = impute_balance_from_entries([
            entry(BalanceDirection::Inbound, true, Some(500), 0),
            entry(BalanceDirection::Outbound, true, Some(125), 2),
            entry(BalanceDirection::Inbound, false, Some(900), 0),
            entry(BalanceDirection::Info, true, Some(900), 0),
        ]);

        assert_eq!(balance, Some(373));
    }

    #[test]
    fn does_not_impute_empty_or_spent_history() {
        assert_eq!(
            impute_balance_from_entries(Vec::<BalanceEntry>::new()),
            None
        );
        assert_eq!(
            impute_balance_from_entries([
                entry(BalanceDirection::Inbound, true, Some(100), 0),
                entry(BalanceDirection::Outbound, true, Some(100), 1),
            ]),
            None
        );
    }
}
