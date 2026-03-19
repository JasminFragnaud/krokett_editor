use super::*;

use crate::constants::Colors;

impl GpxState {
    pub(super) fn segment_matches_active_filters(&self, description: &str, comment: &str) -> bool {
        let has_active_filter = self.filter_with_description_color
            || self.filter_to_explore_color
            || self.filter_no_color_or_description;

        if !has_active_filter {
            return true;
        }

        let parsed_color = Colors::from_string(comment.trim());
        let has_description = !description.trim().is_empty();
        let no_color_or_description = parsed_color.is_none() && description.trim().is_empty();

        (self.filter_with_description_color && has_description)
            || (self.filter_to_explore_color && parsed_color == Some(Colors::SEGMENT_TO_EXPLORE))
            || (self.filter_no_color_or_description && no_color_or_description)
    }

    pub(super) fn segment_waypoints(
        &self,
        selection: SegmentSelection,
    ) -> Option<&[gpx::Waypoint]> {
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

    pub(super) fn segment_waypoints_mut(
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

    pub(super) fn segment_description(&self, selection: SegmentSelection) -> String {
        self.segment_waypoints(selection)
            .and_then(|waypoints| waypoints.first())
            .and_then(|waypoint| waypoint.description.clone())
            .unwrap_or_default()
    }

    pub(super) fn set_segment_description(
        &mut self,
        selection: SegmentSelection,
        description: String,
    ) {
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

    pub(super) fn segment_comment(&self, selection: SegmentSelection) -> String {
        self.segment_waypoints(selection)
            .and_then(|waypoints| waypoints.first())
            .and_then(|waypoint| waypoint.comment.clone())
            .unwrap_or_default()
    }

    pub(super) fn set_segment_comment(&mut self, selection: SegmentSelection, comment: String) {
        if let Some(waypoints) = self.segment_waypoints_mut(selection) {
            if let Some(first) = waypoints.first_mut() {
                first.comment = if comment.trim().is_empty() {
                    None
                } else {
                    Some(comment)
                };
            }
        }
    }

    pub(super) fn segment_positions(waypoints: &[gpx::Waypoint]) -> Vec<walkers::Position> {
        waypoints
            .iter()
            .map(|waypoint| {
                let point = waypoint.point();
                walkers::lat_lon(point.y(), point.x())
            })
            .collect()
    }

    pub(super) fn include_waypoints_in_bounds(
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
}
