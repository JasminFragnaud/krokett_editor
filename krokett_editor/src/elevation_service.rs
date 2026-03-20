use std::sync::{Arc, Mutex};

use walkers::Position;

const OPEN_TOPO_DATA_URL: &str = "https://api.opentopodata.org/v1/mapzen";
// Public API limit documented by OpenTopoData.
const MAX_POSITIONS_PER_REQUEST: usize = 100;

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

pub type ElevationResults = Arc<Mutex<Option<Vec<(Position, f64)>>>>;

fn build_elevation_request(positions: &[Position]) -> ehttp::Request {
    let locations = positions
        .iter()
        .map(|pos| format!("{},{}", pos.y(), pos.x()))
        .collect::<Vec<_>>()
        .join("|");

    let request_body = serde_json::to_string(&ElevationRequest { locations }).unwrap_or_default();

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

fn parse_elevation_response(response_result: Result<ehttp::Response, String>) -> Vec<(Position, f64)> {
    match response_result {
        Ok(response) if response.ok => {
            log::debug!("OpenTopoData responded with status {}", response.status);
            let text = response.text().unwrap_or_default();
            log::debug!("Elevation response body length: {}", text.len());

            match serde_json::from_str::<ElevationResponse>(&text) {
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
                    log::error!("Failed to parse elevation response: {}", e);
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

/// Request elevation data for a batch of positions from Open Elevation API
pub fn fetch_elevation_for_positions(
    positions: Vec<Position>,
    results: ElevationResults,
) {
    if positions.is_empty() {
        return;
    }

    log::debug!(
        "Sending elevation request for {} waypoints",
        positions.len()
    );

    let chunks: Vec<Vec<Position>> = positions
        .chunks(MAX_POSITIONS_PER_REQUEST)
        .map(|chunk| chunk.to_vec())
        .collect();

    if chunks.len() == 1 {
        let request = build_elevation_request(&chunks[0]);
        ehttp::fetch(request, move |response_result| {
            let elevations = parse_elevation_response(response_result);
            if let Ok(mut lock) = results.lock() {
                *lock = Some(elevations);
            }
        });
        return;
    }

    struct ChunkState {
        chunks: std::collections::VecDeque<Vec<Position>>,
        combined: Vec<(Position, f64)>,
    }

    log::info!(
        "Splitting elevation request into {} chunks ({} max points/chunk)",
        chunks.len(),
        MAX_POSITIONS_PER_REQUEST
    );

    let state = Arc::new(Mutex::new(ChunkState {
        chunks: chunks.into_iter().collect(),
        combined: Vec::new(),
    }));

    fn fetch_next_chunk(state: Arc<Mutex<ChunkState>>, results: ElevationResults) {
        let next_chunk = {
            let mut lock = match state.lock() {
                Ok(lock) => lock,
                Err(_) => return,
            };
            lock.chunks.pop_front()
        };

        let Some(chunk) = next_chunk else {
            let final_result = {
                let mut lock = match state.lock() {
                    Ok(lock) => lock,
                    Err(_) => return,
                };
                std::mem::take(&mut lock.combined)
            };

            if let Ok(mut lock) = results.lock() {
                *lock = Some(final_result);
            }
            return;
        };

        let request = build_elevation_request(&chunk);
        let state_ref = Arc::clone(&state);
        let results_ref = Arc::clone(&results);

        ehttp::fetch(request, move |response_result| {
            let chunk_elevations = parse_elevation_response(response_result);
            if let Ok(mut lock) = state_ref.lock() {
                lock.combined.extend(chunk_elevations);
            }
            fetch_next_chunk(state_ref, results_ref);
        });
    }

    fetch_next_chunk(state, results);
}
