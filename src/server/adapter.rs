use crate::ChromeError;
use crate::server::protocol::{Response, error_codes};
use serde::Serialize;

fn map_error_code(err: &ChromeError) -> i32 {
    match err {
        ChromeError::ElementNotFound { .. } => error_codes::ELEMENT_NOT_FOUND,
        ChromeError::NavigationTimeout(_) => error_codes::TIMEOUT,
        ChromeError::ConnectionLost | ChromeError::SessionNotFound => {
            error_codes::SESSION_NOT_FOUND
        }
        _ => error_codes::BROWSER_ERROR,
    }
}

pub trait ToResponse {
    fn to_response(self, id: u64) -> Response;
}

impl<T: Serialize> ToResponse for crate::Result<T> {
    fn to_response(self, id: u64) -> Response {
        match self {
            Ok(result) => match serde_json::to_value(&result) {
                Ok(value) => Response::success(id, value),
                Err(e) => Response::error(id, error_codes::INTERNAL_ERROR, e.to_string()),
            },
            Err(e) => Response::error(id, map_error_code(&e), e.to_string()),
        }
    }
}

macro_rules! opt_str {
    ($params:expr, $name:literal) => {
        $params.get($name).and_then(|v| v.as_str())
    };
}

macro_rules! opt_u64 {
    ($params:expr, $name:literal, $default:expr) => {
        $params
            .get($name)
            .and_then(|v| v.as_u64())
            .unwrap_or($default)
    };
}

macro_rules! opt_bool {
    ($params:expr, $name:literal, $default:expr) => {
        $params
            .get($name)
            .and_then(|v| v.as_bool())
            .unwrap_or($default)
    };
}

pub(crate) use opt_bool;
pub(crate) use opt_str;
pub(crate) use opt_u64;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Serialize)]
    struct TestResult {
        value: String,
    }

    #[test]
    fn test_to_response_success() {
        let result: crate::Result<TestResult> = Ok(TestResult {
            value: "test".into(),
        });
        let response = result.to_response(1);
        assert!(response.error.is_none());
        assert!(response.result.is_some());
    }

    #[test]
    fn test_to_response_error() {
        let result: crate::Result<TestResult> = Err(ChromeError::ElementNotFound {
            selector: "div".into(),
        });
        let response = result.to_response(1);
        assert!(response.result.is_none());
        assert_eq!(
            response.error.as_ref().unwrap().code,
            error_codes::ELEMENT_NOT_FOUND
        );
    }

    #[test]
    fn test_param_macros() {
        let params = json!({"url": "https://example.com", "timeout": 5000, "hard": true});
        assert_eq!(opt_str!(params, "url"), Some("https://example.com"));
        assert_eq!(opt_str!(params, "missing"), None);
        assert_eq!(opt_u64!(params, "timeout", 30000), 5000);
        assert_eq!(opt_u64!(params, "missing", 30000), 30000);
        assert!(opt_bool!(params, "hard", false));
        assert!(!opt_bool!(params, "missing", false));
    }
}
