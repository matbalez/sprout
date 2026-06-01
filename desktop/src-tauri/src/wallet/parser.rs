use super::types::WalletCommand;

pub(crate) fn parse_wallet_command(content: &str) -> Result<WalletCommand, String> {
    let tokens = content.split_whitespace().collect::<Vec<_>>();
    let mut index = 0usize;

    if tokens.first().is_some_and(|token| token.starts_with('@')) {
        let mention_index = tokens
            .iter()
            .position(|token| is_walletbot_mention_token(token))
            .ok_or_else(|| "message starts with @, but does not address WalletBot".to_string())?;
        index = mention_index + 1;
    }

    let command = tokens
        .get(index)
        .ok_or_else(|| "empty WalletBot command".to_string())?
        .to_ascii_lowercase();
    let rest = &tokens[index + 1..];

    match command.as_str() {
        "help" | "commands" => Ok(WalletCommand::Help),
        "get" => parse_get_command(rest),
        "fund" => parse_fund_command(rest),
        "create" => parse_create_command(rest),
        "send" => parse_send_command(rest),
        _ => Err(format!("unknown WalletBot command: {command}")),
    }
}

fn parse_get_command(rest: &[&str]) -> Result<WalletCommand, String> {
    if rest.len() != 1 {
        return Err("get command must be one of: balance, BOLT12, transactions".to_string());
    }
    match rest[0].to_ascii_lowercase().as_str() {
        "balance" => Ok(WalletCommand::GetBalance),
        "bolt12" => Ok(WalletCommand::GetBolt12),
        "transactions" => Ok(WalletCommand::GetTransactions),
        other => Err(format!("unknown get target: {other}")),
    }
}

fn parse_fund_command(rest: &[&str]) -> Result<WalletCommand, String> {
    match rest {
        ["wallet"] => Ok(WalletCommand::FundWallet),
        _ => Err("fund command must be: fund wallet".to_string()),
    }
}

fn parse_create_command(rest: &[&str]) -> Result<WalletCommand, String> {
    match rest {
        ["invoice", "for", amount] => Ok(WalletCommand::CreateInvoice {
            amount: parse_amount_token(amount)?,
        }),
        _ => Err("create command must be: create invoice for ₿1,000".to_string()),
    }
}

fn parse_send_command(rest: &[&str]) -> Result<WalletCommand, String> {
    match rest {
        [amount, "to", payable] => Ok(WalletCommand::Send {
            amount: parse_amount_token(amount)?,
            payable: payable.trim().to_string(),
        }),
        _ => Err("send command must be: send ₿500 to <payment target>".to_string()),
    }
}

fn parse_amount_token(token: &str) -> Result<u64, String> {
    let raw = token
        .strip_prefix('₿')
        .ok_or_else(|| "amount must start with ₿".to_string())?;
    let compact = raw.replace(',', "");
    if compact.is_empty() {
        return Err("amount is empty".to_string());
    }
    if compact.len() > 1 && compact.starts_with('0') {
        return Err("amount must not have leading zeros".to_string());
    }
    if !compact.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("amount must be an integer number of sats".to_string());
    }
    compact
        .parse::<u64>()
        .map_err(|error| format!("amount is out of range: {error}"))
}

fn is_walletbot_mention_token(token: &str) -> bool {
    let normalized = token
        .trim_start_matches('@')
        .trim_end_matches([',', '.', '!', '?', ':', ';'])
        .to_ascii_lowercase();
    normalized == "walletbot"
        || normalized == "wallet-bot"
        || (normalized.starts_with("walletbot[") && normalized.ends_with(']'))
}

pub(crate) fn username_target_from_payable(payable: &str) -> Option<String> {
    let trimmed = payable.trim();
    if !trimmed.starts_with('@') {
        return None;
    }
    normalize_username(trimmed)
}

pub(crate) fn normalize_username(value: &str) -> Option<String> {
    let username = value
        .trim()
        .trim_start_matches('@')
        .trim_end_matches([',', '.', '!', '?', ':', ';']);
    if username.is_empty()
        || username.len() > 80
        || username
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return None;
    }
    Some(username.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_accepts_wallet_commands() {
        assert!(matches!(
            parse_wallet_command("@WalletBot get balance").unwrap(),
            WalletCommand::GetBalance
        ));
        assert!(matches!(
            parse_wallet_command("help").unwrap(),
            WalletCommand::Help
        ));
        assert!(matches!(
            parse_wallet_command("@Mat's WalletBot get balance").unwrap(),
            WalletCommand::GetBalance
        ));
        assert!(matches!(
            parse_wallet_command("@WalletBot[Mat] get BOLT12").unwrap(),
            WalletCommand::GetBolt12
        ));
        assert!(matches!(
            parse_wallet_command("fund wallet").unwrap(),
            WalletCommand::FundWallet
        ));
        assert!(matches!(
            parse_wallet_command("get transactions").unwrap(),
            WalletCommand::GetTransactions
        ));
        assert!(matches!(
            parse_wallet_command("create invoice for ₿1,000").unwrap(),
            WalletCommand::CreateInvoice { amount: 1000 }
        ));
        assert!(matches!(
            parse_wallet_command("send ₿500 to @baxen").unwrap(),
            WalletCommand::Send { amount: 500, .. }
        ));
    }

    #[test]
    fn parser_rejects_loose_amounts() {
        assert!(parse_wallet_command("@WalletBot send 500 to lno1abc").is_err());
        assert!(parse_wallet_command("@WalletBot create invoice for 500").is_err());
        assert!(parse_wallet_command("@WalletBot send ₿001 to lno1abc").is_err());
    }

    #[test]
    fn parser_rejects_non_walletbot_mentions() {
        assert!(parse_wallet_command("hey @WalletBot get balance").is_err());
        assert!(parse_wallet_command("@Mat get balance").is_err());
        assert!(parse_wallet_command("@WalletBot balance").is_err());
    }

    #[test]
    fn username_target_normalizes_payable() {
        assert_eq!(
            username_target_from_payable("@Baxen,"),
            Some("baxen".to_string())
        );
        assert_eq!(username_target_from_payable("baxen"), None);
        assert_eq!(username_target_from_payable("@bad name"), None);
    }
}
