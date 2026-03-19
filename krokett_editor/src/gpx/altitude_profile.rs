use super::*;

use egui_plot::{Line, Plot, PlotPoints};
use std::sync::{Arc, Mutex};

use crate::elevation_service::ElevationResults;

/// Calculates the great-circle distance between two waypoints in kilometers
fn distance_between_waypoints(wp1: &gpx::Waypoint, wp2: &gpx::Waypoint) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;

    let p1 = wp1.point();
    let p2 = wp2.point();

    let lat1 = p1.y().to_radians();
    let lat2 = p2.y().to_radians();
    let lon1 = p1.x().to_radians();
    let lon2 = p2.x().to_radians();

    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;

    let a = (dlat / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

    EARTH_RADIUS_KM * c
}

/// Extracts altitude profile data from a list of waypoints
/// Returns a vector of (distance_km, elevation_m) tuples
pub(super) fn extract_altitude_profile(waypoints: &[gpx::Waypoint]) -> Vec<(f64, f64)> {
    if waypoints.is_empty() {
        return Vec::new();
    }

    let mut profile = Vec::new();
    let mut cumulative_distance = 0.0;

    for (i, waypoint) in waypoints.iter().enumerate() {
        // Try to get elevation, skip if not available
        if let Some(elevation) = waypoint.elevation {
            profile.push((cumulative_distance, elevation));
        }

        // Calculate distance to next waypoint
        if i < waypoints.len() - 1 {
            cumulative_distance += distance_between_waypoints(waypoint, &waypoints[i + 1]);
        }
    }

    profile
}

fn format_altitude_tooltip(name: &str, dist_km: f64, alt_m: f64) -> String {
    if name.is_empty() {
        format!("dist: {:.1} km\nalt: {:.0} m", dist_km, alt_m)
    } else {
        format!("{name}\ndist: {:.0} km\nalt: {:.0} m", dist_km, alt_m)
    }
}

pub struct AltitudeProfileState {
    pub open: bool,
    pub selected_segment: Option<SegmentSelection>,
    elevation_results: ElevationResults,
    fetch_in_progress: bool,
    fetch_start_time: Option<std::time::Instant>,
}

impl AltitudeProfileState {
    pub fn new() -> Self {
        Self {
            open: false,
            selected_segment: None,
            elevation_results: Arc::new(Mutex::new(None)),
            fetch_in_progress: false,
            fetch_start_time: None,
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.selected_segment = None;
        self.elevation_results = Arc::new(Mutex::new(None));
        self.fetch_in_progress = false;
        self.fetch_start_time = None;
    }
}

pub struct TempAltitudeProfileState {
    pub open: bool,
    pub waypoints: Vec<gpx::Waypoint>,
    elevation_results: ElevationResults,
    fetch_in_progress: bool,
    fetch_start_time: Option<std::time::Instant>,
}

impl TempAltitudeProfileState {
    pub fn new() -> Self {
        Self {
            open: false,
            waypoints: Vec::new(),
            elevation_results: Arc::new(Mutex::new(None)),
            fetch_in_progress: false,
            fetch_start_time: None,
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.waypoints.clear();
        self.elevation_results = Arc::new(Mutex::new(None));
        self.fetch_in_progress = false;
        self.fetch_start_time = None;
    }

    pub fn reset_fetch(&mut self) {
        self.elevation_results = Arc::new(Mutex::new(None));
        self.fetch_in_progress = false;
        self.fetch_start_time = None;
    }
}

impl GpxState {
    pub(crate) fn show_altitude_profile_window(&mut self, ctx: &egui::Context) {
        let Some(segment_selection) = self.altitude_profile.selected_segment else {
            self.altitude_profile.open = false;
            return;
        };

        // Check if we need to start fetching elevation data
        if !self.altitude_profile.fetch_in_progress {
            if let Some(waypoints) = self.segment_waypoints(segment_selection) {
                let waypoints_without_elevation = waypoints
                    .iter()
                    .filter(|wp| wp.elevation.is_none())
                    .count();

                if waypoints_without_elevation > 0 {
                    // Start fetching elevation data
                    let positions: Vec<walkers::Position> = waypoints
                        .iter()
                        .map(|wp| {
                            let point = wp.point();
                            walkers::lat_lon(point.y(), point.x())
                        })
                        .collect();

                    if !positions.is_empty() {
                        log::info!("Fetching elevation for {} waypoints", positions.len());
                        crate::elevation_service::fetch_elevation_for_positions(
                            positions,
                            self.altitude_profile.elevation_results.clone(),
                        );
                        self.altitude_profile.fetch_in_progress = true;
                        self.altitude_profile.fetch_start_time = Some(std::time::Instant::now());
                    }
                }
            }
        }

        // Check if fetch has timed out (5 seconds)
        if self.altitude_profile.fetch_in_progress {
            if let Some(start_time) = self.altitude_profile.fetch_start_time {
                if start_time.elapsed().as_secs() > 5 {
                    log::warn!("Elevation fetch timed out");
                    self.altitude_profile.fetch_in_progress = false;
                    self.altitude_profile.fetch_start_time = None;
                }
            }
        }

        // Check if elevation data has been fetched and apply it
        let fetched_elevations = {
            let mut results = self.altitude_profile.elevation_results.lock().ok();
            results.as_mut().and_then(|r| r.take())
        };

        if let Some(elevations) = fetched_elevations {
            // Apply fetched elevations to waypoints
            if let Some(waypoints) = self.segment_waypoints_mut(segment_selection) {
                for waypoint in waypoints {
                    if waypoint.elevation.is_some() {
                        continue;
                    }

                    let point = waypoint.point();
                    let wp_pos = walkers::lat_lon(point.y(), point.x());

                    // Find matching elevation
                    for (fetched_pos, elevation) in &elevations {
                        if (wp_pos.x() - fetched_pos.x()).abs() < 1e-6
                            && (wp_pos.y() - fetched_pos.y()).abs() < 1e-6
                        {
                            waypoint.elevation = Some(*elevation);
                            break;
                        }
                    }
                }
            }
            self.altitude_profile.fetch_in_progress = false;
            self.altitude_profile.fetch_start_time = None;
        }

        // Now get waypoints to display
        let Some(waypoints) = self.segment_waypoints(segment_selection) else {
            self.altitude_profile.open = false;
            return;
        };

        let profile_data = extract_altitude_profile(waypoints);

        if profile_data.is_empty() {
            if self.altitude_profile.fetch_in_progress {
                // Still loading
                let mut open = self.altitude_profile.open;
                let window_title = format!("Profil d'altitude - Segment {}", segment_selection.1 + 1);

                egui::Window::new(window_title)
                    .open(&mut open)
                    .resizable(true)
                    .default_width(600.0)
                    .default_height(400.0)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.add_space(ui.available_width() / 2.0 - 50.0);
                            ui.label("⏳ Récupération des données d'altitude...");
                        });
                    });

