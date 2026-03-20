use std::sync::{Arc, Mutex, OnceLock, mpsc::Sender};

use crate::task_utils::execute;
use walkers::Position;

const OPEN_TOPO_DATA_URL: &str = "https://api.opentopodata.org/v1/mapzen";
// Public API limit documented by OpenTopoData.
const MAX_POSITIONS_PER_REQUEST: usize = 100;

// ---------------------------------------------------------------------------
// SRTM3 HGT offline elevation database
// ---------------------------------------------------------------------------

/// One 1°×1° SRTM HGT tile loaded into memory.
struct HgtTile {
    /// Integer latitude of the south edge (e.g. 45 for N45).
    lat_min: i16,
    /// Integer longitude of the west edge (e.g. 6 for E006).
    lon_min: i16,
    /// Number of samples per side (1201 for SRTM3, 3601 for SRTM1).
    size: usize,
    /// Row-major, big-endian i16 elevations. Row 0 = northernmost row.
    data: Vec<i16>,
}

impl HgtTile {
    fn from_bytes(lat_min: i16, lon_min: i16, bytes: &[u8]) -> Option<Self> {
        let n_values = bytes.len() / 2;
        let size = (n_values as f64).sqrt().round() as usize;
        if size * size * 2 != bytes.len() {
            return None;
        }
        let data: Vec<i16> = bytes
            .chunks_exact(2)
            .map(|b| i16::from_be_bytes([b[0], b[1]]))
            .collect();
        Some(HgtTile {
            lat_min,
            lon_min,
            size,
            data,
        })
    }

    fn elevation_at(&self, lat: f64, lon: f64) -> Option<f64> {
        let lat_min = self.lat_min as f64;
        let lon_min = self.lon_min as f64;
        if lat < lat_min || lat >= lat_min + 1.0 || lon < lon_min || lon >= lon_min + 1.0 {
            return None;
        }
        let n = self.size - 1;
        // Row 0 is the northernmost row.
        let row_f = (lat_min + 1.0 - lat) * n as f64;
        let col_f = (lon - lon_min) * n as f64;
        let row = (row_f as usize).min(n - 1);
        let col = (col_f as usize).min(n - 1);
        let dr = row_f - row as f64;
        let dc = col_f - col as f64;
        let v00 = self.sample(row, col)?;
        let v01 = self.sample(row, col + 1)?;
        let v10 = self.sample(row + 1, col)?;
        let v11 = self.sample(row + 1, col + 1)?;
        Some(
            v00 * (1.0 - dr) * (1.0 - dc)
                + v01 * (1.0 - dr) * dc
                + v10 * dr * (1.0 - dc)
                + v11 * dr * dc,
        )
    }

    fn sample(&self, row: usize, col: usize) -> Option<f64> {
        let v = *self.data.get(row * self.size + col)?;
        if v == -32768 { None } else { Some(v as f64) }
    }
}

/// Parses an HGT filename stem like "N45E006" into (lat_min, lon_min).
fn parse_hgt_filename(path: &std::path::Path) -> Option<(i16, i16)> {
    let stem = path.file_stem()?.to_str()?.to_uppercase();
    let lat_sign: i16 = match stem.chars().next()? {
        'N' => 1,
        'S' => -1,
        _ => return None,
    };
    let lat: i16 = stem.get(1..3)?.parse().ok()?;
    let rest = stem.get(3..)?;
    let lon_sign: i16 = match rest.chars().next()? {
        'E' => 1,
        'W' => -1,
        _ => return None,
    };
    let lon: i16 = rest.get(1..)?.parse().ok()?;
    Some((lat * lat_sign, lon * lon_sign))
}

/// Collection of loaded SRTM tiles for offline elevation lookup.
pub struct OfflineElevationDb {
    tiles: Vec<HgtTile>,
}

impl OfflineElevationDb {
    pub fn elevation_at(&self, lat: f64, lon: f64) -> Option<f64> {
        self.tiles.iter().find_map(|t| t.elevation_at(lat, lon))
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }
}

static OFFLINE_DB: OnceLock<OfflineElevationDb> = OnceLock::new();

/// SRTM3 tiles compiled into the binary.
const EMBEDDED_TILES: &[(&str, &[u8])] = &[
    ("N45E005", include_bytes!("../assets/dem/N45E005.hgt")),
    ("N45E006", include_bytes!("../assets/dem/N45E006.hgt")),
    ("N45E007", include_bytes!("../assets/dem/N45E007.hgt")),
    ("N46E005", include_bytes!("../assets/dem/N46E005.hgt")),
    ("N46E006", include_bytes!("../assets/dem/N46E006.hgt")),
    ("N46E007", include_bytes!("../assets/dem/N46E007.hgt")),
];

