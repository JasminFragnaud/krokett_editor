use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Color32, PointerButton, Pos2};
use egui_ltreeview::{NodeBuilder, TreeView};
use itertools::Itertools as _;
use walkers::{Map, MapMemory, Plugin};

#[derive(Clone)]
struct GpxSegment {
    waypoints: Vec<gpx::Waypoint>,
    positions: Vec<walkers::Position>,
    description: String,
    visible: bool,
}

#[derive(Clone)]
struct GpxTrack {
    source: String,
    name: String,
    description: String,
    comment: Option<String>,
    data_source: Option<String>,
    links: Vec<gpx::Link>,
    type_: Option<String>,
    number: Option<u32>,
    original_kind: GpxTrackKind,
    segments: Vec<GpxSegment>,
    visible: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GpxTrackKind {
    Track,
    Route,
}

#[derive(Clone, Copy)]
struct GpxBounds {
    min_lat: f64,
    max_lat: f64,
    min_lon: f64,
    max_lon: f64,
}

impl GpxBounds {
    fn from_position(position: walkers::Position) -> Self {
        Self {
            min_lat: position.y(),
            max_lat: position.y(),
            min_lon: position.x(),
            max_lon: position.x(),
        }
    }

    fn include_position(&mut self, position: walkers::Position) {
        self.min_lat = self.min_lat.min(position.y());
        self.max_lat = self.max_lat.max(position.y());
        self.min_lon = self.min_lon.min(position.x());
        self.max_lon = self.max_lon.max(position.x());
    }

    fn merge(&mut self, other: Self) {
        self.min_lat = self.min_lat.min(other.min_lat);
        self.max_lat = self.max_lat.max(other.max_lat);
        self.min_lon = self.min_lon.min(other.min_lon);
        self.max_lon = self.max_lon.max(other.max_lon);
    }

    fn center(&self) -> walkers::Position {
        walkers::lat_lon(
            (self.min_lat + self.max_lat) * 0.5,
            (self.min_lon + self.max_lon) * 0.5,
        )
    }
}

type SegmentSelection = (usize, usize);
type CutRequest = (usize, usize, usize);
type MergeRequest = (usize, usize);
type ClickedTrack = Arc<Mutex<Option<usize>>>;
type ClickedSegment = Arc<Mutex<Option<SegmentSelection>>>;
type PendingCutRequest = Arc<Mutex<Option<CutRequest>>>;
type PendingMergeRequest = Arc<Mutex<Option<MergeRequest>>>;
type AddPluginsOutput<'a, 'b, 'c> = (
    Map<'a, 'b, 'c>,
    ClickedTrack,
    ClickedSegment,
    PendingCutRequest,
    PendingMergeRequest,
);

#[derive(Clone, PartialEq, Eq, Hash)]
enum GpxTreeNodeId {
    File(String),
    Track(usize),
    Segment(usize, usize),
}

pub(crate) struct GpxState {
    tracks: Vec<GpxTrack>,
    status: Option<String>,
    pending_gpx_fit: Option<GpxBounds>,
    auto_fit_enabled: bool,
    toast_message: Option<String>,
    toast_until: Option<Instant>,
    metadata_editor_open: bool,
    selected_track_index: Option<usize>,
    segment_editor_open: bool,
    selected_segment: Option<SegmentSelection>,
    window_highlight_segment: Option<SegmentSelection>,
    tree_window_visible: bool,
    tree_hover_track: Option<usize>,
    tree_hover_segment: Option<SegmentSelection>,
    cut_tool_enabled: bool,
}

impl GpxState {
    pub(crate) fn new() -> Self {
        Self {
            tracks: Vec::new(),
            status: None,
            pending_gpx_fit: None,
            auto_fit_enabled: true,
            toast_message: None,
            toast_until: None,
            metadata_editor_open: false,
            selected_track_index: None,
            segment_editor_open: false,
            selected_segment: None,
            window_highlight_segment: None,
            tree_window_visible: true,
            tree_hover_track: None,
            tree_hover_segment: None,
            cut_tool_enabled: false,
        }
    }

    pub(crate) fn tracks_count(&self) -> usize {
        self.tracks.iter().map(|track| track.segments.len()).sum()
    }

    pub(crate) fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub(crate) fn clear(&mut self) {
        self.tracks.clear();
        self.status = Some("GPX overlays cleared".to_owned());
        self.pending_gpx_fit = None;
        self.metadata_editor_open = false;
        self.selected_track_index = None;
        self.segment_editor_open = false;
        self.selected_segment = None;
        self.window_highlight_segment = None;
        self.tree_hover_track = None;
        self.tree_hover_segment = None;
    }

