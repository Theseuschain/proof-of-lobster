//! Local wallet management for Proof of Lobster.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sp_core::crypto::Ss58Codec;
use std::path::PathBuf;

/// Wallet configuration stored locally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConfig {
    /// 12-word mnemonic phrase
    pub mnemonic: String,

    /// Public key in SS58 format
    pub public_key: String,
}

impl WalletConfig {
    /// Get the wallet file path.
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("proof-of-lobster")
            .join("wallet.json")
    }

    /// Generate a new wallet.
    pub fn generate() -> Result<Self> {
        // Generate 16 bytes of entropy for a 12-word mnemonic
        let mut entropy = [0u8; 16];
        getrandom::getrandom(&mut entropy)?;
        let mnemonic = bip39::Mnemonic::from_entropy(&entropy)?;
        let mnemonic_str = mnemonic.to_string();

        // Derive keypair from mnemonic
        let keypair = subxt_signer::sr25519::Keypair::from_phrase(&mnemonic, None)
            .map_err(|e| anyhow::anyhow!("Failed to create keypair: {:?}", e))?;

        // Get public key as SS58
        let public_bytes = keypair.public_key().0;
        let public = sp_core::sr25519::Public::from_raw(public_bytes);
        let public_key = public.to_ss58check();

        Ok(Self {
            mnemonic: mnemonic_str,
            public_key,
        })
    }

    /// Load wallet from disk.
    pub fn load() -> Result<Option<Self>> {
        let path = Self::path();
        if path.exists() {
            let contents = std::fs::read_to_string(&path)?;
            Ok(Some(serde_json::from_str(&contents)?))
        } else {
            Ok(None)
        }
    }

    /// Save wallet to disk.
    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Load or generate a wallet.
    pub fn load_or_generate() -> Result<Self> {
        if let Some(wallet) = Self::load()? {
            Ok(wallet)
        } else {
            let wallet = Self::generate()?;
            wallet.save()?;
            Ok(wallet)
        }
    }

    /// Get the keypair for signing.
    pub fn keypair(&self) -> Result<subxt_signer::sr25519::Keypair> {
        let mnemonic = bip39::Mnemonic::parse(&self.mnemonic)?;
        subxt_signer::sr25519::Keypair::from_phrase(&mnemonic, None)
            .map_err(|e| anyhow::anyhow!("Failed to create keypair: {:?}", e))
    }

    /// Get short display version of public key.
    pub fn short_address(&self) -> String {
        let pk = &self.public_key;
        if pk.len() > 16 {
            format!("{}...{}", &pk[..8], &pk[pk.len() - 6..])
        } else {
            pk.clone()
        }
    }
}
