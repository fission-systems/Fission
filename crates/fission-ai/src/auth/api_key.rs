//! API key resolution from environment variables.

use super::{ENV_FISSION_AI_API_KEY, ENV_OPENAI_API_KEY};

/// Read the Fission-specific API key from the environment.
pub fn read_fission_api_key() -> Option<String> {
    std::env::var(ENV_FISSION_AI_API_KEY).ok().filter(|k| !k.is_empty())
}

/// Read the OpenAI API key from the environment.
pub fn read_openai_api_key() -> Option<String> {
    std::env::var(ENV_OPENAI_API_KEY).ok().filter(|k| !k.is_empty())
}
