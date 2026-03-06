use super::*;

impl GpxState {
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

    pub(super) fn source_for_file(&self, file_index: usize) -> String {
        self.gpx_documents
            .get(file_index)
            .and_then(|document| document.metadata.as_ref())
            .and_then(|metadata| metadata.name.clone())
            .unwrap_or_else(|| format!("GPX {}", file_index + 1))
    }

    pub(super) fn file_track_selections(&self, file_index: usize) -> Vec<TrackSelection> {
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

    pub(super) fn segment_count(&self, track_selection: TrackSelection) -> Option<usize> {
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

    pub(super) fn track_name(&self, track_selection: TrackSelection) -> Option<String> {
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

    pub(super) fn track_description(&self, track_selection: TrackSelection) -> Option<String> {
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

    pub(super) fn set_track_metadata(
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

    pub(super) fn is_track_visible(&self, track_selection: TrackSelection) -> bool {
        self.track_visibility
            .get(&track_selection)
            .copied()
            .unwrap_or(true)
    }

    pub(super) fn set_track_visible(&mut self, track_selection: TrackSelection, visible: bool) {
        self.track_visibility.insert(track_selection, visible);
        if let Some(segment_count) = self.segment_count(track_selection) {
            for segment_index in 0..segment_count {
                self.segment_visibility
                    .insert((track_selection, segment_index), visible);
            }
        }
    }

    pub(super) fn is_segment_visible(&self, segment_selection: SegmentSelection) -> bool {
        self.segment_visibility
            .get(&segment_selection)
            .copied()
            .unwrap_or(true)
    }

    pub(super) fn set_segment_visible(&mut self, segment_selection: SegmentSelection, visible: bool) {
        self.segment_visibility.insert(segment_selection, visible);
    }
}