                self.altitude_profile.open = open;
                if !self.altitude_profile.open {
                    self.altitude_profile.selected_segment = None;
                }
                return;
            } else {
                self.altitude_profile.open = false;
                return;
            }
        }

        let points: Vec<[f64; 2]> = profile_data.iter().map(|&(x, y)| [x, y]).collect();
        let line = Line::new("Profil d'altitude", PlotPoints::new(points)).fill(0.0);

        let mut open = self.altitude_profile.open;
        let window_title = format!("Profil d'altitude - Segment {}", segment_selection.1 + 1);

        egui::Window::new(window_title)
            .open(&mut open)
            .resizable(true)
            .default_width(600.0)
            .default_height(400.0)
            .show(ctx, |ui| {
                // Statistics panel
                let max_elevation = profile_data
                    .iter()
                    .map(|(_, elev)| *elev)
                    .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                    .unwrap_or(0.0);
                let min_elevation = profile_data
                    .iter()
                    .map(|(_, elev)| *elev)
                    .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                    .unwrap_or(0.0);
                let total_distance = profile_data.last().map(|(x, _)| *x).unwrap_or(0.0);

                // Calculate total elevation gain and loss
                let (mut climb, mut descent) = (0.0, 0.0);
                for window in profile_data.windows(2) {
                    let diff = window[1].1 - window[0].1;
                    if diff > 0.0 {
                        climb += diff;
                    } else {
                        descent -= diff;
                    }
                }

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "Distance: {:.2} km",
                                total_distance
                            ))
                            .strong(),
                        );
                        ui.label(format!("Élévation: {:.0}m - {:.0}m", min_elevation, max_elevation));
                        ui.label(format!("Montée: {:.0}m | Descente: {:.0}m", climb, descent));
                    });
                });

                ui.separator();

                // Draw the altitude profile plot
                Plot::new("altitude_profile_plot")
                    .label_formatter(|name, value| {
                        format_altitude_tooltip(name, value.x, value.y)
                    })
                    .view_aspect(2.0)
                    .show(ui, |plot_ui| {
                        plot_ui.line(line);
                    });
            });

        self.altitude_profile.open = open;
        if !self.altitude_profile.open {
            self.altitude_profile.selected_segment = None;
        }
    }
}

