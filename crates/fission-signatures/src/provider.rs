//! Facade for signature resources under `utils/signatures`.

use crate::api_types::{ApiSignature, ApiTypeDatabase, ApiTypeError};
use serde::Serialize;
use std::sync::LazyLock;

pub static SIGNATURE_RESOURCES: LazyLock<SignatureResourceProvider> =
    LazyLock::new(SignatureResourceProvider::detect);

#[derive(Debug)]
pub struct SignatureResourceProvider {
    api_types: Result<ApiTypeDatabase, ApiTypeError>,
}

impl SignatureResourceProvider {
    pub fn detect() -> Self {
        Self {
            api_types: ApiTypeDatabase::from_utils_signatures(),
        }
    }

    pub fn api_signatures(&self) -> Result<impl Iterator<Item = &ApiSignature>, &ApiTypeError> {
        self.api_types.as_ref().map(ApiTypeDatabase::iter)
    }

    pub fn api_signatures_json(&self) -> Option<String> {
        let signatures: Vec<&ApiSignature> = self.api_signatures().ok()?.collect();
        serde_json::to_string(&signatures).ok()
    }
}

impl Serialize for SignatureResourceProvider {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let api_signature_count = self.api_types.as_ref().map_or(0, ApiTypeDatabase::len);
        serializer.serialize_u64(api_signature_count as u64)
    }
}
