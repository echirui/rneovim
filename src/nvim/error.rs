use std::fmt;
use std::io;

#[derive(Debug)]
pub enum NvimError {
    /// Standard I/O error
    Io(io::Error),
    /// Buffer-specific error (e.g., invalid line number)
    Buffer(String),
    /// Window-specific error
    Window(String),
    /// API/Request error
    Api(String),
    /// Lua runtime error
    Lua(String),
    /// Operation not permitted (e.g., readonly)
    ReadOnly,
}

impl fmt::Display for NvimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NvimError::Io(err) => write!(f, "I/O error: {}", err),
            NvimError::Buffer(msg) => write!(f, "Buffer error: {}", msg),
            NvimError::Window(msg) => write!(f, "Window error: {}", msg),
            NvimError::Api(msg) => write!(f, "API error: {}", msg),
            NvimError::Lua(msg) => write!(f, "Lua error: {}", msg),
            NvimError::ReadOnly => write!(f, "Error: Buffer is readonly"),
        }
    }
}

impl std::error::Error for NvimError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NvimError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for NvimError {
    fn from(err: io::Error) -> Self {
        NvimError::Io(err)
    }
}

impl From<mlua::Error> for NvimError {
    fn from(err: mlua::Error) -> Self {
        NvimError::Lua(err.to_string())
    }
}

/// A specialized Result type for Nvim operations.
pub type Result<T> = std::result::Result<T, NvimError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_error_conversion() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "not found");
        let nvim_err: NvimError = io_err.into();
        assert!(matches!(nvim_err, NvimError::Io(_)));
        
        let lua_err = mlua::Error::RuntimeError("lua crash".to_string());
        let nvim_err: NvimError = lua_err.into();
        assert!(matches!(nvim_err, NvimError::Lua(_)));
    }
}