    pub(crate) fn show_toast(&mut self, ctx: &egui::Context) {
        let Some(until) = self.toast_until else {
            return;
        };

        if Instant::now() > until {
            self.toast_until = None;
            self.toast_message = None;
            return;
        }

        let Some(message) = self.toast_message.as_ref() else {
            return;
        };

        egui::Area::new("gpx_toast".into())
            .anchor(egui::Align2::RIGHT_BOTTOM, [-12.0, -12.0])
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(message);
                });
            });

        ctx.request_repaint_after(Duration::from_millis(100));
    }

    pub(crate) fn auto_fit_enabled(&self) -> bool {
        self.auto_fit_enabled
    }

    pub(crate) fn set_auto_fit_enabled(&mut self, enabled: bool) {
        self.auto_fit_enabled = enabled;
    }

    pub(crate) fn cut_tool_enabled(&self) -> bool {
        self.cut_tool_enabled
    }

    pub(crate) fn set_cut_tool_enabled(&mut self, enabled: bool) {
        self.cut_tool_enabled = enabled;
    }

    pub(crate) fn tree_window_visible(&self) -> bool {
        self.tree_window_visible
    }

    pub(crate) fn set_tree_window_visible(&mut self, visible: bool) {
        self.tree_window_visible = visible;
        if !visible {
            self.tree_hover_track = None;
            self.tree_hover_segment = None;
        }
    }

    pub(crate) fn show_tree_window(&mut self, ctx: &egui::Context) {
        self.tree_hover_track = None;
        self.tree_hover_segment = None;

        if !self.tree_window_visible {
            return;
        }

        let mut open = self.tree_window_visible;
        let default_pos = ctx.available_rect().left_top() + egui::vec2(10., 200.0);
        egui::Window::new("GPX Tree")
            .open(&mut open)
            .resizable(true)
            .default_pos(default_pos)
            .show(ctx, |ui| {
                if self.tracks.is_empty() {
                    ui.label("No GPX loaded");
                    return;
                }

                let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
                for (track_index, track) in self.tracks.iter().enumerate() {
                    groups
                        .entry(track.source.clone())
                        .or_default()
                        .push(track_index);
                }

                let mut hover_track = None;
                let mut hover_segment = None;
                let mut click_track = None;
                let mut click_segment = None;

                let mut file_visibility_updates: Vec<(Vec<usize>, bool)> = Vec::new();
                let mut track_visibility_updates: Vec<(usize, bool)> = Vec::new();
                let mut segment_visibility_updates: Vec<((usize, usize), bool)> = Vec::new();

                let tree_id = ui.make_persistent_id("gpx_tree_view");
                let (_response, _actions) = TreeView::new(tree_id).show(ui, |builder| {
                    for (source, track_indices) in &groups {
                        let mut file_visible = true;
                        for &track_index in track_indices {
                            let track = &self.tracks[track_index];
                            if !track.visible
                                || track.segments.iter().any(|segment| !segment.visible)
                            {
                                file_visible = false;
                                break;
                            }
                        }

                        let file_label = format!("File: {source}");
                        let file_is_open = builder.node(
                            NodeBuilder::dir(GpxTreeNodeId::File(source.clone())).label_ui(|ui| {
                                let row = ui.horizontal(|ui| {
                                    let checkbox_response = ui.checkbox(&mut file_visible, "");
                                    if checkbox_response.changed() {
                                        file_visibility_updates
                                            .push((track_indices.clone(), file_visible));
                                    }
                                    let label_response = ui.label(&file_label);
                                    checkbox_response.hovered() || label_response.hovered()
                                });

                                let mut full_line_rect = row.response.rect;
                                full_line_rect.set_left(ui.min_rect().left());
                                full_line_rect.set_right(ui.max_rect().right());
                                let row_hovered = row
                                    .response
                                    .ctx
                                    .pointer_hover_pos()
                                    .map(|pointer| full_line_rect.contains(pointer))
                                    .unwrap_or(false);

                                if row.inner || row_hovered {
                                    for &track_index in track_indices {
                                        hover_track = Some(track_index);
                                    }
                                }
                            }),
                        );

                        if file_is_open {
                            for &track_index in track_indices {
                                let track = &self.tracks[track_index];
                                let mut track_visible = track.visible;
                                let track_title = if track.name.trim().is_empty() {
                                    format!("Track {}", track_index + 1)
                                } else {
                                    track.name.clone()
                                };

                                let track_is_open = builder.node(
                                    NodeBuilder::dir(GpxTreeNodeId::Track(track_index)).label_ui(
                                        |ui| {
                                            let row = ui.horizontal(|ui| {
                                                let checkbox_response =
                                                    ui.checkbox(&mut track_visible, "");
                                                if checkbox_response.changed() {
                                                    track_visibility_updates
                                                        .push((track_index, track_visible));
                                                }
                                                let label_response = ui.label(&track_title);
                                                (
                                                    checkbox_response.hovered()
                                                        || label_response.hovered(),
                                                    checkbox_response.clicked()
                                                        || label_response.clicked(),
                                                )
                                            });
                                            let mut full_line_rect = row.response.rect;
                                            full_line_rect.set_left(ui.min_rect().left());
                                            full_line_rect.set_right(ui.max_rect().right());
                                            let row_hovered = row
                                                .response
                                                .ctx
                                                .pointer_hover_pos()
                                                .map(|pointer| full_line_rect.contains(pointer))
                                                .unwrap_or(false);
                                            let row_clicked = row.response.ctx.input(|input| {
                                                input.pointer.primary_clicked()
                                                    && input.pointer.interact_pos().is_some_and(
                                                        |pointer| full_line_rect.contains(pointer),
                                                    )
                                            });

                                            if row.inner.0 || row_hovered {
                                                hover_track = Some(track_index);
                                            }
                                            if row.inner.1 || row_clicked {
                                                click_track = Some(track_index);
                                            }
                                        },
                                    ),
                                );

                                if track_is_open {
                                    for (segment_index, segment) in
                                        track.segments.iter().enumerate()
                                    {
                                        let mut segment_visible = segment.visible;
                                        let segment_label = format!(
                                            "{}: {}",
                                            segment_index + 1,
                                            segment.description
                                        );

                                        builder.node(
                                            NodeBuilder::leaf(GpxTreeNodeId::Segment(
                                                track_index,
                                                segment_index,
                                            ))
                                            .label_ui(|ui| {
                                                let row = ui.horizontal(|ui| {
                                                    let checkbox_response =
                                                        ui.checkbox(&mut segment_visible, "");
                                                    if checkbox_response.changed() {
                                                        segment_visibility_updates.push((
                                                            (track_index, segment_index),
                                                            segment_visible,
                                                        ));
                                                    }
                                                    let label_response = ui.label(&segment_label);
                                                    (
                                                        checkbox_response.hovered()
                                                            || label_response.hovered(),
                                                        checkbox_response.clicked()
                                                            || label_response.clicked(),
                                                    )
                                                });
                                                let mut full_line_rect = row.response.rect;
                                                full_line_rect.set_left(ui.min_rect().left());
                                                full_line_rect.set_right(ui.max_rect().right());
                                                let row_hovered = row
                                                    .response
                                                    .ctx
                                                    .pointer_hover_pos()
                                                    .map(|pointer| full_line_rect.contains(pointer))
                                                    .unwrap_or(false);
                                                let row_clicked = row.response.ctx.input(|input| {
                                                    input.pointer.primary_clicked()
                                                        && input.pointer.interact_pos().is_some_and(
                                                            |pointer| {
                                                                full_line_rect.contains(pointer)
                                                            },
                                                        )
                                                });

                                                if row.inner.0 || row_hovered {
                                                    hover_segment =
                                                        Some((track_index, segment_index));
                                                }
                                                if row.inner.1 || row_clicked {
                                                    click_segment =
                                                        Some((track_index, segment_index));
                                                }
                                            }),
                                        );
                                    }
                                }

                                builder.close_dir();
                            }
                        }

                        builder.close_dir();
                    }
                });

                for (track_indices, visible) in file_visibility_updates {
                    for track_index in track_indices {
                        if let Some(track) = self.tracks.get_mut(track_index) {
                            track.visible = visible;
                            for segment in &mut track.segments {
                                segment.visible = visible;
                            }
                        }
                    }
                }

                for (track_index, visible) in track_visibility_updates {
                    if let Some(track) = self.tracks.get_mut(track_index) {
                        track.visible = visible;
                        for segment in &mut track.segments {
                            segment.visible = visible;
                        }
                    }
                }

                for ((track_index, segment_index), visible) in segment_visibility_updates {
                    if let Some(track) = self.tracks.get_mut(track_index) {
                        if let Some(segment) = track.segments.get_mut(segment_index) {
                            segment.visible = visible;
                        }
                    }
                }

                if let Some(track_index) = click_track {
                    self.selected_track_index = Some(track_index);
                    self.metadata_editor_open = true;
                }

                if let Some((track_index, segment_index)) = click_segment {
                    self.selected_segment = Some((track_index, segment_index));
                    self.segment_editor_open = true;
                }

                self.tree_hover_track = hover_track;
                self.tree_hover_segment = hover_segment;
            });

        self.tree_window_visible = open;
        if !self.tree_window_visible {
            self.tree_hover_track = None;
            self.tree_hover_segment = None;
        }
    }

    pub(crate) fn show_metadata_editor_window(&mut self, ctx: &egui::Context) {
        let Some(track_index) = self.selected_track_index else {
            return;
        };
        if track_index >= self.tracks.len() {
            self.metadata_editor_open = false;
            self.selected_track_index = None;
            return;
        }

        let mut open = self.metadata_editor_open;
        let track = &mut self.tracks[track_index];

        egui::Window::new("Track metadata")
            .open(&mut open)
            .resizable(true)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.label(format!("Source: {}", track.source));
                ui.separator();
                ui.label("Name");
                ui.text_edit_singleline(&mut track.name);
                ui.label("Description");
                ui.text_edit_multiline(&mut track.description);
            });

        self.metadata_editor_open = open;
        if !self.metadata_editor_open {
            self.selected_track_index = None;
        }
    }

    pub(crate) fn show_segment_editor_window(&mut self, ctx: &egui::Context) {
        let Some((track_index, segment_index)) = self.selected_segment else {
            self.window_highlight_segment = None;
            return;
        };

        if track_index >= self.tracks.len()
            || segment_index >= self.tracks[track_index].segments.len()
        {
            self.segment_editor_open = false;
            self.selected_segment = None;
            self.window_highlight_segment = None;
            return;
        }

        let mut open = self.segment_editor_open;
        let mut go_previous = false;
        let mut go_next = false;

        let track_name = self.tracks[track_index].name.clone();
        let segment_count = self.tracks[track_index].segments.len();
        let segment = &mut self.tracks[track_index].segments[segment_index];

        let response = egui::Window::new("Segment metadata")
            .open(&mut open)
            .resizable(true)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.label(format!("Track: {track_name}"));
                ui.horizontal(|ui| {
                    let prev_enabled = segment_index > 0;
                    let next_enabled = segment_index + 1 < segment_count;

                    if ui
                        .add_enabled(prev_enabled, egui::Button::new("-"))
                        .clicked()
                    {
                        go_previous = true;
                    }

                    ui.label(format!(
                        "Segment: {} / {}",
                        segment_index + 1,
                        segment_count
                    ));

                    if ui
                        .add_enabled(next_enabled, egui::Button::new("+"))
                        .clicked()
                    {
                        go_next = true;
                    }
                });
                ui.separator();
                ui.label("Description");
                ui.text_edit_multiline(&mut segment.description);
            });

        let window_hovered = response
            .as_ref()
            .and_then(|r| {
                ctx.pointer_hover_pos()
                    .map(|pointer| r.response.rect.contains(pointer))
            })
            .unwrap_or(false);

        self.window_highlight_segment = if window_hovered {
            Some((track_index, segment_index))
        } else {
            None
        };

        if go_previous {
            self.selected_segment = Some((track_index, segment_index - 1));
        } else if go_next {
            self.selected_segment = Some((track_index, segment_index + 1));
        }

        self.segment_editor_open = open;
        if !self.segment_editor_open {
            self.selected_segment = None;
            self.window_highlight_segment = None;
        }
    }

    fn fit_zoom_for_bounds(bounds: GpxBounds, viewport_size: egui::Vec2) -> f64 {
        let width = (viewport_size.x as f64 - 80.0).max(64.0);
        let height = (viewport_size.y as f64 - 80.0).max(64.0);

        let lon_span = (bounds.max_lon - bounds.min_lon).abs().max(1e-9);
        let x_fraction = (lon_span / 360.0).clamp(1e-9, 1.0);

        let project_lat = |lat: f64| {
            let clamped = lat.clamp(-85.051_128_78, 85.051_128_78);
            let rad = clamped.to_radians();
            let mercator = (rad.tan() + 1.0 / rad.cos()).ln();
            (1.0 - mercator / std::f64::consts::PI) * 0.5
        };

        let y1 = project_lat(bounds.min_lat);
        let y2 = project_lat(bounds.max_lat);
        let y_fraction = (y2 - y1).abs().max(1e-9);

        let zoom_x = (width / (256.0 * x_fraction)).log2();
        let zoom_y = (height / (256.0 * y_fraction)).log2();

        zoom_x.min(zoom_y).clamp(0.0, 26.0)
    }

    fn fit_map_to_bounds(
        &mut self,
        bounds: GpxBounds,
        viewport_size: egui::Vec2,
        map_memory: &mut MapMemory,
    ) {
        map_memory.center_at(bounds.center());
        let zoom = Self::fit_zoom_for_bounds(bounds, viewport_size);
        let _ = map_memory.set_zoom(zoom);
    }

    fn import_segment(
        segment_waypoints: Vec<gpx::Waypoint>,
        segment_description: String,
        bounds: &mut Option<GpxBounds>,
    ) -> Option<GpxSegment> {
        let segment_positions: Vec<_> = segment_waypoints
            .iter()
            .map(|waypoint| {
                let point = waypoint.point();
                walkers::lat_lon(point.y(), point.x())
            })
            .collect();

        if segment_positions.len() <= 1 {
            return None;
        }

        let mut segment_bounds = GpxBounds::from_position(segment_positions[0]);
        for position in &segment_positions {
            segment_bounds.include_position(*position);
        }

        if let Some(existing) = bounds.as_mut() {
            existing.merge(segment_bounds);
        } else {
            *bounds = Some(segment_bounds);
        }

        Some(GpxSegment {
            waypoints: segment_waypoints,
            positions: segment_positions,
            description: segment_description,
            visible: true,
        })
    }

    fn load_gpx_from_bytes(
        &mut self,
        file_name: &str,
        bytes: &[u8],
    ) -> Result<(usize, Option<GpxBounds>), String> {
        let gpx = gpx::read(Cursor::new(bytes))
            .map_err(|err| format!("Could not parse {file_name}: {err}"))?;

        let mut imported_segments = 0;
        let mut imported_bounds: Option<GpxBounds> = None;

        for track in gpx.tracks {
            let mut segments = Vec::new();
            for segment in track.segments {
                let segment_description = segment
                    .points
                    .first()
                    .and_then(|waypoint| waypoint.description.clone())
                    .unwrap_or_default();

                if let Some(imported_segment) =
                    Self::import_segment(segment.points, segment_description, &mut imported_bounds)
                {
                    segments.push(imported_segment);
                    imported_segments += 1;
                }
            }

            if !segments.is_empty() {
                self.tracks.push(GpxTrack {
                    source: file_name.to_owned(),
                    name: track.name.clone().unwrap_or_else(|| file_name.to_owned()),
                    description: track.description.clone().unwrap_or_default(),
                    comment: track.comment.clone(),
                    data_source: track.source.clone(),
                    links: track.links.clone(),
                    type_: track.type_.clone(),
                    number: track.number,
                    original_kind: GpxTrackKind::Track,
                    segments,
                    visible: true,
                });
            }
        }

        for route in gpx.routes {
            if let Some(imported_segment) = Self::import_segment(
                route.points,
                route.description.clone().unwrap_or_default(),
                &mut imported_bounds,
            ) {
                imported_segments += 1;
                self.tracks.push(GpxTrack {
                    source: file_name.to_owned(),
                    name: route.name.clone().unwrap_or_else(|| file_name.to_owned()),
                    description: String::new(),
                    comment: route.comment.clone(),
                    data_source: route.source.clone(),
                    links: route.links.clone(),
                    type_: route.type_.clone(),
                    number: route.number,
                    original_kind: GpxTrackKind::Route,
                    segments: vec![imported_segment],
                    visible: true,
                });
            }
        }

        if imported_segments == 0 {
            return Err(format!("No drawable tracks found in {file_name}"));
        }

        Ok((imported_segments, imported_bounds))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn load_from_disk_dialog(
        &mut self,
        ctx: &egui::Context,
        map_memory: &mut MapMemory,
    ) {
        let Some(paths) = rfd::FileDialog::new()
            .add_filter("GPX", &["gpx"])
            .pick_files()
        else {
            return;
        };

        self.load_from_paths(paths, ctx, map_memory);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn save_to_disk_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("tracks.gpx")
            .save_file()
        else {
            return;
        };

        let mut gpx_file = gpx::Gpx {
            version: gpx::GpxVersion::Gpx11,
            creator: Some("krokett_editor".to_owned()),
            ..Default::default()
        };

        for track in &self.tracks {
            if track.original_kind == GpxTrackKind::Route && track.segments.len() == 1 {
                let segment = &track.segments[0];
                let mut exported_route = gpx::Route::new();
                if !track.name.trim().is_empty() {
                    exported_route.name = Some(track.name.clone());
                }
                exported_route.description = if segment.description.trim().is_empty() {
                    None
                } else {
                    Some(segment.description.clone())
                };
                exported_route.comment = track.comment.clone();
                exported_route.source = track.data_source.clone();
                exported_route.links = track.links.clone();
                exported_route.type_ = track.type_.clone();
                exported_route.number = track.number;

                exported_route.points = segment.waypoints.clone();
                if let Some(first_waypoint) = exported_route.points.first_mut() {
                    first_waypoint.description = exported_route.description.clone();
                }

                gpx_file.routes.push(exported_route);
                continue;
            }

            let mut exported_track = gpx::Track::new();
            if !track.name.trim().is_empty() {
                exported_track.name = Some(track.name.clone());
            }
            if !track.description.trim().is_empty() {
                exported_track.description = Some(track.description.clone());
            }
            exported_track.comment = track.comment.clone();
            exported_track.source = track.data_source.clone();
            exported_track.links = track.links.clone();
            exported_track.type_ = track.type_.clone();
            exported_track.number = track.number;

            for segment in &track.segments {
                let mut exported_segment = gpx::TrackSegment::new();
                exported_segment.points = segment.waypoints.clone();
                if let Some(first_waypoint) = exported_segment.points.first_mut() {
                    first_waypoint.description = if segment.description.trim().is_empty() {
                        None
                    } else {
                        Some(segment.description.clone())
                    };
                }
                exported_track.segments.push(exported_segment);
            }

            gpx_file.tracks.push(exported_track);
        }

        match std::fs::File::create(&path) {
            Ok(file) => {
                let writer = std::io::BufWriter::new(file);
                match gpx::write(&gpx_file, writer) {
                    Ok(()) => {
                        let message = format!("Saved {} track(s)", self.tracks.len());
                        self.status = Some(message.clone());
                        self.toast_message = Some(message);
                        self.toast_until = Some(Instant::now() + Duration::from_secs(5));
                    }
                    Err(err) => {
                        self.status = Some(format!("Could not save GPX: {err}"));
                    }
                }
            }
            Err(err) => {
                self.status = Some(format!("Could not create {}: {err}", path.display()));
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn load_from_disk_dialog(
        &mut self,
        _ctx: &egui::Context,
        _map_memory: &mut MapMemory,
    ) {
        self.status =
            Some("File dialog is unavailable on this target; use drag and drop".to_owned());
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn save_to_disk_dialog(&mut self) {
        self.status = Some("Save dialog is unavailable on this target".to_owned());
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load_from_paths(
        &mut self,
        paths: Vec<PathBuf>,
        ctx: &egui::Context,
        map_memory: &mut MapMemory,
    ) {
        let mut imported_segments = 0;
        let mut errors = Vec::new();
        let mut imported_bounds: Option<GpxBounds> = None;

        for path in paths {
            let file_name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());

            match std::fs::read(&path) {
                Ok(bytes) => match self.load_gpx_from_bytes(&file_name, &bytes) {
                    Ok((count, bounds)) => {
                        imported_segments += count;
                        if let Some(bounds) = bounds {
                            if let Some(existing) = imported_bounds.as_mut() {
                                existing.merge(bounds);
                            } else {
                                imported_bounds = Some(bounds);
                            }
                        }
                    }
                    Err(error) => errors.push(error),
                },
                Err(err) => errors.push(format!("Could not read {}: {err}", path.display())),
            }
        }

        self.finalize_import(imported_segments, errors, imported_bounds, ctx, map_memory);
    }

    fn finalize_import(
        &mut self,
        imported_segments: usize,
        errors: Vec<String>,
        imported_bounds: Option<GpxBounds>,
        ctx: &egui::Context,
        map_memory: &mut MapMemory,
    ) {
        self.status = if !errors.is_empty() {
            Some(errors.join(" | "))
        } else if imported_segments > 0 {
            let message = format!("Loaded {imported_segments} GPX segment(s)");
            self.toast_message = Some(message.clone());
            self.toast_until = Some(Instant::now() + Duration::from_secs(5));
            Some(message)
        } else {
            Some("No GPX data imported".to_owned())
        };

        if imported_segments > 0 {
            self.pending_gpx_fit = imported_bounds;
            if self.auto_fit_enabled {
                if let Some(bounds) = self.pending_gpx_fit {
                    self.fit_map_to_bounds(bounds, ctx.content_rect().size(), map_memory);
                    self.pending_gpx_fit = None;
                }
            }
            ctx.request_repaint();
        }
    }

    pub(crate) fn handle_dropped_files(&mut self, ctx: &egui::Context, map_memory: &mut MapMemory) {
        let dropped_files = ctx.input(|input| input.raw.dropped_files.clone());
        if dropped_files.is_empty() {
            return;
        }

        let mut imported_segments = 0;
        let mut errors = Vec::new();
        let mut imported_bounds: Option<GpxBounds> = None;

        for file in dropped_files {
            let file_name = if !file.name.is_empty() {
                file.name.clone()
            } else {
                file.path
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| "dropped-file.gpx".to_owned())
            };

            let result = if let Some(bytes) = file.bytes.as_ref() {
                self.load_gpx_from_bytes(&file_name, bytes.as_ref())
            } else if let Some(path) = file.path.as_ref() {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    match std::fs::read(path) {
                        Ok(bytes) => self.load_gpx_from_bytes(&file_name, &bytes),
                        Err(err) => Err(format!("Could not read {}: {err}", path.display())),
                    }
                }
                #[cfg(target_arch = "wasm32")]
                {
                    Err(format!(
                        "Could not read {file_name}: file path access is unavailable"
                    ))
                }
            } else {
                Err(format!("Could not read {file_name}: missing file bytes"))
            };

            match result {
                Ok((count, bounds)) => {
                    imported_segments += count;
                    if let Some(bounds) = bounds {
                        if let Some(existing) = imported_bounds.as_mut() {
                            existing.merge(bounds);
                        } else {
                            imported_bounds = Some(bounds);
                        }
                    }
                }
                Err(error) => errors.push(error),
            }
        }

        self.finalize_import(imported_segments, errors, imported_bounds, ctx, map_memory);
    }

    pub(crate) fn apply_pending_fit(
        &mut self,
        viewport_size: egui::Vec2,
        map_memory: &mut MapMemory,
    ) {
        if self.auto_fit_enabled {
            if let Some(bounds) = self.pending_gpx_fit.take() {
                self.fit_map_to_bounds(bounds, viewport_size, map_memory);
            }
        }
    }

    pub(crate) fn add_plugins<'a, 'b, 'c>(
        &'c self,
        mut map: Map<'a, 'b, 'c>,
    ) -> AddPluginsOutput<'a, 'b, 'c>
    where
        'a: 'c,
        'b: 'c,
    {
        let clicked_track = Arc::new(Mutex::new(None));
        let clicked_segment = Arc::new(Mutex::new(None));
        let cut_request = Arc::new(Mutex::new(None));
        let remove_request = Arc::new(Mutex::new(None));

        for (track_index, track) in self.tracks.iter().enumerate() {
            if !track.visible {
                continue;
            }

            let segment_count = track.segments.len();
            for (segment_index, segment) in track.segments.iter().enumerate() {
                if !segment.visible {
                    continue;
                }

                map = map.with_plugin(GpxPolyline {
                    positions: segment.positions.clone(),
                    description: segment.description.clone(),
                    track_index,
                    segment_index,
                    has_previous_separator: segment_index > 0,
                    has_next_separator: segment_index + 1 < segment_count,
                    window_highlighted: self
                        .window_highlight_segment
                        .map(|selected| selected == (track_index, segment_index))
                        .unwrap_or(false)
                        || self.tree_hover_track == Some(track_index)
                        || self.tree_hover_segment == Some((track_index, segment_index)),
                    cut_tool_enabled: self.cut_tool_enabled,
                    clicked_track: clicked_track.clone(),
                    clicked_segment: clicked_segment.clone(),
                    cut_request: cut_request.clone(),
                    remove_request: remove_request.clone(),
                });
            }
        }

        (
            map,
            clicked_track,
            clicked_segment,
            cut_request,
            remove_request,
        )
    }

    pub(crate) fn consume_track_click(&mut self, clicked_track: Arc<Mutex<Option<usize>>>) {
        if let Some(track_index) = clicked_track.lock().ok().and_then(|mut lock| lock.take()) {
            self.selected_track_index = Some(track_index);
            self.metadata_editor_open = true;
        }
    }

    pub(crate) fn consume_segment_click(
        &mut self,
        clicked_segment: Arc<Mutex<Option<SegmentSelection>>>,
    ) {
        if self.cut_tool_enabled {
            return;
        }

        if let Some((track_index, segment_index)) =
            clicked_segment.lock().ok().and_then(|mut lock| lock.take())
        {
            self.selected_segment = Some((track_index, segment_index));
            self.segment_editor_open = true;
        }
    }

    pub(crate) fn consume_cut_request(&mut self, cut_request: Arc<Mutex<Option<CutRequest>>>) {
        let Some((track_index, segment_index, split_idx)) =
            cut_request.lock().ok().and_then(|mut lock| lock.take())
        else {
            return;
        };

        if track_index >= self.tracks.len()
            || segment_index >= self.tracks[track_index].segments.len()
        {
            return;
        }

        let segment = &self.tracks[track_index].segments[segment_index];
        if split_idx == 0 || split_idx >= segment.positions.len() {
            return;
        }

        let first_waypoints = segment.waypoints[..=split_idx].to_vec();
        let second_waypoints = segment.waypoints[split_idx..].to_vec();
        let first_positions = segment.positions[..=split_idx].to_vec();
        let second_positions = segment.positions[split_idx..].to_vec();

        if first_positions.len() < 2 || second_positions.len() < 2 {
            self.status = Some("Unable to cut segment at this position".to_owned());
            return;
        }

        let original_description = segment.description.clone();
        self.tracks[track_index].segments.remove(segment_index);
        self.tracks[track_index].segments.insert(
            segment_index,
            GpxSegment {
                waypoints: first_waypoints,
                positions: first_positions,
                description: original_description,
                visible: true,
            },
        );
        self.tracks[track_index].segments.insert(
            segment_index + 1,
            GpxSegment {
                waypoints: second_waypoints,
                positions: second_positions,
                description: String::new(),
                visible: true,
            },
        );

        self.selected_segment = Some((track_index, segment_index + 1));
        self.segment_editor_open = true;
        self.status = Some("Segment cut".to_owned());
        self.toast_message = Some("Segment cut".to_owned());
        self.toast_until = Some(Instant::now() + Duration::from_secs(5));
    }

    pub(crate) fn consume_remove_request(
        &mut self,
        remove_request: Arc<Mutex<Option<MergeRequest>>>,
    ) {
        let Some((track_index, left_idx)) =
            remove_request.lock().ok().and_then(|mut lock| lock.take())
        else {
            return;
        };

        if track_index >= self.tracks.len() {
            return;
        }

        let segment_count = self.tracks[track_index].segments.len();
        if segment_count < 2 {
            self.status = Some("No adjacent segment to merge".to_owned());
            return;
        }

        if left_idx + 1 >= segment_count {
            return;
        }

        let right_idx = left_idx + 1;

        let left_segment = self.tracks[track_index].segments[left_idx].clone();
        let right_segment = self.tracks[track_index].segments[right_idx].clone();

        let mut merged_waypoints = left_segment.waypoints.clone();
        let mut merged_positions = left_segment.positions.clone();
        let mut right_waypoints = right_segment.waypoints;
        let mut right_positions = right_segment.positions;
        if merged_positions.last() == right_positions.first() {
            if !right_waypoints.is_empty() {
                right_waypoints.remove(0);
            }
            right_positions.remove(0);
        }
        merged_waypoints.extend(right_waypoints);
        merged_positions.extend(right_positions);

        let merged_description = if !left_segment.description.trim().is_empty() {
            left_segment.description
        } else {
            right_segment.description
        };

        self.tracks[track_index].segments[left_idx] = GpxSegment {
            waypoints: merged_waypoints,
            positions: merged_positions,
            description: merged_description,
            visible: true,
        };
        self.tracks[track_index].segments.remove(right_idx);

        self.selected_segment = Some((track_index, left_idx));
        self.segment_editor_open = true;
        self.status = Some("Segments merged".to_owned());
        self.toast_message = Some("Segments merged".to_owned());
        self.toast_until = Some(Instant::now() + Duration::from_secs(5));
    }
}

struct GpxPolyline {
    positions: Vec<walkers::Position>,
    description: String,
    track_index: usize,
    segment_index: usize,
    has_previous_separator: bool,
    has_next_separator: bool,
    window_highlighted: bool,
    cut_tool_enabled: bool,
    clicked_track: Arc<Mutex<Option<usize>>>,
    clicked_segment: Arc<Mutex<Option<SegmentSelection>>>,
    cut_request: Arc<Mutex<Option<CutRequest>>>,
    remove_request: Arc<Mutex<Option<MergeRequest>>>,
}

fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let ab_len_sq = ab.length_sq();
    if ab_len_sq <= f32::EPSILON {
        return ap.length();
    }
    let t = (ap.dot(ab) / ab_len_sq).clamp(0.0, 1.0);
    let projection = a + ab * t;
    (p - projection).length()
}