impl GpxState {
    pub(crate) fn show_temp_altitude_profile_window(&mut self, ctx: &egui::Context) {
        if !self.temp_altitude_profile.open {
            return;
        }

        // Start elevation fetch for waypoints that have no elevation
        if !self.temp_altitude_profile.fetch_in_progress {
            let needs_elevation = self
                .temp_altitude_profile
                .waypoints
                .iter()
                .any(|wp| wp.elevation.is_none());

            if needs_elevation {
                let positions: Vec<walkers::Position> = self
                    .temp_altitude_profile
                    .waypoints
                    .iter()
                    .filter(|wp| wp.elevation.is_none())
                    .map(|wp| {
                        let point = wp.point();
                        walkers::lat_lon(point.y(), point.x())
                    })
                    .collect();

                if !positions.is_empty() {
                    log::info!(
                        "Fetching elevation for {} temp segment waypoints",
                        positions.len()
                    );
                    crate::elevation_service::fetch_elevation_for_positions(
                        positions,
                        self.temp_altitude_profile.elevation_results.clone(),
                    );
                    self.temp_altitude_profile.fetch_in_progress = true;
                    self.temp_altitude_profile.fetch_start_time =
                        Some(std::time::Instant::now());
                }
            }
        }

        // Check fetch timeout (10 seconds)
        if self.temp_altitude_profile.fetch_in_progress {
            if let Some(start_time) = self.temp_altitude_profile.fetch_start_time {
                if start_time.elapsed().as_secs() > 10 {
                    log::warn!("Temp segment elevation fetch timed out");
                    self.temp_altitude_profile.fetch_in_progress = false;
                    self.temp_altitude_profile.fetch_start_time = None;
                }
            }
        }

        // Apply fetched elevations
        let fetched_elevations = {
            let mut results = self.temp_altitude_profile.elevation_results.lock().ok();
            results.as_mut().and_then(|r| r.take())
        };
        if let Some(elevations) = fetched_elevations {
            for waypoint in &mut self.temp_altitude_profile.waypoints {
                if waypoint.elevation.is_some() {
                    continue;
                }
                let point = waypoint.point();
                let wp_pos = walkers::lat_lon(point.y(), point.x());
                for (fetched_pos, elevation) in &elevations {
                    if (wp_pos.x() - fetched_pos.x()).abs() < 1e-6
                        && (wp_pos.y() - fetched_pos.y()).abs() < 1e-6
                    {
                        waypoint.elevation = Some(*elevation);
                        break;
                    }
                }
            }
            self.temp_altitude_profile.fetch_in_progress = false;
            self.temp_altitude_profile.fetch_start_time = None;
        }

        let profile_data = extract_altitude_profile(&self.temp_altitude_profile.waypoints);
        let fetch_in_progress = self.temp_altitude_profile.fetch_in_progress;
        let mut open = self.temp_altitude_profile.open;

        if profile_data.is_empty() {
            egui::Window::new("Profil d'altitude - Segment temporaire")
                .open(&mut open)
                .resizable(true)
                .default_width(600.0)
                .default_height(200.0)
                .show(ctx, |ui| {
                    if fetch_in_progress {
                        ui.label("⏳ Récupération des données d'altitude...");
                    } else {
                        ui.label("Aucune donnée d'altitude disponible");
                    }
                });
            self.temp_altitude_profile.open = open;
            return;
        }

        let points: Vec<[f64; 2]> = profile_data.iter().map(|&(x, y)| [x, y]).collect();
        let line = Line::new("Profil", PlotPoints::new(points)).fill(0.0);

        egui::Window::new("Profil d'altitude - Segment temporaire")
            .open(&mut open)
            .resizable(true)
            .default_width(600.0)
            .default_height(400.0)
            .show(ctx, |ui| {
                let max_elevation = profile_data
                    .iter()
                    .map(|(_, e)| *e)
                    .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                    .unwrap_or(0.0);
                let min_elevation = profile_data
                    .iter()
                    .map(|(_, e)| *e)
                    .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                    .unwrap_or(0.0);
                let total_distance = profile_data.last().map(|(x, _)| *x).unwrap_or(0.0);

                let (mut climb, mut descent) = (0.0f64, 0.0f64);
                for w in profile_data.windows(2) {
                    let diff = w[1].1 - w[0].1;
                    if diff > 0.0 {
                        climb += diff;
                    } else {
                        descent -= diff;
                    }
                }

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "Distance: {:.2} km",
                                total_distance
                            ))
                            .strong(),
                        );
                        ui.label(format!(
                            "Élévation: {:.0}m - {:.0}m",
                            min_elevation, max_elevation
                        ));
                        ui.label(format!(
                            "Montée: {:.0}m | Descente: {:.0}m",
                            climb, descent
                        ));
                    });
                });

                ui.separator();

                Plot::new("temp_altitude_profile_plot")
                    .label_formatter(|name, value| {
                        format_altitude_tooltip(name, value.x, value.y)
                    })
                    .view_aspect(2.0)
                    .show(ui, |plot_ui| {
                        plot_ui.line(line);
                    });
            });

        self.temp_altitude_profile.open = open;
    }
}
