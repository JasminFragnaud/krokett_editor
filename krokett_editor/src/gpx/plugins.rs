use crate::gpx::polyline::GpxPolyline;

use super::*;

use walkers::Map;

impl GpxState {
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
                    let comment = segment
                        .points
                        .first()
                        .and_then(|waypoint| waypoint.comment.clone())
                        .unwrap_or_default();

                    if !self.segment_matches_active_filters(&description, &comment) {
                        continue;
                    }

                    map = map.with_plugin(GpxPolyline {
                        positions,
                        description,
                        comment,
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
                let comment = route
                    .points
                    .first()
                    .and_then(|waypoint| waypoint.comment.clone())
                    .unwrap_or_default();

                if !self.segment_matches_active_filters(&description, &comment) {
                    continue;
                }

                map = map.with_plugin(GpxPolyline {
                    positions,
                    description,
                    comment,
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
}
