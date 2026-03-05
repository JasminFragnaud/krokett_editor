use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex};

use egui::{Color32, PointerButton, Pos2};
use egui_ltreeview::{NodeBuilder, TreeView};
use egui_notify::{Anchor, Toasts};
use itertools::Itertools as _;
use walkers::{Map, MapMemory, Plugin};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum GpxTrackKind {
    Track,
    Route,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TrackSelection {
    file_index: usize,
    kind: GpxTrackKind,
    track_index: usize,
}

#[derive(Clone, Copy)]
pub struct GpxBounds {
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

type SegmentSelection = (TrackSelection, usize);
type CutRequest = (TrackSelection, usize, usize);
type MergeRequest = (TrackSelection, usize);
type ClickedTrack = Arc<Mutex<Option<TrackSelection>>>;
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
    File(usize),
    Track(TrackSelection),
    Segment(TrackSelection, usize),
}

pub(crate) struct GpxState {
    gpx_documents: Vec<gpx::Gpx>,
    track_visibility: BTreeMap<TrackSelection, bool>,
    segment_visibility: BTreeMap<SegmentSelection, bool>,
    status: Option<String>,
    pending_gpx_fit: Option<GpxBounds>,
    auto_fit_enabled: bool,
    toasts: Toasts,
    metadata_editor_open: bool,
    selected_track_index: Option<TrackSelection>,
    segment_editor_open: bool,
    selected_segment: Option<SegmentSelection>,
    window_highlight_segment: Option<SegmentSelection>,
    tree_window_visible: bool,
    tree_hover_track: Option<TrackSelection>,
    tree_hover_segment: Option<SegmentSelection>,
    cut_tool_enabled: bool,
}

impl GpxState {
    pub(crate) fn new() -> Self {
        Self {
            gpx_documents: Vec::new(),
            track_visibility: BTreeMap::new(),
            segment_visibility: BTreeMap::new(),
            status: None,
            pending_gpx_fit: None,
            auto_fit_enabled: true,
            toasts: Toasts::default().with_anchor(Anchor::BottomRight),
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
        self.gpx_documents
            .iter()
            .map(|document| {
                document
                    .tracks
                    .iter()
                    .map(|track| track.segments.len())
                    .sum::<usize>()
                    + document.routes.len()
            })
            .sum()
    }

    pub(crate) fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub(crate) fn set_status_message(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.status = Some(message.clone());
        self.toasts.info(message);
    }

    pub(crate) fn export_file_name(&self) -> String {
        let base_name = self
            .gpx_documents
            .first()
            .and_then(|document| document.metadata.as_ref())
            .and_then(|metadata| metadata.name.clone())
            .unwrap_or_else(|| "tracks".to_owned());

        if base_name.to_ascii_lowercase().ends_with(".gpx") {
            base_name
        } else {
            format!("{base_name}.gpx")
        }
    }

    pub(crate) fn export_gpx_bytes(&self) -> Result<Vec<u8>, String> {
        if self.gpx_documents.is_empty() {
            return Err("No GPX data to save".to_owned());
        }

        let mut merged = gpx::Gpx {
            version: gpx::GpxVersion::Gpx11,
            creator: Some("krokett_editor".to_owned()),
            ..Default::default()
        };

        if let Some(first) = self.gpx_documents.first() {
            if first.version != gpx::GpxVersion::Unknown {
                merged.version = first.version;
            }
            merged.creator = first.creator.clone().or(merged.creator);
            merged.metadata = first.metadata.clone();
        }

        for document in &self.gpx_documents {
            merged.waypoints.extend(document.waypoints.clone());
            merged.tracks.extend(document.tracks.clone());
            merged.routes.extend(document.routes.clone());
        }

        let mut bytes = Vec::new();
        gpx::write(&merged, &mut bytes)
            .map_err(|err| format!("Failed to serialize GPX data: {err}"))?;

        Ok(bytes)
    }

    pub(crate) fn clear(&mut self) {
        self.gpx_documents.clear();
        self.track_visibility.clear();
        self.segment_visibility.clear();
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

    fn source_for_file(&self, file_index: usize) -> String {
        self.gpx_documents
            .get(file_index)
            .and_then(|document| document.metadata.as_ref())
            .and_then(|metadata| metadata.name.clone())
            .unwrap_or_else(|| format!("GPX {}", file_index + 1))
    }

    fn file_track_selections(&self, file_index: usize) -> Vec<TrackSelection> {
        let Some(document) = self.gpx_documents.get(file_index) else {
            return Vec::new();
        };

        let mut selections = Vec::with_capacity(document.tracks.len() + document.routes.len());
        for track_index in 0..document.tracks.len() {
            selections.push(TrackSelection {
                file_index,
                kind: GpxTrackKind::Track,
                track_index,
            });
        }
        for route_index in 0..document.routes.len() {
            selections.push(TrackSelection {
                file_index,
                kind: GpxTrackKind::Route,
                track_index: route_index,
            });
        }
        selections
    }

    fn segment_count(&self, track_selection: TrackSelection) -> Option<usize> {
        match track_selection.kind {
            GpxTrackKind::Track => self
                .gpx_documents
                .get(track_selection.file_index)
                .and_then(|document| document.tracks.get(track_selection.track_index))
                .map(|track| track.segments.len()),
            GpxTrackKind::Route => self
                .gpx_documents
                .get(track_selection.file_index)
                .and_then(|document| document.routes.get(track_selection.track_index))
                .map(|_| 1),
        }
    }

    fn track_name(&self, track_selection: TrackSelection) -> Option<String> {
        match track_selection.kind {
            GpxTrackKind::Track => self
                .gpx_documents
                .get(track_selection.file_index)
                .and_then(|document| document.tracks.get(track_selection.track_index))
                .and_then(|track| track.name.clone()),
            GpxTrackKind::Route => self
                .gpx_documents
                .get(track_selection.file_index)
                .and_then(|document| document.routes.get(track_selection.track_index))
                .and_then(|route| route.name.clone()),
        }
    }

    fn track_description(&self, track_selection: TrackSelection) -> Option<String> {
        match track_selection.kind {
            GpxTrackKind::Track => self
                .gpx_documents
                .get(track_selection.file_index)
                .and_then(|document| document.tracks.get(track_selection.track_index))
                .and_then(|track| track.description.clone()),
            GpxTrackKind::Route => self
                .gpx_documents
                .get(track_selection.file_index)
                .and_then(|document| document.routes.get(track_selection.track_index))
                .and_then(|route| route.description.clone()),
        }
    }

    fn set_track_metadata(
        &mut self,
        track_selection: TrackSelection,
        name: String,
        description: String,
    ) {
        match track_selection.kind {
            GpxTrackKind::Track => {
                if let Some(track) = self
                    .gpx_documents
                    .get_mut(track_selection.file_index)
                    .and_then(|document| document.tracks.get_mut(track_selection.track_index))
                {
                    track.name = Some(name);
                    track.description = Some(description);
                }
            }
            GpxTrackKind::Route => {
                if let Some(route) = self
                    .gpx_documents
                    .get_mut(track_selection.file_index)
                    .and_then(|document| document.routes.get_mut(track_selection.track_index))
                {
                    route.name = Some(name);
                    route.description = Some(description);
                }
            }
        }
    }

    fn segment_waypoints(&self, selection: SegmentSelection) -> Option<&[gpx::Waypoint]> {
        let (track_selection, segment_index) = selection;
        match track_selection.kind {
            GpxTrackKind::Track => self
                .gpx_documents
                .get(track_selection.file_index)
                .and_then(|document| document.tracks.get(track_selection.track_index))
                .and_then(|track| track.segments.get(segment_index))
                .map(|segment| segment.points.as_slice()),
            GpxTrackKind::Route => {
                if segment_index != 0 {
                    return None;
                }
                self.gpx_documents
                    .get(track_selection.file_index)
                    .and_then(|document| document.routes.get(track_selection.track_index))
                    .map(|route| route.points.as_slice())
            }
        }
    }

    fn segment_waypoints_mut(
        &mut self,
        selection: SegmentSelection,
    ) -> Option<&mut Vec<gpx::Waypoint>> {
        let (track_selection, segment_index) = selection;
        match track_selection.kind {
            GpxTrackKind::Track => self
                .gpx_documents
                .get_mut(track_selection.file_index)
                .and_then(|document| document.tracks.get_mut(track_selection.track_index))
                .and_then(|track| track.segments.get_mut(segment_index))
                .map(|segment| &mut segment.points),
            GpxTrackKind::Route => {
                if segment_index != 0 {
                    return None;
                }
                self.gpx_documents
                    .get_mut(track_selection.file_index)
                    .and_then(|document| document.routes.get_mut(track_selection.track_index))
                    .map(|route| &mut route.points)
            }
        }
    }

    fn segment_description(&self, selection: SegmentSelection) -> String {
        self.segment_waypoints(selection)
            .and_then(|waypoints| waypoints.first())
            .and_then(|waypoint| waypoint.description.clone())
            .unwrap_or_default()
    }

    fn set_segment_description(&mut self, selection: SegmentSelection, description: String) {
        if let Some(waypoints) = self.segment_waypoints_mut(selection) {
            if let Some(first) = waypoints.first_mut() {
                first.description = if description.trim().is_empty() {
                    None
                } else {
                    Some(description)
                };
            }
        }
    }

    fn is_track_visible(&self, track_selection: TrackSelection) -> bool {
        self.track_visibility
            .get(&track_selection)
            .copied()
            .unwrap_or(true)
    }

    fn set_track_visible(&mut self, track_selection: TrackSelection, visible: bool) {
        self.track_visibility.insert(track_selection, visible);
        if let Some(segment_count) = self.segment_count(track_selection) {
            for segment_index in 0..segment_count {
                self.segment_visibility
                    .insert((track_selection, segment_index), visible);
            }
        }
    }

    fn is_segment_visible(&self, segment_selection: SegmentSelection) -> bool {
        self.segment_visibility
            .get(&segment_selection)
            .copied()
            .unwrap_or(true)
    }

    fn set_segment_visible(&mut self, segment_selection: SegmentSelection, visible: bool) {
        self.segment_visibility.insert(segment_selection, visible);
    }

    fn segment_positions(waypoints: &[gpx::Waypoint]) -> Vec<walkers::Position> {
        waypoints
            .iter()
            .map(|waypoint| {
                let point = waypoint.point();
                walkers::lat_lon(point.y(), point.x())
            })
            .collect()
    }

    fn include_waypoints_in_bounds(
        waypoints: &[gpx::Waypoint],
        bounds: &mut Option<GpxBounds>,
    ) -> bool {
        let positions = Self::segment_positions(waypoints);
        if positions.len() <= 1 {
            return false;
        }

        let mut segment_bounds = GpxBounds::from_position(positions[0]);
        for position in &positions {
            segment_bounds.include_position(*position);
        }

        if let Some(existing) = bounds.as_mut() {
            existing.merge(segment_bounds);
        } else {
            *bounds = Some(segment_bounds);
        }

        true
    }

    pub(crate) fn show_toast(&mut self, ctx: &egui::Context) {
        self.toasts.show(ctx);
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
                if self.gpx_documents.is_empty() {
                    ui.label("No GPX loaded");
                    return;
                }

                let mut hover_track = None;
                let mut hover_segment = None;
                let mut click_track = None;
                let mut click_segment = None;

                let mut file_visibility_updates: Vec<(Vec<TrackSelection>, bool)> = Vec::new();
                let mut track_visibility_updates: Vec<(TrackSelection, bool)> = Vec::new();
                let mut segment_visibility_updates: Vec<(SegmentSelection, bool)> = Vec::new();

                let tree_id = ui.make_persistent_id("gpx_tree_view");
                let (_response, _actions) = TreeView::new(tree_id).show(ui, |builder| {
                    for file_index in 0..self.gpx_documents.len() {
                        let track_selections = self.file_track_selections(file_index);
                        if track_selections.is_empty() {
                            continue;
                        }

                        let mut file_visible = true;
                        for &track_selection in &track_selections {
                            if !self.is_track_visible(track_selection) {
                                file_visible = false;
                                break;
                            }
                            if let Some(segment_count) = self.segment_count(track_selection) {
                                if (0..segment_count).any(|segment_index| {
                                    !self.is_segment_visible((track_selection, segment_index))
                                }) {
                                    file_visible = false;
                                    break;
                                }
                            }
                        }

                        let source = self.source_for_file(file_index);
                        let file_label = format!("File: {source}");
                        let file_is_open = builder.node(
                            NodeBuilder::dir(GpxTreeNodeId::File(file_index)).label_ui(|ui| {
                                let row = ui.horizontal(|ui| {
                                    let checkbox_response = ui.checkbox(&mut file_visible, "");
                                    if checkbox_response.changed() {
                                        file_visibility_updates
                                            .push((track_selections.clone(), file_visible));
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
                                    hover_track = track_selections.first().copied();
                                }
                            }),
                        );

                        if file_is_open {
                            for (in_file_index, &track_selection) in
                                track_selections.iter().enumerate()
                            {
                                let mut track_visible = self.is_track_visible(track_selection);
                                let default_prefix = match track_selection.kind {
                                    GpxTrackKind::Track => "Track",
                                    GpxTrackKind::Route => "Route",
                                };
                                let track_title = self
                                    .track_name(track_selection)
                                    .filter(|name| !name.trim().is_empty())
                                    .unwrap_or_else(|| {
                                        format!("{default_prefix} {}", in_file_index + 1)
                                    });

                                let track_is_open = builder.node(
                                    NodeBuilder::dir(GpxTreeNodeId::Track(track_selection))
                                        .label_ui(|ui| {
                                            let row = ui.horizontal(|ui| {
                                                let checkbox_response =
                                                    ui.checkbox(&mut track_visible, "");
                                                if checkbox_response.changed() {
                                                    track_visibility_updates
                                                        .push((track_selection, track_visible));
                                                }
                                                let label_response = ui.label(&track_title);
                                                (
                                                    checkbox_response.hovered()
                                                        || label_response.hovered(),
                                                    label_response.clicked(),
                                                    checkbox_response.rect,
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
                                                                && !row.inner.2.contains(pointer)
                                                        },
                                                    )
                                            });

                                            if row.inner.0 || row_hovered {
                                                hover_track = Some(track_selection);
                                            }
                                            if row.inner.1 || row_clicked {
                                                click_track = Some(track_selection);
                                            }
                                        }),
                                );

                                if track_is_open {
                                    let segment_count =
                                        self.segment_count(track_selection).unwrap_or(0);
                                    for segment_index in 0..segment_count {
                                        let mut segment_visible = self
                                            .is_segment_visible((track_selection, segment_index));
                                        let segment_label = format!(
                                            "{}: {}",
                                            segment_index + 1,
                                            self.segment_description((
                                                track_selection,
                                                segment_index
                                            ))
                                        );

                                        builder.node(
                                            NodeBuilder::leaf(GpxTreeNodeId::Segment(
                                                track_selection,
                                                segment_index,
                                            ))
                                            .label_ui(|ui| {
                                                let row = ui.horizontal(|ui| {
                                                    let checkbox_response =
                                                        ui.checkbox(&mut segment_visible, "");
                                                    if checkbox_response.changed() {
                                                        segment_visibility_updates.push((
                                                            (track_selection, segment_index),
                                                            segment_visible,
                                                        ));
                                                    }
                                                    let label_response = ui.label(&segment_label);
                                                    (
                                                        checkbox_response.hovered()
                                                            || label_response.hovered(),
                                                        label_response.clicked(),
                                                        checkbox_response.rect,
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
                                                                    && !row
                                                                        .inner
                                                                        .2
                                                                        .contains(pointer)
                                                            },
                                                        )
                                                });

                                                if row.inner.0 || row_hovered {
                                                    hover_segment =
                                                        Some((track_selection, segment_index));
                                                }
                                                if row.inner.1 || row_clicked {
                                                    click_segment =
                                                        Some((track_selection, segment_index));
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

                for (track_selections, visible) in file_visibility_updates {
                    for track_selection in track_selections {
                        self.set_track_visible(track_selection, visible);
                    }
                }

                for (track_selection, visible) in track_visibility_updates {
                    self.set_track_visible(track_selection, visible);
                }

                for (segment_selection, visible) in segment_visibility_updates {
                    self.set_segment_visible(segment_selection, visible);
                }

                if let Some(track_selection) = click_track {
                    self.selected_track_index = Some(track_selection);
                    self.metadata_editor_open = true;
                }

                if let Some(segment_selection) = click_segment {
                    self.selected_segment = Some(segment_selection);
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
        let Some(track_selection) = self.selected_track_index else {
            return;
        };
        if self.segment_count(track_selection).is_none() {
            self.metadata_editor_open = false;
            self.selected_track_index = None;
            return;
        }

        let mut open = self.metadata_editor_open;
        let mut track_name = self.track_name(track_selection).unwrap_or_default();
        let mut track_description = self.track_description(track_selection).unwrap_or_default();
        let source = self.source_for_file(track_selection.file_index);

        egui::Window::new("Track metadata")
            .open(&mut open)
            .resizable(true)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.label(format!("Source: {source}"));
                ui.separator();
                ui.label("Name");
                ui.text_edit_singleline(&mut track_name);
                ui.label("Description");
                ui.text_edit_multiline(&mut track_description);
            });

        self.set_track_metadata(track_selection, track_name, track_description);

        self.metadata_editor_open = open;
        if !self.metadata_editor_open {
            self.selected_track_index = None;
        }
    }

    pub(crate) fn show_segment_editor_window(&mut self, ctx: &egui::Context) {
        let Some((track_selection, segment_index)) = self.selected_segment else {
            self.window_highlight_segment = None;
            return;
        };

        let Some(segment_count) = self.segment_count(track_selection) else {
            self.segment_editor_open = false;
            self.selected_segment = None;
            self.window_highlight_segment = None;
            return;
        };

        if segment_index >= segment_count {
            self.segment_editor_open = false;
            self.selected_segment = None;
            self.window_highlight_segment = None;
            return;
        }

        let mut open = self.segment_editor_open;
        let mut go_previous = false;
        let mut go_next = false;

        let track_name = self
            .track_name(track_selection)
            .unwrap_or_else(|| "Unnamed".to_owned());
        let mut segment_description = self.segment_description((track_selection, segment_index));

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
                ui.text_edit_multiline(&mut segment_description);
            });

        self.set_segment_description((track_selection, segment_index), segment_description);

        let window_hovered = response
            .as_ref()
            .and_then(|r| {
                ctx.pointer_hover_pos()
                    .map(|pointer| r.response.rect.contains(pointer))
            })
            .unwrap_or(false);

        self.window_highlight_segment = if window_hovered {
            Some((track_selection, segment_index))
        } else {
            None
        };

        if go_previous {
            self.selected_segment = Some((track_selection, segment_index - 1));
        } else if go_next {
            self.selected_segment = Some((track_selection, segment_index + 1));
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

    pub fn load_gpx_from_bytes(
        &mut self,
        file_name: &str,
        bytes: &[u8],
    ) -> Result<(usize, Option<GpxBounds>), String> {
        let mut gpx = gpx::read(Cursor::new(bytes))
            .map_err(|err| format!("Could not parse {file_name}: {err}"))?;

        let mut imported_segments = 0;
        let mut imported_bounds: Option<GpxBounds> = None;

        for track in &gpx.tracks {
            for segment in &track.segments {
                if Self::include_waypoints_in_bounds(&segment.points, &mut imported_bounds) {
                    imported_segments += 1;
                }
            }
        }

        for route in &gpx.routes {
            if Self::include_waypoints_in_bounds(&route.points, &mut imported_bounds) {
                imported_segments += 1;
            }
        }

        if imported_segments == 0 {
            return Err(format!("No drawable tracks found in {file_name}"));
        }

        gpx.metadata
            .get_or_insert_with(Default::default)
            .name
            .get_or_insert_with(|| file_name.to_owned());

        self.gpx_documents.push(gpx);

        Ok((imported_segments, imported_bounds))
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
            let message = errors.join(" | ");
            self.toasts.error(message.clone());
            Some(message)
        } else if imported_segments > 0 {
            let message = format!("Loaded {imported_segments} GPX segment(s)");
            self.toasts.success(message.clone());
            Some(message)
        } else {
            let message = "No GPX data imported".to_owned();
            self.toasts.warning(message.clone());
            Some(message)
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
            } else if file.path.is_some() {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let path = file
                        .path
                        .as_ref()
                        .expect("file.path.is_some() checked above");
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

    pub(crate) fn load_gpx_file(
        &mut self,
        file_name: &str,
        bytes: &[u8],
        ctx: &egui::Context,
        map_memory: &mut MapMemory,
    ) {
        let mut imported_segments = 0;
        let mut errors = Vec::new();
        let mut imported_bounds = None;

        match self.load_gpx_from_bytes(file_name, bytes) {
            Ok((count, bounds)) => {
                imported_segments = count;
                imported_bounds = bounds;
            }
            Err(error) => errors.push(error),
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

        for (file_index, document) in self.gpx_documents.iter().enumerate() {
            for (track_index, track) in document.tracks.iter().enumerate() {
                let track_selection = TrackSelection {
                    file_index,
                    kind: GpxTrackKind::Track,
                    track_index,
                };

                if !self.is_track_visible(track_selection) {
                    continue;
                }

                let segment_count = track.segments.len();
                for (segment_index, segment) in track.segments.iter().enumerate() {
                    let segment_selection = (track_selection, segment_index);
                    if !self.is_segment_visible(segment_selection) {
                        continue;
                    }

                    let positions = Self::segment_positions(&segment.points);
                    if positions.len() <= 1 {
                        continue;
                    }

                    let description = segment
                        .points
                        .first()
                        .and_then(|waypoint| waypoint.description.clone())
                        .unwrap_or_default();

                    map = map.with_plugin(GpxPolyline {
                        positions,
                        description,
                        track_selection,
                        segment_index,
                        has_previous_separator: segment_index > 0,
                        has_next_separator: segment_index + 1 < segment_count,
                        window_highlighted: self
                            .window_highlight_segment
                            .map(|selected| selected == segment_selection)
                            .unwrap_or(false)
                            || self.tree_hover_track == Some(track_selection)
                            || self.tree_hover_segment == Some(segment_selection),
                        cut_tool_enabled: self.cut_tool_enabled,
                        clicked_track: clicked_track.clone(),
                        clicked_segment: clicked_segment.clone(),
                        cut_request: cut_request.clone(),
                        remove_request: remove_request.clone(),
                    });
                }
            }

            for (route_index, route) in document.routes.iter().enumerate() {
                let track_selection = TrackSelection {
                    file_index,
                    kind: GpxTrackKind::Route,
                    track_index: route_index,
                };

                if !self.is_track_visible(track_selection)
                    || !self.is_segment_visible((track_selection, 0))
                {
                    continue;
                }

                let positions = Self::segment_positions(&route.points);
                if positions.len() <= 1 {
                    continue;
                }

                let description = route
                    .points
                    .first()
                    .and_then(|waypoint| waypoint.description.clone())
                    .unwrap_or_default();

                map = map.with_plugin(GpxPolyline {
                    positions,
                    description,
                    track_selection,
                    segment_index: 0,
                    has_previous_separator: false,
                    has_next_separator: false,
                    window_highlighted: self
                        .window_highlight_segment
                        .map(|selected| selected == (track_selection, 0))
                        .unwrap_or(false)
                        || self.tree_hover_track == Some(track_selection)
                        || self.tree_hover_segment == Some((track_selection, 0)),
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

    pub(crate) fn consume_track_click(&mut self, clicked_track: ClickedTrack) {
        if let Some(track_selection) = clicked_track.lock().ok().and_then(|mut lock| lock.take()) {
            self.selected_track_index = Some(track_selection);
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

        if let Some((track_selection, segment_index)) =
            clicked_segment.lock().ok().and_then(|mut lock| lock.take())
        {
            self.selected_segment = Some((track_selection, segment_index));
            self.segment_editor_open = true;
        }
    }

    pub(crate) fn consume_cut_request(&mut self, cut_request: Arc<Mutex<Option<CutRequest>>>) {
        let Some((track_selection, segment_index, split_idx)) =
            cut_request.lock().ok().and_then(|mut lock| lock.take())
        else {
            return;
        };

        if track_selection.kind != GpxTrackKind::Track {
            self.status = Some("Cut is only supported for track segments".to_owned());
            return;
        }

        let Some(track) = self
            .gpx_documents
            .get_mut(track_selection.file_index)
            .and_then(|document| document.tracks.get_mut(track_selection.track_index))
        else {
            return;
        };

        if segment_index >= track.segments.len() {
            return;
        }

        let original_segment = track.segments[segment_index].clone();
        if split_idx == 0 || split_idx >= original_segment.points.len() {
            return;
        }

        let first_waypoints = original_segment.points[..=split_idx].to_vec();
        let mut second_waypoints = original_segment.points[split_idx..].to_vec();

        if first_waypoints.len() < 2 || second_waypoints.len() < 2 {
            self.status = Some("Unable to cut segment at this position".to_owned());
            return;
        }

        if let Some(first) = second_waypoints.first_mut() {
            first.description = None;
        }

        let mut first_segment = original_segment.clone();
        first_segment.points = first_waypoints;

        let mut second_segment = original_segment;
        second_segment.points = second_waypoints;

        track.segments.remove(segment_index);
        track.segments.insert(segment_index, first_segment);
        track.segments.insert(segment_index + 1, second_segment);

        self.set_segment_visible((track_selection, segment_index), true);
        self.set_segment_visible((track_selection, segment_index + 1), true);

        self.selected_segment = Some((track_selection, segment_index + 1));
        self.segment_editor_open = true;
        self.status = Some("Segment cut".to_owned());
        self.toasts.success("Segment cut");
    }

    pub(crate) fn consume_remove_request(
        &mut self,
        remove_request: Arc<Mutex<Option<MergeRequest>>>,
    ) {
        let Some((track_selection, left_idx)) =
            remove_request.lock().ok().and_then(|mut lock| lock.take())
        else {
            return;
        };

        if track_selection.kind != GpxTrackKind::Track {
            self.status = Some("Merge is only supported for track segments".to_owned());
            return;
        }

        let Some(track) = self
            .gpx_documents
            .get_mut(track_selection.file_index)
            .and_then(|document| document.tracks.get_mut(track_selection.track_index))
        else {
            return;
        };

        let segment_count = track.segments.len();
        if segment_count < 2 {
            self.status = Some("No adjacent segment to merge".to_owned());
            return;
        }

        if left_idx + 1 >= segment_count {
            return;
        }

        let right_idx = left_idx + 1;

        let left_segment = track.segments[left_idx].clone();
        let right_segment = track.segments[right_idx].clone();

        let mut merged_waypoints = left_segment.points;
        let mut right_waypoints = right_segment.points;
        if let (Some(left_last), Some(right_first)) =
            (merged_waypoints.last(), right_waypoints.first())
        {
            if left_last.point() == right_first.point() && !right_waypoints.is_empty() {
                right_waypoints.remove(0);
            }
        }
        merged_waypoints.extend(right_waypoints);

        let left_description = merged_waypoints
            .first()
            .and_then(|waypoint| waypoint.description.clone())
            .unwrap_or_default();
        if left_description.trim().is_empty() {
            let right_description = track.segments[right_idx]
                .points
                .first()
                .and_then(|waypoint| waypoint.description.clone());
            if let Some(first) = merged_waypoints.first_mut() {
                first.description = right_description;
            }
        }

        let mut merged_segment = track.segments[left_idx].clone();
        merged_segment.points = merged_waypoints;

        track.segments[left_idx] = merged_segment;
        track.segments.remove(right_idx);

        self.set_segment_visible((track_selection, left_idx), true);

        self.selected_segment = Some((track_selection, left_idx));
        self.segment_editor_open = true;
        self.status = Some("Segment removed".to_owned());
        self.toasts.success("Segment removed");
    }
}

struct GpxPolyline {
    positions: Vec<walkers::Position>,
    description: String,
    track_selection: TrackSelection,
    segment_index: usize,
    has_previous_separator: bool,
    has_next_separator: bool,
    window_highlighted: bool,
    cut_tool_enabled: bool,
    clicked_track: ClickedTrack,
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
                self.track_selection,
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
            egui::Stroke::new(4.0, Color32::from_rgb(30, 100, 190))
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
                    6.,
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
                    6.,
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
                            *remove = Some((self.track_selection, left_index));
                        }
                    }
                } else if pointer_hits_polyline(pointer_pos, &self.positions, projector) {
                    if let Ok(mut clicked) = self.clicked_track.lock() {
                        *clicked = Some(self.track_selection);
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
                                *cut = Some((self.track_selection, self.segment_index, split_idx));
                            }
                        }
                    } else if let Ok(mut clicked) = self.clicked_segment.lock() {
                        *clicked = Some((self.track_selection, self.segment_index));
                    }
                }
            }
        }
    }
}
