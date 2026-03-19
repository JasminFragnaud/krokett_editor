use std::sync::{Arc, Mutex};

use walkers::Position;

const OPEN_ELEVATION_URL: &str = "https://api.open-elevation.com/api/v1/lookup";

#[derive(serde::Serialize)]
struct ElevationRequest {
    locations: Vec<LocationPoint>,
}

#[derive(serde::Serialize)]
struct LocationPoint {
    latitude: f64,
    longitude: f64,
}

#[derive(serde::Deserialize)]
struct ElevationResponse {
    results: Vec<ElevationResult>,
}

#[derive(serde::Deserialize)]
struct ElevationResult {
    elevation: f64,
    latitude: f64,
    longitude: f64,
}

pub type ElevationResults = Arc<Mutex<Option<Vec<(Position, f64)>>>>;

/// Request elevation data for a batch of positions from Open Elevation API
pub fn fetch_elevation_for_positions(
    positions: Vec<Position>,
    results: ElevationResults,
) {
    if positions.is_empty() {
        return;
    }

    let locations: Vec<LocationPoint> = positions
        .iter()
        .map(|pos| LocationPoint {
            latitude: pos.y(),
            longitude: pos.x(),
        })
        .collect();

    let request_body = serde_json::to_string(&ElevationRequest { locations })
        .unwrap_or_default();

    let mut request = ehttp::Request {
        url: OPEN_ELEVATION_URL.to_string(),
        method: "POST".to_string(),
        body: request_body.into_bytes(),
        headers: Default::default(),
    };
    request.headers.insert("Content-Type".to_string(), "application/json".to_string());

    log::debug!("Sending elevation request for {} waypoints", positions.len());

    ehttp::fetch(request, move |response_result| {
        let elevations = match response_result {
            Ok(response) if response.ok => {
                log::debug!("Elevation API responded with status {}", response.status);
                let text = response.text().unwrap_or_default();
                log::debug!("Elevation response body length: {}", text.len());
                
                match serde_json::from_str::<ElevationResponse>(&text) {
                    Ok(payload) => {
                        log::info!("Successfully parsed {} elevation results", payload.results.len());
                        Some(payload
                            .results
                            .into_iter()
                            .map(|result| {
                                (
                                    walkers::lon_lat(result.longitude, result.latitude),
                                    result.elevation,
                                )
                            })
                            .collect::<Vec<_>>())
                    }
                    Err(e) => {
                        log::error!("Failed to parse elevation response: {}", e);
                        None
                    }
                }
            }
            Ok(response) => {
                log::warn!(
                    "Elevation lookup failed with status {} {}",
                    response.status,
                    response.status_text
                );
                None
            }
            Err(error) => {
                log::error!("Elevation lookup request failed: {error}");
                None
            }
        };

        if let Ok(mut lock) = results.lock() {
            *lock = elevations;
        }
    });
}