/// Initialize the offline elevation database from tiles embedded in the binary.
/// Call once at app startup. Silently does nothing on second call.
pub fn init_offline_elevation() {
    let mut tiles = Vec::new();
    for (name, bytes) in EMBEDDED_TILES {
        // Derive (lat, lon) from the embedded name string via a temporary Path.
        let path = std::path::Path::new(name).with_extension("hgt");
        let Some((lat, lon)) = parse_hgt_filename(&path) else {
            log::warn!("Cannot parse embedded tile name: {name}");
            continue;
        };
        let Some(tile) = HgtTile::from_bytes(lat, lon, bytes) else {
            log::warn!("Invalid embedded tile: {name} ({} bytes)", bytes.len());
            continue;
        };
        log::info!(
            "Embedded SRTM tile {}{}{}E{:03} ({} samples/side)",
            if lat >= 0 { "N" } else { "S" },
            lat.unsigned_abs(),
            if lon >= 0 { "E" } else { "W" },
            lon.unsigned_abs(),
            tile.size,
        );
        tiles.push(tile);
    }
    let db = OfflineElevationDb { tiles };
    log::info!(
        "Offline elevation ready ({} embedded tiles)",
        db.tile_count()
    );
    let _ = OFFLINE_DB.set(db);
}

#[derive(serde::Serialize)]
struct ElevationRequest {
    locations: String,
}

#[derive(serde::Deserialize)]
struct ElevationResponse {
    status: String,
    results: Option<Vec<ElevationResult>>,
}

#[derive(serde::Deserialize)]
struct ElevationResult {
    elevation: Option<f64>,
    location: ElevationLocation,
}

#[derive(serde::Deserialize)]
struct ElevationLocation {
    lat: f64,
    lng: f64,
}

pub type ElevationResults = (
    Sender<Vec<(Position, f64)>>,
    std::sync::mpsc::Receiver<Vec<(Position, f64)>>,
);

fn build_elevation_request(positions: &[Position]) -> ehttp::Request {
    let locations = positions
        .iter()
        .map(|pos| format!("{},{}", pos.y(), pos.x()))
        .collect::<Vec<_>>()
        .join("|");

    let request_body = serde_json::to_string(&ElevationRequest { locations }).unwrap_or_default();

    #[cfg(target_arch = "wasm32")]
    {
        let mut request = ehttp::Request {
            url: OPEN_TOPO_DATA_URL.to_string(),
            method: "POST".to_string(),
            body: request_body.into_bytes(),
            headers: Default::default(),
            mode: Default::default(),
        };
        request
            .headers
            .insert("Content-Type".to_string(), "application/json".to_string());
        request
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut request = ehttp::Request {
            url: OPEN_TOPO_DATA_URL.to_string(),
            method: "POST".to_string(),
            body: request_body.into_bytes(),
            headers: Default::default(),
        };
        request
            .headers
            .insert("Content-Type".to_string(), "application/json".to_string());
        request
    }
}

