mod editors;
mod import_io;
mod plugins;
mod segments;
mod tracks;
mod tree;
mod polyline;
mod editing;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use egui_notify::{Anchor, Toasts};
use walkers::Map;

use crate::constants::GPX_EXTENSION;

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
    filter_with_description_color: bool,
    filter_to_explore_color: bool,
    filter_no_color_or_description: bool,
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
            filter_with_description_color: false,
            filter_to_explore_color: false,
            filter_no_color_or_description: false,
        }
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
            .unwrap_or_else(|| "traces".to_owned());

        if base_name.to_ascii_lowercase().ends_with(GPX_EXTENSION) {
            base_name
        } else {
            format!("{base_name}{GPX_EXTENSION}")
        }
    }

    pub(crate) fn export_gpx_bytes(&self) -> Result<Vec<u8>, String> {
        if self.gpx_documents.is_empty() {
            return Err("Pas de GPX à sauvegarder".to_owned());
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
            .map_err(|err| format!("Échec de la sérialisation des données GPX : {err}"))?;

        Ok(bytes)
    }

    pub(crate) fn clear(&mut self) {
        self.gpx_documents.clear();
        self.track_visibility.clear();
        self.segment_visibility.clear();
        self.status = Some("Supprimer les GPX".to_owned());
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
}