fn pointer_hits_polyline(
    pointer: Pos2,
    positions: &[walkers::Position],
    projector: &walkers::Projector,
) -> bool {
    const CLICK_DISTANCE: f32 = 8.0;
    for (from, to) in positions.iter().tuple_windows() {
        let from_projected = projector.project(*from).to_pos2();
        let to_projected = projector.project(*to).to_pos2();
        if distance_to_segment(pointer, from_projected, to_projected) <= CLICK_DISTANCE {
            return true;
        }
    }
    false
}

fn nearest_segment_split_index(
    pointer: Pos2,
    positions: &[walkers::Position],
    projector: &walkers::Projector,
) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (index, (from, to)) in positions.iter().tuple_windows().enumerate() {
        let from_projected = projector.project(*from).to_pos2();
        let to_projected = projector.project(*to).to_pos2();
        let distance = distance_to_segment(pointer, from_projected, to_projected);
        match best {
            Some((_, best_distance)) if distance >= best_distance => {}
            _ => best = Some((index, distance)),
        }
    }
    best.and_then(|(index, distance)| {
        if distance <= 8.0 {
            Some(index + 1)
        } else {
            None
        }
    })
}

fn merge_left_index_from_separator_click(
    pointer: Pos2,
    positions: &[walkers::Position],
    projector: &walkers::Projector,
    segment_index: usize,
    has_previous_separator: bool,
    has_next_separator: bool,
) -> Option<usize> {
    const CLICK_DISTANCE: f32 = 8.0;
    let mut closest: Option<(usize, f32)> = None;

    if has_previous_separator {
        if let Some(first) = positions.first() {
            let first_projected = projector.project(*first).to_pos2();
            let distance = pointer.distance(first_projected);
            if distance <= CLICK_DISTANCE {
                closest = Some((segment_index - 1, distance));
            }
        }
    }

    if has_next_separator {
        if let Some(last) = positions.last() {
            let last_projected = projector.project(*last).to_pos2();
            let distance = pointer.distance(last_projected);
            if distance <= CLICK_DISTANCE {
                match closest {
                    Some((_, best_distance)) if distance >= best_distance => {}
                    _ => closest = Some((segment_index, distance)),
                }
            }
        }
    }

    closest.map(|(left_index, _)| left_index)
}

