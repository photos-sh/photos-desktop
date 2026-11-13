//! Errors of the API client.

use thiserror::Error;

/// The API's error codes the app branches on.
pub mod code {
    pub const UNAUTHORIZED: &str = "unauthorized";
    pub const PAYMENT_REQUIRED: &str = "payment_required";
    pub const ACCOUNT_SUSPENDED: &str = "account_suspended";
    pub const BUSY: &str = "busy";
    pub const MODEL_UNAVAILABLE: &str = "model_unavailable";
    /// Not an API code: the response was an error without the API's JSON.
    pub const UNKNOWN: &str = "unknown";
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApiError {
    /// The API answered with an error.
    #[error("{message}")]
    Api {
        status: u16,
        /// The API's `error.code`, or `unknown` when the body was not its JSON.
        code: String,
        message: String,
        /// `Retry-After`, in seconds, when the response carried one.
        retry_after: Option<u64>,
    },
    /// No response: the server could not be reached.
    #[error("could not reach the server: {0}")]
    Connection(String),
    /// No response within the timeout.
    #[error("the server took too long to answer")]
    Timeout,
    /// There is no API key to send.
    #[error("no API key is set")]
    NoKey,
    /// The request was given up because the work was cancelled.
    #[error("cancelled")]
    Cancelled,
    /// A file to send could not be read.
    #[error("{0}")]
    Io(String),
    /// The request could not be made (a bad base URL, a response that is not
    /// what the API documents).
    #[error("{0}")]
    Invalid(String),
}

impl ApiError {
    /// A short, stable code: the API's own for API errors.
    pub fn code(&self) -> &str {
        match self {
            Self::Api { code, .. } => code,
            Self::Connection(_) => "connection",
            Self::Timeout => "timeout",
            Self::NoKey => "no_key",
            Self::Cancelled => "cancelled",
            Self::Io(_) => "io",
            Self::Invalid(_) => "invalid",
        }
    }

    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Api { status, .. } => Some(*status),
            _ => None,
        }
    }

    pub fn retry_after(&self) -> Option<u64> {
        match self {
            Self::Api { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    /// An error every further request with this key would also get: a batch
    /// stops on it instead of failing file after file.
    pub fn is_fatal(&self) -> bool {
        matches!(
            self.code(),
            code::UNAUTHORIZED | code::PAYMENT_REQUIRED | code::ACCOUNT_SUSPENDED | "no_key"
        )
    }

    /// Builds the error for a non-success response.
    pub(crate) fn from_response(status: u16, retry_after: Option<u64>, body: &[u8]) -> Self {
        #[derive(serde::Deserialize)]
        struct Envelope {
            error: Inner,
        }
        #[derive(serde::Deserialize)]
        struct Inner {
            code: String,
            message: String,
        }
        match serde_json::from_slice::<Envelope>(body) {
            Ok(e) => Self::Api {
                status,
                code: e.error.code,
                message: e.error.message,
                retry_after,
            },
            Err(_) => {
                let text = String::from_utf8_lossy(body);
                let start: String = text.trim().chars().take(200).collect();
                Self::Api {
                    status,
                    code: code::UNKNOWN.to_string(),
                    message: if start.is_empty() {
                        format!("HTTP {status}")
                    } else {
                        start
                    },
                    retry_after,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_api_error_document() {
        let e = ApiError::from_response(
            402,
            None,
            br#"{"error":{"code":"payment_required","message":"Out of credits"}}"#,
        );
        assert_eq!(e.code(), "payment_required");
        assert_eq!(e.status(), Some(402));
        assert_eq!(e.to_string(), "Out of credits");
        assert!(e.is_fatal());
    }

    #[test]
    fn a_non_json_body_still_gives_an_api_error() {
        let e = ApiError::from_response(502, Some(3), b"<html>Bad Gateway</html>");
        assert_eq!(e.code(), "unknown");
        assert_eq!(e.status(), Some(502));
        assert_eq!(e.retry_after(), Some(3));
        assert_eq!(e.to_string(), "<html>Bad Gateway</html>");
        assert!(!e.is_fatal());
        assert_eq!(
            ApiError::from_response(500, None, b"").to_string(),
            "HTTP 500"
        );
    }

    #[test]
    fn fatal_errors_are_the_account_ones() {
        let api = |code: &str| ApiError::Api {
            status: 400,
            code: code.into(),
            message: String::new(),
            retry_after: None,
        };
        assert!(api("unauthorized").is_fatal());
        assert!(api("account_suspended").is_fatal());
        assert!(ApiError::NoKey.is_fatal());
        assert!(!api("bad_request").is_fatal());
        assert!(!api("busy").is_fatal());
        assert!(!ApiError::Timeout.is_fatal());
        assert!(!ApiError::Connection("refused".into()).is_fatal());
    }
}
