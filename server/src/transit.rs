/// Server-side proxy for VRR (Verkehrsverbund Rhein-Ruhr) EFA endpoints,
/// the canonical source for Düsseldorf / Rheinbahn departures. We proxy
/// instead of letting the browser fetch directly because EFA does not
/// send Access-Control-Allow-Origin, so a browser fetch would be blocked.
///
/// Two endpoints, both auth-gated (session cookie):
///   GET /api/v1/transit/departures?stop_id=20018235
///   GET /api/v1/transit/nearby?lat=51.2277&lon=6.7735
///
/// Response shape mirrors what the SPA was already getting from
/// v6.db.transport.rest, so the TransitWidget keeps its existing types
/// and only the URL changes.
///
/// Light per-stop cache (30s) so a refresh-spam doesn't hammer EFA, and
/// so multiple concurrent kiosks share work.
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::{Query, State},
    http::header,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::session::AuthUser;
use crate::AppState;

const EFA_BASE: &str = "https://efa.vrr.de/standard";
const CACHE_TTL_SECS: u64 = 30;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/departures", get(departures))
        .route("/nearby", get(nearby))
}

// ---- Public response shapes (the SPA consumes these) -----------------------

#[derive(Serialize)]
pub struct DepartureView {
    pub trip_id: String,
    /// ISO 8601, real if available, else planned. Null on parse failure.
    pub when: Option<String>,
    pub planned_when: Option<String>,
    /// Seconds (positive = late). Null when there's no realtime info.
    pub delay: Option<i32>,
    pub direction: String,
    pub line: LineView,
    pub cancelled: bool,
}

#[derive(Serialize)]
pub struct LineView {
    /// Short label suitable for the line badge — "S 1", "U 70", "ICE 642".
    pub name: String,
    /// Coarse mode bucket: "suburban" | "subway" | "tram" | "bus" |
    /// "regional" | "longdistance" | "ferry" | "other". Drives the
    /// colour mapping in the widget.
    pub product: Option<String>,
}

#[derive(Serialize)]
pub struct StopView {
    pub id: String,
    pub name: String,
    /// Distance from the requested point, in metres.
    pub distance: i32,
}

// ---- EFA wire types --------------------------------------------------------
// Only the fields we use; serde tolerates extras by default.

#[derive(Deserialize)]
struct EfaDmResponse {
    #[serde(default, rename = "departureList")]
    departures: Vec<EfaDeparture>,
}

#[derive(Deserialize)]
struct EfaDeparture {
    #[serde(default, rename = "dateTime")]
    date_time: Option<EfaDateTime>,
    #[serde(default, rename = "realDateTime")]
    real_date_time: Option<EfaDateTime>,
    #[serde(default, rename = "realtimeTripStatus")]
    realtime_trip_status: Option<String>,
    #[serde(default, rename = "servingLine")]
    serving_line: Option<EfaServingLine>,
    #[serde(default)]
    attrs: Vec<EfaAttr>,
}

#[derive(Deserialize, Default)]
struct EfaDateTime {
    #[serde(default)]
    year: String,
    #[serde(default)]
    month: String,
    #[serde(default)]
    day: String,
    #[serde(default)]
    hour: String,
    #[serde(default)]
    minute: String,
}

#[derive(Deserialize, Default)]
struct EfaServingLine {
    #[serde(default)]
    number: String,
    #[serde(default)]
    symbol: String,
    #[serde(default)]
    direction: String,
    #[serde(default)]
    delay: String,
    #[serde(default, rename = "motType")]
    mot_type: String,
    #[serde(default, rename = "trainType")]
    train_type: String,
    #[serde(default, rename = "trainNum")]
    train_num: String,
    #[serde(default)]
    stateless: String,
}

#[derive(Deserialize, Default)]
struct EfaAttr {
    #[serde(default)]
    name: String,
    #[serde(default)]
    value: String,
}

#[derive(Deserialize)]
struct EfaCoordResponse {
    #[serde(default)]
    pins: Vec<EfaPin>,
}

#[derive(Deserialize)]
struct EfaPin {
    #[serde(default)]
    id: String,
    #[serde(default)]
    desc: String,
    #[serde(default, rename = "type")]
    pin_type: String,
    #[serde(default)]
    distance: String,
}

// ---- Departures endpoint ---------------------------------------------------

#[derive(Deserialize)]
pub struct DeparturesQuery {
    pub stop_id: String,
    /// Cap returned departures. Defaults to 8, clamped to [1, 30].
    #[serde(default)]
    pub limit: Option<u8>,
}

