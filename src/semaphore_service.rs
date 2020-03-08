//! This module exposes the sate of the server via an HTTP interface. As such it primary concerns
//! are modding success and error states to HTTP status codes. Defining in which formats to
//! deserialize paramaters and serialize respones, or deciding on which HTTP methods to map the
//! functions.

use crate::state::{Error, State};
use actix_web::{
    delete, get,
    http::StatusCode,
    post, put,
    web::{Data, Json, Path, Query},
    HttpResponse, ResponseError,
};
use log::{debug, warn};
use serde::Deserialize;
use std::{collections::HashMap, time::Duration};

impl ResponseError for Error {
    fn status_code(&self) -> StatusCode {
        match self {
            Error::UnknownLease => StatusCode::BAD_REQUEST,
            Error::UnknownSemaphore => StatusCode::BAD_REQUEST,
            Error::ForeverPending { .. } => StatusCode::CONFLICT,
        }
    }
}

type Admissions = HashMap<String, u32>;

/// Parameters for acquiring a lease
#[derive(Deserialize)]
pub struct PendingAdmissions {
    pending: Admissions,
    /// Duration in seconds. After the specified time has passed the lease may be freed by litter
    /// collection.
    #[serde(with = "humantime_serde")]
    expires_in: Duration,
}

impl PendingAdmissions {
    fn pending(&self) -> Option<(&str, u32)> {
        self.pending
            .iter()
            .next()
            .map(|(sem, &amount)| (sem.as_str(), amount))
    }
}

/// Parameters for heartbeat to a lease
#[derive(Deserialize)]
pub struct ActiveAdmissions {
    active: Admissions,
    /// Duration in seconds. After the specified time has passed the lease may be freed by litter
    /// collection.
    #[serde(with = "humantime_serde")]
    expires_in: Duration,
}

impl ActiveAdmissions {
    fn active(&self) -> Option<(&str, u32)> {
        self.active
            .iter()
            .next()
            .map(|(sem, &amount)| (sem.as_str(), amount))
    }
}

/// Acquire a new lease to a Semaphore
#[post("/acquire")]
async fn acquire(body: Json<PendingAdmissions>, state: Data<State>) -> HttpResponse {
    if let Some((semaphore, amount)) = body.pending() {
        match state.acquire(semaphore, amount, body.expires_in) {
            Ok((lease_id, true)) => HttpResponse::Created().json(lease_id),
            Ok((lease_id, false)) => HttpResponse::Accepted().json(lease_id),
            Err(error) => HttpResponse::from_error(error.into()),
        }
    } else {
        HttpResponse::BadRequest().json("Empty leases are not supported, yet.")
    }
}

#[derive(Deserialize)]
struct MaxTimeout {
    timeout_ms: Option<u64>,
}

/// Wait for a ticket to be promoted to a lease
#[post("/leases/{id}/block_until_acquired")]
async fn block_until_acquired(
    path: Path<u64>,
    query: Query<MaxTimeout>,
    body: Json<PendingAdmissions>,
    state: Data<State>,
) -> Result<Json<bool>, Error> {
    let lease_id = *path;
    let timeout = Duration::from_millis(query.timeout_ms.unwrap_or(0));
    debug!(
        "Lease {} is waiting for admission with timeout {:?}",
        lease_id, timeout
    );
    let expires_in = body.expires_in;
    if let Some((semaphore, amount)) = body.pending() {
        state
            .block_until_acquired(lease_id, expires_in, semaphore, amount, timeout)
            .map(Json)
    } else {
        Ok(Json(true))
    }
}

/// Query parameters for getting remaining semaphore count
#[derive(Deserialize)]
struct Remainder {
    semaphore: String,
}

/// Get the remainder of a semaphore
#[get("/remainder")]
async fn remainder(query: Query<Remainder>, state: Data<State>) -> Result<Json<i64>, Error> {
    state.remainder(&query.semaphore).map(Json)
}

#[delete("/leases/{id}")]
async fn release(path: Path<u64>, state: Data<State>) -> HttpResponse {
    if state.release(*path) {
        HttpResponse::Ok().json("Lease released")
    } else {
        // Post condition of lease not being there is satisfied, let's make this request 200 still.
        HttpResponse::Ok().json("Lease not found")
    }
}

/// Manually remove all expired semapahores. Usefull for testing
#[post("/remove_expired")]
async fn remove_expired(state: Data<State>) -> Json<usize> {
    debug!("Remove expired triggered");
    Json(state.remove_expired())
}

#[put("/leases/{id}")]
async fn put_lease(
    path: Path<u64>,
    body: Json<ActiveAdmissions>,
    state: Data<State>,
) -> Result<&'static str, Error> {
    let lease_id = *path;
    if let Some((semaphore, amount)) = body.active() {
        debug!("Received heartbeat for {}", lease_id);
        state.heartbeat_to_active_lease(lease_id, semaphore, amount, body.expires_in)?;
    } else {
        warn!("Empty heartbeat (no active leases) for {}", lease_id);
    }

    Ok("Ok")
}
