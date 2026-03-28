use thiserror::Error;

use super::AppError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Error)]
pub(crate) enum ApiError {
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

impl ApiError {
    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
}

impl From<String> for ApiError {
    fn from(value: String) -> Self {
        Self::internal(value)
    }
}

impl From<&str> for ApiError {
    fn from(value: &str) -> Self {
        Self::internal(value)
    }
}

impl From<AppError> for ApiError {
    fn from(value: AppError) -> Self {
        match value {
            AppError::NotFound { resource, id } => ApiError::NotFound { resource, id },
            AppError::Validation { field, reason } => ApiError::Validation { field, reason },
            AppError::Conflict { reason } => ApiError::Conflict { reason },
            AppError::Unavailable { service, reason } => ApiError::Unavailable { service, reason },
            AppError::SeaOrm(err) => ApiError::Internal {
                message: format!("sea-orm error: {err}"),
            },
            AppError::Io(err) => ApiError::Internal {
                message: format!("io error: {err}"),
            },
            AppError::Uuid(err) => ApiError::Validation {
                field: None,
                reason: format!("invalid UUID: {err}"),
            },
            AppError::Provider(err) => ApiError::Unavailable {
                service: "provider".to_string(),
                reason: err.to_string(),
            },
            AppError::Http { operation, source } => ApiError::Unavailable {
                service: format!("http:{operation}"),
                reason: source.to_string(),
            },
            AppError::Filesystem {
                operation,
                path,
                source,
            } => ApiError::Internal {
                message: format!("filesystem:{operation}:{path}: {source}"),
            },
            AppError::Metadata { operation, reason } => ApiError::Internal {
                message: format!("metadata:{operation}: {reason}"),
            },
            AppError::DownloadPipeline { stage, reason } => ApiError::Internal {
                message: format!("download:{stage}: {reason}"),
            },
        }
    }
}
