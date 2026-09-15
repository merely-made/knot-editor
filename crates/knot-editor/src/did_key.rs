// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! `did:key` naming for ed25519 verifying keys.
//!
//! A `did:key` IRI is self-certifying: it carries the public key itself, so an
//! author or a minted predicate namespace resolves with no network and no Knot
//! registry. Hand-rolled because the dependency graph carries no base58 or
//! multibase crate and this is a fifty-line encoding.

/// Multicodec prefix for an ed25519 public key, varint-encoded.
const ED25519_PUB_MULTICODEC: [u8; 2] = [0xed, 0x01];
const BASE58_ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Bitcoin-alphabet base58, one leading `1` per leading zero byte.
fn base58btc(bytes: &[u8]) -> String {
    let leading_zeros = bytes.iter().take_while(|byte| **byte == 0).count();
    // Big-endian base-256 to base-58, one digit at a time.
    let mut digits: Vec<u8> = Vec::with_capacity(bytes.len() * 138 / 100 + 1);
    for &byte in &bytes[leading_zeros..] {
        let mut carry = u32::from(byte);
        for digit in digits.iter_mut() {
            carry += u32::from(*digit) << 8;
            *digit = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let mut encoded = String::with_capacity(leading_zeros + digits.len());
    encoded.extend(std::iter::repeat_n('1', leading_zeros));
    for digit in digits.iter().rev() {
        encoded.push(BASE58_ALPHABET[usize::from(*digit)] as char);
    }
    encoded
}

/// Inverse of [`base58btc`]. `None` on any character outside the alphabet.
fn base58btc_decode(encoded: &str) -> Option<Vec<u8>> {
    let leading_ones = encoded
        .chars()
        .take_while(|character| *character == '1')
        .count();
    let mut bytes: Vec<u8> = Vec::with_capacity(encoded.len());
    for character in encoded.chars().skip(leading_ones) {
        let value = BASE58_ALPHABET
            .iter()
            .position(|candidate| char::from(*candidate) == character)?;
        let mut carry = value as u32;
        for byte in bytes.iter_mut() {
            carry += u32::from(*byte) * 58;
            *byte = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            bytes.push((carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    let mut decoded = vec![0u8; leading_ones];
    decoded.extend(bytes.iter().rev());
    Some(decoded)
}

/// Name one ed25519 verifying key as a `did:key` IRI.
pub fn did_key(verifying_key: &[u8; 32]) -> String {
    let mut multicodec = Vec::with_capacity(34);
    multicodec.extend_from_slice(&ED25519_PUB_MULTICODEC);
    multicodec.extend_from_slice(verifying_key);
    format!("did:key:z{}", base58btc(&multicodec))
}

/// Recover the ed25519 verifying key named by a `did:key` IRI.
///
/// Any other multibase or multicodec prefix is refused rather than reinterpreted.
pub fn did_key_verifying_key(did: &str) -> Option<[u8; 32]> {
    let multibase = did.strip_prefix("did:key:")?;
    let base58 = multibase.strip_prefix('z')?;
    let decoded = base58btc_decode(base58)?;
    let (prefix, key) = decoded.split_at_checked(2)?;
    if prefix != ED25519_PUB_MULTICODEC {
        return None;
    }
    key.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::{base58btc, base58btc_decode, did_key, did_key_verifying_key};

    fn from_hex(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn base58btc_matches_the_published_bitcoin_vectors() {
        for (hex, expected) in [
            ("", ""),
            ("61", "2g"),
            ("626262", "a3gV"),
            ("636363", "aPEr"),
            ("516b6fcd0f", "ABnLTmg"),
            ("bf4f89001e670274dd", "3SEo3LWLoPntC"),
            ("572e4794", "3EFU7m"),
            ("ecac89cad93923c02321", "EJDM8drfXA6uyA"),
            ("10c8511e", "Rt5zm"),
            ("00000000000000000000", "1111111111"),
            (
                "73696d706c792061206c6f6e6720737472696e67",
                "2cFupjhnEsSn59qHXstmK2ffpLv2",
            ),
        ] {
            let bytes = from_hex(hex);
            assert_eq!(base58btc(&bytes), expected, "encoding {hex}");
            assert_eq!(base58btc_decode(expected).unwrap(), bytes, "decoding {hex}");
        }
    }

    #[test]
    fn did_key_carries_the_ed25519_multicodec_and_round_trips() {
        // The 0xed 0x01 multicodec plus base58btc fixes the visible prefix of
        // every ed25519 did:key, and the whole string is 48 characters.
        for seed in [[0u8; 32], [0xff; 32], [0x5a; 32]] {
            let did = did_key(&seed);
            assert!(did.starts_with("did:key:z6Mk"), "{did}");
            assert_eq!(did.len(), 56, "{did}");
            assert_eq!(did_key_verifying_key(&did), Some(seed));
        }
    }

    #[test]
    fn foreign_prefixes_are_refused_rather_than_reinterpreted() {
        // secp256k1 (0xe7 0x01) carries a valid multibase and a wrong multicodec.
        let mut secp = vec![0xe7, 0x01];
        secp.extend_from_slice(&[0x11; 32]);
        let foreign = format!("did:key:z{}", base58btc(&secp));
        assert_eq!(did_key_verifying_key(&foreign), None);

        assert_eq!(did_key_verifying_key("did:web:example.test"), None);
        assert_eq!(
            did_key_verifying_key("did:key:f6Mkexample"),
            None,
            "base16 multibase is not decoded as base58"
        );
        assert_eq!(did_key_verifying_key("did:key:z0OIl"), None);
        let short = format!("did:key:z{}", base58btc(&[0xed, 0x01, 0x07]));
        assert_eq!(did_key_verifying_key(&short), None);
    }
}
