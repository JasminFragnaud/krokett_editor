use super::*;

use std::io::Cursor;

use walkers::MapMemory;

impl GpxState {
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
            .map_err(|err| format!("Impossible de lire {file_name} : {err}"))?;

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
            return Err(format!("Aucune trace dessinable trouvée dans {file_name}"));
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
            let message = format!("Chargé {imported_segments} segment(s) GPX");
            self.toasts.success(message.clone());
            Some(message)
        } else {
            let message = "Aucune donnée GPX importée".to_owned();
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
                    .unwrap_or_else(|| "new.gpx".to_owned())
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
                        Err(err) => Err(format!("Impossible de lire {} : {err}", path.display())),
                    }
                }
                #[cfg(target_arch = "wasm32")]
                {
                    Err(format!(
                        "Impossible de lire {file_name} : l'accès au chemin du fichier est indisponible"
                    ))
                }
            } else {
                Err(format!(
                    "Impossible de lire {file_name} : les octets du fichier sont manquants"
                ))
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
}
