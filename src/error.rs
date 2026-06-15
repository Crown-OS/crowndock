use thiserror::Error;

pub type Result<T, E = AppError> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("toml parse: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("toml serialize: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("wayland connect: {0}")]
    WaylandConnect(#[from] wayland_client::ConnectError),

    #[error("wayland global: {0}")]
    WaylandGlobal(#[from] wayland_client::globals::GlobalError),

    #[error("wayland dispatch: {0}")]
    WaylandDispatch(#[from] wayland_client::DispatchError),

    #[error("shm pool: {0}")]
    ShmPool(#[from] smithay_client_toolkit::shm::CreatePoolError),

    #[error("calloop: {0}")]
    Calloop(String),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}

impl From<calloop::Error> for AppError {
    fn from(e: calloop::Error) -> Self {
        Self::Calloop(e.to_string())
    }
}

impl<T> From<calloop::InsertError<T>> for AppError {
    fn from(e: calloop::InsertError<T>) -> Self {
        Self::Calloop(e.error.to_string())
    }
}
