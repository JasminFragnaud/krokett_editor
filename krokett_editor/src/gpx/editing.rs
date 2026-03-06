use super::*;

impl GpxState {
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
            self.status =
                Some("La découpe n'est prise en charge que pour les segments de trace".to_owned());
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
            self.status = Some("Impossible de découper le segment à cette position".to_owned());
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
        self.status = Some("Segment créé".to_owned());
        self.toasts.success("Segment créé");
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
            self.status =
                Some("La fusion n'est prise en charge que pour les segments de trace".to_owned());
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
            self.status = Some("Aucun segment à fusionner".to_owned());
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
        self.status = Some("Segment supprimé".to_owned());
        self.toasts.success("Segment supprimé");
    }
}
