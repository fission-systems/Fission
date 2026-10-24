//! Facade for signature resources resolved via [`fission_core::resources::ResourceProvider`].

use crate::api_types::{ApiSignature, ApiTypeDatabase, ApiTypeError};
use serde::Serialize;
use std::sync::LazyLock;

pub static SIGNATURE_RESOURCES: LazyLock<SignatureResourceProvider> =
    LazyLock::new(SignatureResourceProvider::detect);

#[derive(Debug)]
pub struct SignatureResourceProvider {
    api_types: Result<ApiTypeDatabase, ApiTypeError>,
    api_types_32: Result<ApiTypeDatabase, ApiTypeError>,
}

impl SignatureResourceProvider {
    pub fn detect() -> Self {
        Self {
            api_types: ApiTypeDatabase::from_utils_signatures_for_bitness(true),
            api_types_32: ApiTypeDatabase::from_utils_signatures_for_bitness(false),
        }
    }

    pub fn api_signatures(&self) -> Result<impl Iterator<Item = &ApiSignature>, &ApiTypeError> {
        self.api_types.as_ref().map(ApiTypeDatabase::iter)
    }

    pub fn api_signature(&'static self, name: &str) -> Option<&'static ApiSignature> {
        self.api_signature_for_bitness(name, true)
    }

    pub fn api_signature_for_bitness(
        &'static self,
        name: &str,
        is_64bit: bool,
    ) -> Option<&'static ApiSignature> {
        let db = if is_64bit {
            self.api_types.as_ref().ok()?
        } else {
            self.api_types_32.as_ref().ok()?
        };
        db.get(name)
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
