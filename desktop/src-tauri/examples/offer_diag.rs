use std::{path::PathBuf, str::FromStr};

use lexe::{
    config::{Network, WalletEnvConfig},
    types::{
        auth::{ClientCredentials, CredentialsRef, RootSeed},
        bitcoin::Offer,
    },
    wallet::LexeWallet,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        anyhow::bail!("usage: offer_diag <wallet-root> [<wallet-root>...]");
    }

    for root in args.drain(..) {
        let root = PathBuf::from(root);
        let offer_path = root.join("bolt12_offer.txt");
        let offer_text = std::fs::read_to_string(&offer_path)?;
        let offer = Offer::from_str(offer_text.trim())?;
        println!("wallet_root={}", root.display());
        println!("offer_len={}", offer_text.trim().len());
        println!("offer_id={}", offer.id());
        println!("description={:?}", offer.description());
        println!("payee={:?}", offer.payee());
        println!("payee_node_pk={:?}", offer.payee_node_pk());
        println!(
            "supports_mainnet={}",
            offer.supports_network(Network::Mainnet)
        );
        println!(
            "min_amount={:?}",
            offer.min_amount().map(|amount| amount.sats_u64())
        );
        println!("expires_at={:?}", offer.expires_at());
        println!("expects_quantity={}", offer.expects_quantity());

        let seed_path = root.join("seedphrase.txt");
        if seed_path.exists() {
            let Some(root_seed) = RootSeed::read_from_path(&seed_path)? else {
                println!("node_info=missing-seed");
                println!();
                continue;
            };
            let data_dir = root.join("data");
            let wallet = LexeWallet::load_or_fresh(
                WalletEnvConfig::mainnet(),
                CredentialsRef::from(&root_seed),
                Some(data_dir),
            )?;
            let info = wallet.node_info().await?;
            println!("node_user_pk={}", info.user_pk);
            println!("node_pk={}", info.node_pk);
            println!("balance={}", info.balance.sats_u64());
            println!("lightning_balance={}", info.lightning_balance.sats_u64());
            println!(
                "lightning_sendable_balance={}",
                info.lightning_sendable_balance.sats_u64()
            );
            println!(
                "lightning_max_sendable_balance={}",
                info.lightning_max_sendable_balance.sats_u64()
            );
            println!("onchain_balance={}", info.onchain_balance.sats_u64());
            println!(
                "onchain_trusted_balance={}",
                info.onchain_trusted_balance.sats_u64()
            );
            println!("num_channels={}", info.num_channels);
            println!("num_usable_channels={}", info.num_usable_channels);
        } else {
            println!("node_info=not-local-seed-wallet");
        }

        let existing_client_credential_path = root.join("lexe_client_credential.txt");
        if existing_client_credential_path.exists() {
            let client_credential = std::fs::read_to_string(&existing_client_credential_path)?;
            let client_credential = ClientCredentials::from_string(client_credential.trim())?;
            let wallet = LexeWallet::load_or_fresh(
                WalletEnvConfig::mainnet(),
                CredentialsRef::from(&client_credential),
                Some(root.join("existing-data")),
            )?;
            let info = wallet.node_info().await?;
            println!("existing_node_user_pk={}", info.user_pk);
            println!("existing_node_pk={}", info.node_pk);
            println!("existing_balance={}", info.balance.sats_u64());
            println!(
                "existing_lightning_balance={}",
                info.lightning_balance.sats_u64()
            );
            println!(
                "existing_lightning_sendable_balance={}",
                info.lightning_sendable_balance.sats_u64()
            );
            println!(
                "existing_lightning_max_sendable_balance={}",
                info.lightning_max_sendable_balance.sats_u64()
            );
            println!(
                "existing_onchain_balance={}",
                info.onchain_balance.sats_u64()
            );
            println!(
                "existing_onchain_trusted_balance={}",
                info.onchain_trusted_balance.sats_u64()
            );
            println!("existing_num_channels={}", info.num_channels);
            println!("existing_num_usable_channels={}", info.num_usable_channels);
        }
        println!();
    }

    Ok(())
}
