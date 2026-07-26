pub(crate) mod handlers;
mod model;
mod service;

pub use handlers::{list, replace};
pub use service::ContestAdminScopeService;