fn parse_elevation_response(
    response_result: Result<ehttp::Response, String>,
) -> Vec<(Position, f64)> {
    match response_result {
        Ok(response) if response.ok => {
            log::debug!("OpenTopoData responded with status {}", response.status);
            let text = response.text().unwrap_or_default();
            log::debug!("Elevation response body length: {}", text.len());

            match serde_json::from_str::<ElevationResponse>(text) {
                Ok(payload) => {
                    if payload.status != "OK" {
                        log::warn!("OpenTopoData returned non-OK status: {}", payload.status);
                        return Vec::new();
                    }

                    let results = payload
                        .results
                        .unwrap_or_default()
                        .into_iter()
                        .filter_map(|result| {
                            result.elevation.map(|elevation| {
                                (
                                    walkers::lon_lat(result.location.lng, result.location.lat),
                                    elevation,
                                )
                            })
                        })
                        .collect::<Vec<_>>();

                    log::info!("Successfully parsed {} elevation results", results.len());
                    results
                }
                Err(e) => {
                    log::error!("Failed to parse elevation response: {e}");
                    Vec::new()
                }
            }
        }
        Ok(response) => {
            log::warn!(
                "Elevation lookup failed with status {} {}",
                response.status,
                response.status_text
            );
            Vec::new()
        }
        Err(error) => {
            log::error!("Elevation lookup request failed: {error}");
            Vec::new()
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn yield_to_runtime() {
    use futures::channel::oneshot;
    use wasm_bindgen::{JsCast, closure::Closure};

    let Some(window) = web_sys::window() else {
        return;
    };

    let (sender, receiver) = oneshot::channel();
    let callback = Closure::once(move || {
        let _ = sender.send(());
    });

    if window
        .request_animation_frame(callback.as_ref().unchecked_ref())
        .is_ok()
    {
        callback.forget();
        let _ = receiver.await;
    }
}

async fn split_offline_coverage_async(
    positions: Vec<Position>,
) -> (Vec<(Position, f64)>, Vec<Position>) {
    #[cfg(target_arch = "wasm32")]
    yield_to_runtime().await;

    let Some(db) = OFFLINE_DB.get() else {
        return (Vec::new(), positions);
    };
    if db.is_empty() {
        return (Vec::new(), positions);
    }

    let mut elevations = Vec::new();
    let mut missing = Vec::new();

    #[cfg(target_arch = "wasm32")]
    let chunk_size = 32;
    #[cfg(not(target_arch = "wasm32"))]
    let chunk_size = positions.len().max(1);

    for chunk in positions.chunks(chunk_size) {
        for pos in chunk {
            if let Some(elevation) = db.elevation_at(pos.y(), pos.x()) {
                elevations.push((*pos, elevation));
            } else {
                missing.push(*pos);
            }
        }

        #[cfg(target_arch = "wasm32")]
        yield_to_runtime().await;
    }

    (elevations, missing)
}

async fn fetch_via_api_async(positions: Vec<Position>) -> Vec<(Position, f64)> {
    if positions.is_empty() {
        return Vec::new();
    }

    let (sender, receiver) = futures::channel::oneshot::channel();
    fetch_via_api(positions, move |elevations| {
        let _ = sender.send(elevations);
    });

    receiver.await.unwrap_or_default()
}

/// Request elevation data for a list of positions.
///
/// Uses the offline SRTM database when available (fast, no network).
/// Falls back to the OpenTopoData API for any position not covered offline.
pub fn fetch_elevation_for_positions(
    positions: Vec<Position>,
    sender: Sender<Vec<(Position, f64)>>,
) {
    if positions.is_empty() {
        return;
    }

    let total_positions = positions.len();
    execute(async move {
        let (offline_elevations, missing) = split_offline_coverage_async(positions).await;

        if missing.is_empty() {
            let _ = sender.send(offline_elevations);
            return;
        }

        if offline_elevations.is_empty() {
            log::debug!(
                "Sending API-only elevation request for {} waypoints",
                missing.len()
            );
            let api_elevations = fetch_via_api_async(missing).await;
            let _ = sender.send(api_elevations);
            return;
        }

        log::debug!(
            "Offline elevation: {}/{} offline, {} need API",
            offline_elevations.len(),
            total_positions,
            missing.len()
        );
        let mut combined = offline_elevations;
        combined.extend(fetch_via_api_async(missing).await);
        let _ = sender.send(combined);
    });
}

/// Fetches elevation for `positions` via the OpenTopoData API in sequential chunks,
/// then calls `on_done` with the aggregated results.
fn fetch_via_api<F>(positions: Vec<Position>, on_done: F)
where
    F: FnOnce(Vec<(Position, f64)>) + Send + 'static,
{
    let chunks: Vec<Vec<Position>> = positions
        .chunks(MAX_POSITIONS_PER_REQUEST)
        .map(|chunk| chunk.to_vec())
        .collect();

    if chunks.len() == 1 {
        let request = build_elevation_request(&chunks[0]);
        ehttp::fetch(request, move |response_result| {
            on_done(parse_elevation_response(response_result));
        });
        return;
    }

    struct ChunkState<F> {
        chunks: std::collections::VecDeque<Vec<Position>>,
        combined: Vec<(Position, f64)>,
        on_done: Option<F>,
    }

    log::info!(
        "Splitting elevation request into {} chunks ({} max points/chunk)",
        chunks.len(),
        MAX_POSITIONS_PER_REQUEST
    );

    let state = Arc::new(Mutex::new(ChunkState {
        chunks: chunks.into_iter().collect(),
        combined: Vec::new(),
        on_done: Some(on_done),
    }));

    fn next_chunk<F>(state: Arc<Mutex<ChunkState<F>>>)
    where
        F: FnOnce(Vec<(Position, f64)>) + Send + 'static,
    {
        let next = {
            let mut lock = match state.lock() {
                Ok(l) => l,
                Err(_) => return,
            };
            lock.chunks.pop_front()
        };

        let Some(chunk) = next else {
            let (combined, cb) = {
                let mut lock = match state.lock() {
                    Ok(l) => l,
                    Err(_) => return,
                };
                (std::mem::take(&mut lock.combined), lock.on_done.take())
            };
            if let Some(f) = cb {
                f(combined);
            }
            return;
        };

        let request = build_elevation_request(&chunk);
        let state_ref = Arc::clone(&state);
        ehttp::fetch(request, move |response_result| {
            let chunk_elevations = parse_elevation_response(response_result);
            if let Ok(mut lock) = state_ref.lock() {
                lock.combined.extend(chunk_elevations);
            }
            next_chunk(state_ref);
        });
    }

    next_chunk(state);
}
