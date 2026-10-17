// SPDX-License-Identifier: AGPL-3.0-or-later
//! Domain-separated gateway attestations, independent of repository history.
use crate::{Result, VerifyError, validate_signing_key};
use atrium_crypto::keypair::{Did as _, Secp256k1Keypair};
const DOMAIN: &[u8] = b"codifix-private-gateway-attestation-v1\0";
const MAX_MESSAGE: usize = 4 * 1024 * 1024;

// Intentionally no Debug/Serialize/Clone exposing the private scalar.
pub struct DetachedSigner(Secp256k1Keypair);
impl DetachedSigner {
    pub fn from_secret(secret: &[u8]) -> Result<Self> {
        Ok(Self(
            Secp256k1Keypair::import(secret).map_err(|_| VerifyError::Enrollment)?,
        ))
    }
    pub fn public_key(&self) -> String {
        self.0.did()
    }
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>> {
        self.0
            .sign(&domain_message(message)?)
            .map_err(|_| VerifyError::Signature)
    }
}
#[derive(Clone)]
pub struct DetachedVerifier(String);
impl DetachedVerifier {
    pub fn new(key: String) -> Result<Self> {
        validate_signing_key(&key)?;
        Ok(Self(key))
    }
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> Result<()> {
        if signature.len() != 64 {
            return Err(VerifyError::Signature);
        }
        atrium_crypto::verify::verify_signature(&self.0, &domain_message(message)?, signature)
            .map_err(|_| VerifyError::Signature)
    }
}
fn domain_message(message: &[u8]) -> Result<Vec<u8>> {
    if message.len() > MAX_MESSAGE {
        return Err(VerifyError::Limit);
    }
    let mut bytes = DOMAIN.to_vec();
    bytes.extend_from_slice(message);
    Ok(bytes)
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn detached_signatures_are_domain_separated_and_bounded() {
        let signer = DetachedSigner::from_secret(&[3; 32]).unwrap();
        let verifier = DetachedVerifier::new(signer.public_key()).unwrap();
        let signature = signer.sign(b"receipt").unwrap();
        verifier.verify(b"receipt", &signature).unwrap();
        assert!(verifier.verify(b"different", &signature).is_err());
        assert!(
            atrium_crypto::verify::verify_signature(&signer.public_key(), b"receipt", &signature)
                .is_err()
        );
        assert!(signer.sign(&vec![0; MAX_MESSAGE + 1]).is_err());
        assert!(DetachedSigner::from_secret(&[0; 32]).is_err());
    }
}