async fn departures(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Query(q): Query<DeparturesQuery>,
) -> AppResult<Response> {
    if q.stop_id.is_empty() || q.stop_id.len() > 32 {
        return Err(AppError::BadRequest("invalid stop_id".into()));
    }
    if !q.stop_id.chars().all(|c| c.is_ascii_alphanumeric() || c == ':') {
        return Err(AppError::BadRequest("invalid stop_id".into()));
    }
    let limit = q.limit.unwrap_or(8).clamp(1, 30);

    let cache_key = format!("dep:{}:{}", q.stop_id, limit);
    if let Some(hit) = state.transit_cache.lock().get(&cache_key).cloned() {
        if hit.expires_at > Instant::now() {
            return Ok(json_response(hit.payload));
        }
    }

    // stop_id is already restricted to [A-Za-z0-9:] above, so it's safe
    // to drop into the query string verbatim — no encoding needed.
    let url = format!(
        "{EFA_BASE}/XML_DM_REQUEST?outputFormat=JSON&type_dm=any&useRealtime=1\
         &mode=direct&itdDateTimeDepArr=dep&limit={limit}&name_dm={stop}",
        stop = q.stop_id,
        limit = limit
    );

    let efa: EfaDmResponse = fetch_json(&state.http, &url).await?;
    let mut out: Vec<DepartureView> = efa
        .departures
        .into_iter()
        .map(map_departure)
        .collect();
    // Defensive truncate in case EFA ignored the limit hint.
    out.truncate(limit as usize);

    let payload = serde_json::to_string(&out)
        .map_err(|e| AppError::Other(anyhow::anyhow!("serialise: {}", e)))?;
    cache_put(&state.transit_cache, cache_key, payload.clone());
    Ok(json_response(payload))
}

fn json_response(body: String) -> Response {
    (
        [(header::CONTENT_TYPE, "application/json")],
        body,
    )
        .into_response()
}

fn map_departure(d: EfaDeparture) -> DepartureView {
    let line = d.serving_line.unwrap_or_default();
    let direction = line.direction.clone();
    let planned_when = d.date_time.as_ref().and_then(efa_dt_to_iso);
    let real_when = d.real_date_time.as_ref().and_then(efa_dt_to_iso);
    let when = real_when.clone().or_else(|| planned_when.clone());
    let delay = parse_delay_minutes(&line.delay).map(|m| m * 60);
    let cancelled = d
        .realtime_trip_status
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("TRIP_CANCELLED"))
        .unwrap_or(false);
    let trip_id = compose_trip_id(&line, &planned_when, &d.attrs);
    let line_view = LineView {
        name: line_label(&line),
        product: Some(mot_to_product(&line.mot_type).into()),
    };
    DepartureView {
        trip_id,
        when,
        planned_when,
        delay,
        direction,
        line: line_view,
        cancelled,
    }
}

/// EFA's dateTime block is local time without offset. VRR's network is
/// Europe/Berlin; the dashboard only ever displays minutes-from-now and
/// hour:minute, so emitting it as a naive ISO and letting Date.parse()
/// treat it as local is good enough.
fn efa_dt_to_iso(dt: &EfaDateTime) -> Option<String> {
    let y: i32 = dt.year.parse().ok()?;
    let m: u8 = dt.month.parse().ok()?;
    let d: u8 = dt.day.parse().ok()?;
    let h: u8 = dt.hour.parse().ok()?;
    let mi: u8 = dt.minute.parse().ok()?;
    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:00",
        y, m, d, h, mi
    ))
}

fn parse_delay_minutes(s: &str) -> Option<i32> {
    if s.is_empty() {
        return None;
    }
    s.parse::<i32>().ok()
}

fn compose_trip_id(line: &EfaServingLine, planned: &Option<String>, attrs: &[EfaAttr]) -> String {
    if let Some(avms) = attrs.iter().find(|a| a.name == "AVMSTripID") {
        if !avms.value.is_empty() {
            return avms.value.clone();
        }
    }
    format!(
        "{}|{}|{}",
        line.stateless,
        planned.as_deref().unwrap_or(""),
        line.direction
    )
}

/// EFA's `motType` is a numeric mode-of-transport code (VDV 453). We
/// collapse it to the categories the widget knows how to colour. VRR's
/// production network uses:
///   0  = Zug (regional / RE / RB)
///   1  = S-Bahn
///   2  = U-Bahn
///   3  = Stadtbahn
///   4  = Straßenbahn
///   5  = Stadtbus
///   6  = Regionalbus
///   7  = Schnellbus
///   8  = Seil-/Zahnradbahn
///   9  = Schiff
///   10 = AST / Anrufsammeltaxi
///   11 = Sonderverkehr
///   16 = Hochgeschwindigkeitszug (ICE/IC/EC)
fn mot_to_product(mot: &str) -> &'static str {
    match mot {
        "0" => "regional",
        "1" => "suburban",
        "2" => "subway",
        "3" | "4" => "tram",
        "5" => "bus",
        "6" | "7" => "regional",
        "8" => "tram",
        "9" => "ferry",
        "10" => "bus",
        "11" => "other",
        "16" => "longdistance",
        _ => "other",
    }
}