impl Plugin for GpxPolyline {
    fn run(
        self: Box<Self>,
        ui: &mut egui::Ui,
        response: &egui::Response,
        projector: &walkers::Projector,
        _map_memory: &walkers::MapMemory,
    ) {
        let hover_pos = response.hover_pos();
        let hovered = hover_pos
            .map(|pointer_pos| pointer_hits_polyline(pointer_pos, &self.positions, projector))
            .unwrap_or(false);

        let separator_hovered = hover_pos
            .and_then(|pointer_pos| {
                merge_left_index_from_separator_click(
                    pointer_pos,
                    &self.positions,
                    projector,
                    self.segment_index,
                    self.has_previous_separator,
                    self.has_next_separator,
                )
            })
            .is_some();

        if self.cut_tool_enabled {
            if separator_hovered {
                ui.ctx().set_cursor_icon(egui::CursorIcon::NoDrop);
            } else if self.cut_tool_enabled && hovered {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Copy);
            }
        }

        if hovered && !separator_hovered && !self.description.trim().is_empty() {
            let tooltip_id = egui::Id::new((
                "gpx_segment_hover_desc",
                self.track_index,
                self.segment_index,
            ));
            egui::Tooltip::always_open(
                ui.ctx().clone(),
                response.layer_id,
                tooltip_id,
                egui::PopupAnchor::Pointer,
            )
            .show(|ui| {
                ui.label(&self.description);
            });
        }

