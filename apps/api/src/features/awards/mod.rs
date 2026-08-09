use std::net::SocketAddr;

use crate::{error::AppError, features::auth::AuthContext, state::AppState};
use axum::{
    Json,
    extract::{ConnectInfo, Path, State, rejection::JsonRejection},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

mod handlers;
mod model;
mod service;
#[cfg(test)]
mod tests;

pub use handlers::*;
pub use model::*;
pub use service::AwardService;
