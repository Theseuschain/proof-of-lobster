//! Extrinsic building and signing.
//!
//! This module builds and signs extrinsics locally using the user's wallet.
//! The server provides the call data and chain metadata, and we construct
//! the full signed extrinsic here.
//!
//! The chain uses TxExtension with these extensions (in order):
//! 1. CheckNonZeroSender - empty explicit, empty implicit
//! 2. CheckSpecVersion - empty explicit, u32 implicit
//! 3. CheckTxVersion - empty explicit, u32 implicit
//! 4. CheckGenesis - empty explicit, Hash implicit
//! 5. CheckEra - Era explicit, Hash implicit
//! 6. CheckNonce - Compact<Nonce> explicit, empty implicit
//! 7. CheckWeight - empty explicit, empty implicit
//! 8. ChargeTransactionPayment - Compact<Tip> explicit, empty implicit
//! 9. CheckMetadataHash - u8 mode explicit, Option<Hash> implicit
//! 10. WeightReclaim - empty explicit, empty implicit

use anyhow::Result;
use codec::{Compact, Encode};

/// Build and sign an extrinsic for submission.
///
/// # Arguments
/// * `call_data` - The encoded call data from the server
/// * `nonce` - The account nonce
/// * `genesis_hash` - The chain's genesis hash (32 bytes)
/// * `spec_version` - The runtime spec version
/// * `transaction_version` - The runtime transaction version
/// * `keypair` - The signing keypair
///
/// # Returns
/// The fully signed extrinsic as hex-encoded bytes (with 0x prefix)
pub fn build_signed_extrinsic(
    call_data: &[u8],
    nonce: u64,
    genesis_hash: &[u8; 32],
    spec_version: u32,
    transaction_version: u32,
    keypair: &subxt_signer::sr25519::Keypair,
) -> Result<String> {
    // Build the signing payload
    // This is what gets signed: call + explicit extensions + implicit extensions
    let mut payload = Vec::new();

    // 1. Call data
    payload.extend_from_slice(call_data);

    // 2. Explicit extensions (in TxExtension order):
    // CheckNonZeroSender: () - nothing
    // CheckSpecVersion: () - nothing
    // CheckTxVersion: () - nothing
    // CheckGenesis: () - nothing
    // CheckEra: Era (immortal = 0x00)
    payload.push(0x00);
    // CheckNonce: Compact<nonce>
    Compact(nonce).encode_to(&mut payload);
    // CheckWeight: () - nothing
    // ChargeTransactionPayment: Compact<tip>
    Compact(0u128).encode_to(&mut payload);
    // CheckMetadataHash: u8 mode (0 = disabled)
    payload.push(0x00);
    // WeightReclaim: () - nothing

    // 3. Implicit extensions (additional signed data, not in extrinsic):
    // CheckNonZeroSender: () - nothing
    // CheckSpecVersion: u32
    spec_version.encode_to(&mut payload);
    // CheckTxVersion: u32
    transaction_version.encode_to(&mut payload);
    // CheckGenesis: Hash
    payload.extend_from_slice(genesis_hash);
    // CheckEra: Hash (block hash, = genesis for immortal)
    payload.extend_from_slice(genesis_hash);
    // CheckNonce: () - nothing
    // CheckWeight: () - nothing
    // ChargeTransactionPayment: () - nothing
    // CheckMetadataHash: Option<Hash> (None = 0x00 when mode is 0)
    payload.push(0x00);
    // WeightReclaim: () - nothing

    // Sign the payload
    // If payload > 256 bytes, hash it first
    let signature = if payload.len() > 256 {
        use sp_core::hashing::blake2_256;
        let hash = blake2_256(&payload);
        keypair.sign(&hash)
    } else {
        keypair.sign(&payload)
    };

    // Build the final extrinsic
    let mut extrinsic = Vec::new();

    // Version byte: 0x84 = signed (0x80) + version 4 (0x04)
    extrinsic.push(0x84);

    // Signer address (MultiAddress::Id variant = 0x00 + 32 bytes)
    extrinsic.push(0x00);
    extrinsic.extend_from_slice(&keypair.public_key().0);

    // Signature (MultiSignature::Sr25519 variant = 0x01 + 64 bytes)
    extrinsic.push(0x01);
    extrinsic.extend_from_slice(&signature.0);

    // Explicit extensions (same order, without implicit data):
    // CheckNonZeroSender: () - nothing
    // CheckSpecVersion: () - nothing
    // CheckTxVersion: () - nothing
    // CheckGenesis: () - nothing
    // CheckEra: Era
    extrinsic.push(0x00);
    // CheckNonce: Compact<nonce>
    Compact(nonce).encode_to(&mut extrinsic);
    // CheckWeight: () - nothing
    // ChargeTransactionPayment: Compact<tip>
    Compact(0u128).encode_to(&mut extrinsic);
    // CheckMetadataHash: u8 mode (0 = disabled)
    extrinsic.push(0x00);
    // WeightReclaim: () - nothing

    // Call data
    extrinsic.extend_from_slice(call_data);

    // Length-prefix the whole thing
    let mut final_extrinsic = Vec::new();
    Compact(extrinsic.len() as u32).encode_to(&mut final_extrinsic);
    final_extrinsic.extend_from_slice(&extrinsic);

    Ok(format!("0x{}", hex::encode(&final_extrinsic)))
}

/// Parse an AgentRegistered event from the events list.
/// Returns the agent address (SS58 encoded).
pub fn parse_agent_registered_event(events: &[crate::client::ChainEvent]) -> Option<String> {
    for event in events {
        if event.pallet == "Agents" && event.variant == "AgentRegistered" {
            // The event data contains the agent ID as the first 32 bytes
            if let Some(bytes_hex) = event.data.get("bytes").and_then(|v| v.as_str()) {
                let bytes = hex::decode(bytes_hex).ok()?;
                if bytes.len() >= 32 {
                    let account_bytes: [u8; 32] = bytes[0..32].try_into().ok()?;
                    let public = sp_core::sr25519::Public::from_raw(account_bytes);
                    use sp_core::crypto::Ss58Codec;
                    return Some(public.to_ss58check());
                }
            }
        }
    }
    None
}

/// Parse an AgentCallQueued event to get the run_id.
pub fn parse_agent_call_queued_event(events: &[crate::client::ChainEvent]) -> Option<u64> {
    for event in events {
        if event.pallet == "Agents" && event.variant == "AgentCallQueued" {
            if let Some(bytes_hex) = event.data.get("bytes").and_then(|v| v.as_str()) {
                let bytes = hex::decode(bytes_hex).ok()?;
                if bytes.len() >= 8 {
                    let run_id = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
                    return Some(run_id);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_extrinsic_format() {
        // This is just a smoke test to ensure the encoding doesn't panic
        let mnemonic = bip39::Mnemonic::parse(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
        ).unwrap();
        let keypair = subxt_signer::sr25519::Keypair::from_phrase(&mnemonic, None).unwrap();

        let call_data = vec![0x00, 0x01, 0x02]; // Dummy call data
        let genesis_hash = [0u8; 32];

        let result = build_signed_extrinsic(
            &call_data,
            0,
            &genesis_hash,
            1,
            1,
            &keypair,
        );

        assert!(result.is_ok());
        assert!(result.unwrap().starts_with("0x"));
    }
}