/// Build a compact line label for the badge. Examples:
///   ICE 642 InterCityExpress           → "ICE 642"
///   "" (symbol="U70")                  → "U70"
///   "U 70 nach …"                      → "U70"
///   trainType=RE trainNum=10           → "RE 10"
fn line_label(line: &EfaServingLine) -> String {
    if !line.symbol.is_empty() {
        return line.symbol.replace(' ', "");
    }
    if !line.train_type.is_empty() && !line.train_num.is_empty() {
        return format!("{} {}", line.train_type, line.train_num);
    }
    if !line.number.is_empty() {
        // Take everything up to the third whitespace token at most, drop
        // the long-form name suffix ("InterCityExpress", "S-Bahn", …).
        let mut parts = line.number.split_whitespace();
        let a = parts.next().unwrap_or("");
        let b = parts.next();
        match b {
            Some(b) => format!("{a} {b}"),
            None => a.to_string(),
        }
    } else {
        "?".into()
    }
}

// ---- Nearby endpoint -------------------------------------------------------

#[derive(Deserialize)]
pub struct NearbyQuery {
    pub lat: f64,
    pub lon: f64,
    /// Search radius, metres. Defaults to 1000, clamped to [50, 5000].
    #[serde(default)]
    pub radius: Option<u32>,
    /// Max results. Defaults to 5, clamped to [1, 20].
    #[serde(default)]
    pub limit: Option<u8>,
}

async fn nearby(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Query(q): Query<NearbyQuery>,
) -> AppResult<Json<Vec<StopView>>> {
    if !q.lat.is_finite() || !q.lon.is_finite() {
        return Err(AppError::BadRequest("invalid coords".into()));
    }
    let radius = q.radius.unwrap_or(1000).clamp(50, 5000);
    let limit = q.limit.unwrap_or(5).clamp(1, 20) as usize;

    let url = format!(
        "{EFA_BASE}/XML_COORD_REQUEST?outputFormat=JSON\
         &coordOutputFormat=WGS84%5BDD.DDDDDD%5D\
         &inclFilter=1&radius_1={radius}&type_1=STOP\
         &coord={lon}:{lat}:WGS84%5BDD.DDDDDD%5D",
        radius = radius,
        lon = q.lon,
        lat = q.lat,
    );

    let efa: EfaCoordResponse = fetch_json(&state.http, &url).await?;
    let mut out: Vec<StopView> = efa
        .pins
        .into_iter()
        .filter(|p| p.pin_type == "STOP" && !p.id.is_empty())
        .map(|p| StopView {
            id: p.id,
            name: p.desc,
            distance: p.distance.parse().unwrap_or(-1),
        })
        .collect();
    out.sort_by_key(|s| if s.distance < 0 { i32::MAX } else { s.distance });
    out.truncate(limit);
    Ok(Json(out))
}

// ---- Plumbing --------------------------------------------------------------

async fn fetch_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
) -> AppResult<T> {
    let res = client.get(url).send().await.map_err(|e| {
        tracing::warn!(err = %e, "transit upstream fetch failed");
        AppError::BadGateway("vrr efa unavailable".into())
    })?;
    if !res.status().is_success() {
        let status = res.status();
        tracing::warn!(status = %status, "transit upstream returned non-2xx");
        return Err(AppError::BadGateway(format!("vrr efa {}", status)));
    }
    res.json::<T>().await.map_err(|e| {
        tracing::warn!(err = %e, "transit response parse failed");
        AppError::BadGateway("vrr efa parse failed".into())
    })
}

// Tiny in-memory cache so repeat requests inside CACHE_TTL_SECS don't hit
// EFA. Keyed by the canonical request signature, payload is a JSON string
// of the already-mapped response. Eviction is lazy on read.
#[derive(Clone)]
pub struct CachedResponse {
    pub payload: String,
    pub expires_at: Instant,
}

pub type TransitCache = Arc<Mutex<HashMap<String, CachedResponse>>>;

pub fn new_cache() -> TransitCache {
    Arc::new(Mutex::new(HashMap::new()))
}

fn cache_put(cache: &TransitCache, key: String, payload: String) {
    let entry = CachedResponse {
        payload,
        expires_at: Instant::now() + Duration::from_secs(CACHE_TTL_SECS),
    };
    let mut g = cache.lock();
    // Cheap GC: drop expired entries on every write.
    let now = Instant::now();
    g.retain(|_, v| v.expires_at > now);
    g.insert(key, entry);
}
