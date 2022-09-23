use rocket::{
    response::{self, Responder},
    Request,
};

pub type Result<T = ()> = std::result::Result<T, Error>;

/// Wrapper around [`anyhow::Error`]
/// with rocket's [responder] implemented
///
/// [anyhow::Error]: https://docs.rs/anyhow/1.0/anyhow/struct.Error.html
/// [responder]: https://api.rocket.rs/v0.4/rocket/response/trait.Responder.html
/// Error that can be convert into `anyhow::Error` can be convert directly to this type.
///
/// Responder part are internally delegated to [rocket::response::Debug] which
/// "debug prints the internal value before responding with a 500 error"
///
/// [rocket::response::Debug]: https://api.rocket.rs/v0.4/rocket/response/struct.Debug.html
#[derive(Debug)]
pub struct Error(pub anyhow::Error);

impl<E> From<E> for Error
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        Error(error.into())
    }
}

impl<'r> Responder<'r, 'r> for Error {
    fn respond_to(self, request: &Request<'_>) -> response::Result<'r> {
        response::Debug(self.0).respond_to(request)
    }
}
