use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, Error)]
pub enum YoinkError {
    #[error("Resource not found: {resource}")]
    NotFound {
        resource: String,
        id: Option<String>,
    },
    #[error("Validation failed: {reason}")]
    Validation {
        field: Option<String>,
        reason: String,
    },
    #[error("Conflict: {reason}")]
    Conflict { reason: String },
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Service unavailable: {service}")]
    Unavailable { service: String, reason: String },
    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl YoinkError {
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
}

impl From<String> for YoinkError {
    fn from(value: String) -> Self {
        Self::internal(value)
    }
}

impl From<&str> for YoinkError {
    fn from(value: &str) -> Self {
        Self::internal(value)
    }
}

#[derive(Debug, Clone, Error)]
#[error("Invalid quality string: {value}")]
pub struct ParseQualityError {
    pub value: String,
}