        let stroke = if hovered || self.window_highlighted {
            // segment hover
            egui::Stroke::new(5.0, Color32::from_rgb(14, 214, 85))
        } else if !self.description.trim().is_empty() {
            // segment with description
            egui::Stroke::new(4.0, Color32::from_rgb(65, 130, 210))
        } else {
            // segment not hover no description
            egui::Stroke::new(4.0, Color32::from_rgb(255, 111, 0))
        };

        for (from, to) in self.positions.iter().tuple_windows() {
            let from_projected = projector.project(*from).to_pos2();
            let to_projected = projector.project(*to).to_pos2();
            ui.painter().add(egui::Shape::line(
                vec![from_projected, to_projected],
                stroke,
            ));
        }

        if let Some(first) = self.positions.first() {
            let p = projector.project(*first).to_pos2();
            ui.painter()
                .circle_filled(p, 3.5, Color32::BLACK.gamma_multiply(0.9));
            if self.cut_tool_enabled && self.has_previous_separator {
                ui.painter().circle(
                    p,
                    6.5,
                    Color32::from_rgb(255, 224, 96),
                    egui::Stroke::new(1.5, Color32::from_rgb(45, 45, 45)),
                );
            }
        }
        if let Some(last) = self.positions.last() {
            let p = projector.project(*last).to_pos2();
            ui.painter()
                .circle_filled(p, 3.5, Color32::BLACK.gamma_multiply(0.9));
            if self.cut_tool_enabled && self.has_next_separator {
                ui.painter().circle(
                    p,
                    6.5,
                    Color32::from_rgb(255, 224, 96),
                    egui::Stroke::new(1.5, Color32::from_rgb(45, 45, 45)),
                );
            }
        }

