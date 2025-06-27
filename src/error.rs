use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct StarredError {
    pub message: String,
}

impl fmt::Display for StarredError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for StarredError {}

impl From<reqwest::Error> for StarredError {
    fn from(err: reqwest::Error) -> Self {
        StarredError {
            message: format!("HTTP request error: {}", err),
        }
    }
}

impl From<serde_json::Error> for StarredError {
    fn from(err: serde_json::Error) -> Self {
        StarredError {
            message: format!("JSON parsing error: {}", err),
        }
    }
}

impl From<reqwest::Error> for Box<StarredError> {
    fn from(err: reqwest::Error) -> Self {
        Box::new(StarredError {
            message: format!("HTTP request error: {}", err),
        })
    }
}

impl From<serde_json::Error> for Box<StarredError> {
    fn from(err: serde_json::Error) -> Self {
        Box::new(StarredError {
            message: format!("JSON parsing error: {}", err),
        })
    }
}
