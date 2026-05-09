use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("scene file not found: {0}")]
    SceneNotFound(String),

    #[error("shader compile error: {0}")]
    ShaderCompile(String),

    #[error("scene metadata parse error in {file}: {source}")]
    SceneMeta {
        file: String,
        #[source]
        source: toml::de::Error,
    },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("render backend error: {0}")]
    Backend(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_compile_displays_message() {
        let err = Error::ShaderCompile("test message".into());
        assert_eq!(err.to_string(), "shader compile error: test message");
    }

    #[test]
    fn io_converts_from_std() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
    }
}
