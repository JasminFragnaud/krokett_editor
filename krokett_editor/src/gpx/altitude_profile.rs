use super::*;

use egui_plot::{Line, Plot, PlotPoints};

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

    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
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
        format!("dist: {dist_km:.1} km\nalt: {alt_m:.0} m")
    } else {
        format!("{name}\ndist: {dist_km:.0} km\nalt: {alt_m:.0} m")
    }
}

fn waypoint_positions_missing_elevation(waypoints: &[gpx::Waypoint]) -> Vec<walkers::Position> {
    waypoints
        .iter()
        .filter(|wp| wp.elevation.is_none())
        .map(|wp| {
            let point = wp.point();
            walkers::lat_lon(point.y(), point.x())
        })
        .collect()
}

fn start_elevation_fetch(
    ctx: &egui::Context,
    title: &str,
    waypoints: &[gpx::Waypoint],
    sender: std::sync::mpsc::Sender<Vec<(walkers::Position, f64)>>,
) -> Option<f64> {
    let positions = waypoint_positions_missing_elevation(waypoints);
    if positions.is_empty() {
        return None;
    }

    log::info!(
        "Fetching elevation for {} waypoints ({title})",
        positions.len()
    );
    crate::elevation_service::fetch_elevation_for_positions(positions, sender);
    Some(ctx.input(|i| i.time))
}

fn update_fetch_timeout(
    ctx: &egui::Context,
    title: &str,
    fetch_in_progress: &mut bool,
    fetch_start_time: &mut Option<f64>,
    fetch_timed_out: &mut bool,
) {
    if !*fetch_in_progress {
        return;
    }

    ctx.request_repaint_after(std::time::Duration::from_millis(100));
    if let Some(start_time) = *fetch_start_time {
        if ctx.input(|i| i.time) - start_time > 30.0 {
            log::warn!("Elevation fetch timed out ({title})");
            *fetch_in_progress = false;
            *fetch_start_time = None;
            *fetch_timed_out = true;
        }
    }
}

fn take_fetched_elevation_lookup(
    elevation_results: &ElevationResults,
) -> Option<std::collections::HashMap<(u64, u64), f64>> {
    let elevations = elevation_results.1.try_recv().ok()?;

    Some(
        elevations
            .iter()
            .map(|(pos, elevation)| ((pos.x().to_bits(), pos.y().to_bits()), *elevation))
            .collect(),
    )
}

fn apply_elevation_lookup(
    elevation_by_pos: &std::collections::HashMap<(u64, u64), f64>,
    waypoints: &mut [gpx::Waypoint],
) {
    for waypoint in waypoints {
        if waypoint.elevation.is_some() {
            continue;
        }

        let point = waypoint.point();
        let wp_pos = walkers::lat_lon(point.y(), point.x());
        waypoint.elevation = elevation_by_pos
            .get(&(wp_pos.x().to_bits(), wp_pos.y().to_bits()))
            .copied();
    }
}

pub struct AltitudeProfileState {
    pub open: bool,
    pub selected_segment: Option<SegmentSelection>,
    elevation_results: ElevationResults,
    fetch_in_progress: bool,
    fetch_attempted: bool,
    fetch_start_time: Option<f64>,
    fetch_timed_out: bool,
}

impl AltitudeProfileState {
    pub fn new() -> Self {
        Self {
            open: false,
            selected_segment: None,
            elevation_results: std::sync::mpsc::channel(),
            fetch_in_progress: false,
            fetch_attempted: false,
            fetch_start_time: None,
            fetch_timed_out: false,
        }
    }

    pub fn reset_fetch(&mut self) {
        self.elevation_results = std::sync::mpsc::channel();
        self.fetch_in_progress = false;
        self.fetch_attempted = false;
        self.fetch_start_time = None;
        self.fetch_timed_out = false;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.selected_segment = None;
        self.reset_fetch();
    }
}

pub struct TempAltitudeProfileState {
    pub open: bool,
    pub title: String,
    pub waypoints: Vec<gpx::Waypoint>,
    elevation_results: ElevationResults,
    fetch_in_progress: bool,
    fetch_attempted: bool,
    fetch_start_time: Option<f64>,
    fetch_timed_out: bool,
}

