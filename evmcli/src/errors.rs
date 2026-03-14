use thiserror::Error;

#[derive(Error, Debug)]
pub enum EvmError {
    #[error("RPC error: {message}")]
    Rpc { code: &'static str, message: String },

    #[error("Explorer API error: {message}")]
    Explorer { code: &'static str, message: String },

    #[error("ABI error: {message}")]
    Abi { code: &'static str, message: String },

    #[error("Decode error: {message}")]
    Decode { code: &'static str, message: String },

    #[error("Signing error: {message}")]
    Signing { code: &'static str, message: String },

    #[error("Validation error: {message}")]
    Validation { code: &'static str, message: String },

    #[error("Config error: {message}")]
    Config { code: &'static str, message: String },
}

impl EvmError {
    pub fn machine_code(&self) -> &'static str {
        match self {
            Self::Rpc { code, .. } => code,
            Self::Explorer { code, .. } => code,
            Self::Abi { code, .. } => code,
            Self::Decode { code, .. } => code,
            Self::Signing { code, .. } => code,
            Self::Validation { code, .. } => code,
            Self::Config { code, .. } => code,
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Config { .. } => 2,
            Self::Rpc { .. } => 3,
            Self::Signing { .. } => 5,
            _ => 1,
        }
    }

    pub fn rpc(msg: impl Into<String>) -> Self {
        Self::Rpc { code: "rpc.error", message: msg.into() }
    }

    pub fn rpc_timeout(detail: impl Into<String>) -> Self {
        Self::Rpc { code: "rpc.timeout", message: detail.into() }
    }

    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config { code: "config.error", message: msg.into() }
    }

    pub fn explorer(msg: impl Into<String>) -> Self {
        Self::Explorer { code: "explorer.error", message: msg.into() }
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation { code: "validation.error", message: msg.into() }
    }

    pub fn decode(msg: impl Into<String>) -> Self {
        Self::Decode { code: "decode.error", message: msg.into() }
    }
}

impl From<alloy::transports::TransportError> for EvmError {
    fn from(e: alloy::transports::TransportError) -> Self {
        Self::Rpc { code: "rpc.transport", message: e.to_string() }
    }
}

impl From<alloy::contract::Error> for EvmError {
    fn from(e: alloy::contract::Error) -> Self {
        Self::Rpc { code: "rpc.contract", message: e.to_string() }
    }
}

/// Validate an Ethereum address (0x + 40 hex chars)
pub fn validate_address(addr: &str) -> Result<(), EvmError> {
    let clean = addr.strip_prefix("0x").unwrap_or(addr);
    if clean.len() != 40 || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(EvmError::validation(format!("Invalid address: {addr}. Expected 0x + 40 hex chars.")));
    }
    Ok(())
}