        if response.clicked_by(PointerButton::Secondary) {
            if let Some(pointer_pos) = response.interact_pointer_pos() {
                if self.cut_tool_enabled {
                    if let Some(left_index) = merge_left_index_from_separator_click(
                        pointer_pos,
                        &self.positions,
                        projector,
                        self.segment_index,
                        self.has_previous_separator,
                        self.has_next_separator,
                    ) {
                        if let Ok(mut remove) = self.remove_request.lock() {
                            *remove = Some((self.track_index, left_index));
                        }
                    }
                } else if pointer_hits_polyline(pointer_pos, &self.positions, projector) {
                    if let Ok(mut clicked) = self.clicked_track.lock() {
                        *clicked = Some(self.track_index);
                    }
                }
            }
        }

        if response.clicked_by(PointerButton::Primary) {
            if let Some(pointer_pos) = response.interact_pointer_pos() {
                if pointer_hits_polyline(pointer_pos, &self.positions, projector) {
                    if self.cut_tool_enabled {
                        if let Some(split_idx) =
                            nearest_segment_split_index(pointer_pos, &self.positions, projector)
                        {
                            if let Ok(mut cut) = self.cut_request.lock() {
                                *cut = Some((self.track_index, self.segment_index, split_idx));
                            }
                        }
                    } else if let Ok(mut clicked) = self.clicked_segment.lock() {
                        *clicked = Some((self.track_index, self.segment_index));
                    }
                }
            }
        }
    }
}