impl TempAltitudeProfileState {
    pub fn new() -> Self {
        Self {
            open: false,
            title: "Profil d'altitude - Segment temporaire".to_owned(),
            waypoints: Vec::new(),
            elevation_results: std::sync::mpsc::channel(),
            fetch_in_progress: false,
            fetch_attempted: false,
            fetch_start_time: None,
            fetch_timed_out: false,
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.title = "Profil d'altitude - Segment temporaire".to_owned();
        self.waypoints.clear();
        self.reset_fetch();
    }

    pub fn reset_fetch(&mut self) {
        self.elevation_results = std::sync::mpsc::channel();
        self.fetch_in_progress = false;
        self.fetch_attempted = false;
        self.fetch_start_time = None;
        self.fetch_timed_out = false;
    }
}

impl GpxState {
    pub(crate) fn open_temp_altitude_profile(
        &mut self,
        title: impl Into<String>,
        waypoints: Vec<gpx::Waypoint>,
    ) {
        self.temp_altitude_profile.open = true;
        self.temp_altitude_profile.title = title.into();
        self.temp_altitude_profile.waypoints = waypoints;
        self.temp_altitude_profile.reset_fetch();
    }

    pub(crate) fn show_altitude_profile_window(&mut self, ctx: &egui::Context) {
        let Some(segment_selection) = self.altitude_profile.selected_segment else {
            self.altitude_profile.open = false;
            return;
        };

        if !self.altitude_profile.fetch_in_progress && !self.altitude_profile.fetch_attempted {
            if let Some(waypoints) = self.segment_waypoints(segment_selection) {
                if let Some(start_time) = start_elevation_fetch(
                    ctx,
                    "Profil d'altitude",
                    waypoints,
                    self.altitude_profile.elevation_results.0.clone(),
                ) {
                    self.altitude_profile.fetch_in_progress = true;
                    self.altitude_profile.fetch_attempted = true;
                    self.altitude_profile.fetch_start_time = Some(start_time);
                    self.altitude_profile.fetch_timed_out = false;
                }
            }
        }

        update_fetch_timeout(
            ctx,
            "Profil d'altitude",
            &mut self.altitude_profile.fetch_in_progress,
            &mut self.altitude_profile.fetch_start_time,
            &mut self.altitude_profile.fetch_timed_out,
        );

        if let Some(elevation_by_pos) =
            take_fetched_elevation_lookup(&self.altitude_profile.elevation_results)
        {
            if let Some(waypoints) = self.segment_waypoints_mut(segment_selection) {
                apply_elevation_lookup(&elevation_by_pos, waypoints);
            }
            self.altitude_profile.fetch_in_progress = false;
            self.altitude_profile.fetch_start_time = None;
            self.altitude_profile.fetch_timed_out = false;
        }

        // Now get waypoints to display
        let Some(waypoints) = self.segment_waypoints(segment_selection) else {
            self.altitude_profile.open = false;
            return;
        };

        let profile_data = extract_altitude_profile(waypoints);

        if profile_data.is_empty() {
            let mut open = self.altitude_profile.open;
            let window_title = format!("Profil d'altitude - Segment {}", segment_selection.1 + 1);

            egui::Window::new(window_title)
                .open(&mut open)
                .resizable(true)
                .default_width(600.0)
                .default_height(260.0)
                .show(ctx, |ui| {
                    if self.altitude_profile.fetch_in_progress {
                        ui.label("⏳ Récupération des données d'altitude...");
                    } else if self.altitude_profile.fetch_timed_out {
                        ui.label("Le chargement a expiré.");
                    } else if self.altitude_profile.fetch_attempted {
                        ui.label("Aucune donnée d'altitude disponible");
                    } else {
                        ui.label("Prêt à charger le profil d'altitude");
                    }
                });

            self.altitude_profile.open = open;
            if !self.altitude_profile.open {
                self.altitude_profile.selected_segment = None;
            }
            return;
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
                let slope_percent = if total_distance > 0.0 {
                    (climb / (total_distance * 1000.0)) * 100.0
                } else {
                    0.0
                };

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(format!("Distance: {total_distance:.2} km"))
                                .strong(),
                        );
                        ui.label(format!(
                            "Élévation: {min_elevation:.0}m - {max_elevation:.0}m"
                        ));
                        ui.label(format!("Montée: {climb:.0}m | Descente: {descent:.0}m"));
                        ui.label(format!("Pourcentage de pente: {slope_percent:.1}%"));
                    });
                });

                ui.separator();

                // Draw the altitude profile plot
                Plot::new("altitude_profile_plot")
                    .label_formatter(|name, value| format_altitude_tooltip(name, value.x, value.y))
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

        if !self.temp_altitude_profile.fetch_in_progress
            && !self.temp_altitude_profile.fetch_attempted
        {
            if let Some(start_time) = start_elevation_fetch(
                ctx,
                &self.temp_altitude_profile.title,
                &self.temp_altitude_profile.waypoints,
                self.temp_altitude_profile.elevation_results.0.clone(),
            ) {
                self.temp_altitude_profile.fetch_in_progress = true;
                self.temp_altitude_profile.fetch_attempted = true;
                self.temp_altitude_profile.fetch_start_time = Some(start_time);
                self.temp_altitude_profile.fetch_timed_out = false;
            }
        }

        update_fetch_timeout(
            ctx,
            &self.temp_altitude_profile.title,
            &mut self.temp_altitude_profile.fetch_in_progress,
            &mut self.temp_altitude_profile.fetch_start_time,
            &mut self.temp_altitude_profile.fetch_timed_out,
        );

        if let Some(elevation_by_pos) =
            take_fetched_elevation_lookup(&self.temp_altitude_profile.elevation_results)
        {
            apply_elevation_lookup(&elevation_by_pos, &mut self.temp_altitude_profile.waypoints);
            self.temp_altitude_profile.fetch_in_progress = false;
            self.temp_altitude_profile.fetch_start_time = None;
            self.temp_altitude_profile.fetch_timed_out = false;
        }

        let profile_data = extract_altitude_profile(&self.temp_altitude_profile.waypoints);
        let fetch_in_progress = self.temp_altitude_profile.fetch_in_progress;
        let mut open = self.temp_altitude_profile.open;
        let window_title = self.temp_altitude_profile.title.clone();

        if profile_data.is_empty() {
            egui::Window::new(&window_title)
                .open(&mut open)
                .resizable(true)
                .default_width(600.0)
                .default_height(200.0)
                .show(ctx, |ui| {
                    if fetch_in_progress {
                        ui.label("⏳ Récupération des données d'altitude...");
                    } else if self.temp_altitude_profile.fetch_timed_out {
                        ui.label("Le chargement a expiré.");
                    } else {
                        ui.label("Aucune donnée d'altitude disponible");
                    }
                });
            self.temp_altitude_profile.open = open;
            return;
        }

        let points: Vec<[f64; 2]> = profile_data.iter().map(|&(x, y)| [x, y]).collect();
        let line = Line::new("Profil", PlotPoints::new(points)).fill(0.0);

        egui::Window::new(&window_title)
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
                let slope_percent = if total_distance > 0.0 {
                    (climb - descent) / (total_distance * 1000.0) * 100.0
                } else {
                    0.0
                };

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(format!("Distance: {total_distance:.2} km"));
                        ui.label(format!(
                            "Élévation: {min_elevation:.0}m - {max_elevation:.0}m"
                        ));
                        ui.label(format!("Montée: {climb:.0}m | Descente: {descent:.0}m"));
                        ui.label(format!("Pourcentage de pente: {slope_percent:.1}%"));
                    });
                });

                ui.separator();

                Plot::new("temp_altitude_profile_plot")
                    .label_formatter(|name, value| format_altitude_tooltip(name, value.x, value.y))
                    .view_aspect(2.0)
                    .show(ui, |plot_ui| {
                        plot_ui.line(line);
                    });
            });

        self.temp_altitude_profile.open = open;
    }
}
