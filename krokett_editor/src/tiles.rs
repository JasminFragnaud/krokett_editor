use std::collections::BTreeMap;

use egui::Context;
use walkers::sources::{Attribution, TileSource};
use walkers::{HttpOptions, HttpTiles, Tiles};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Provider {
    OpenStreetMap,
    IgnRandonnee25k,
    OpenStreetMapWithGeoportal,
    MapboxStreets,
    MapboxSatellite,
}

struct IgnRandonnee25k {
    api_key: String,
}

impl TileSource for IgnRandonnee25k {
    fn tile_url(&self, tile_id: walkers::TileId) -> String {
        format!(
            "https://data.geopf.fr/private/wmts?SERVICE=WMTS\
            &VERSION=1.0.0\
            &REQUEST=GetTile\
            &LAYER=GEOGRAPHICALGRIDSYSTEMS.MAPS.SCAN25TOUR\
            &STYLE=normal\
            &FORMAT=image/jpeg\
            &TILEMATRIXSET=PM\
            &TILEMATRIX={}\
            &TILECOL={}\
            &TILEROW={}\
            &apikey={}",
            tile_id.zoom, tile_id.x, tile_id.y, self.api_key
        )
    }

    fn attribution(&self) -> Attribution {
        Attribution {
            text: "IGN Rando",
            url: "https://www.ign.fr/",
            logo_light: None,
            logo_dark: None,
        }
    }
}

pub(crate) enum TilesKind {
    Http(HttpTiles),
}

impl AsMut<dyn Tiles> for TilesKind {
    fn as_mut(&mut self) -> &mut (dyn Tiles + 'static) {
        match self {
            TilesKind::Http(tiles) => tiles,
        }
    }
}

impl AsRef<dyn Tiles> for TilesKind {
    fn as_ref(&self) -> &(dyn Tiles + 'static) {
        match self {
            TilesKind::Http(tiles) => tiles,
        }
    }
}

fn http_options() -> HttpOptions {
    HttpOptions {
        cache: None,
        ..Default::default()
    }
}

pub(crate) fn providers(egui_ctx: Context) -> BTreeMap<Provider, Vec<TilesKind>> {
    let mut providers = BTreeMap::default();
    let ign_api_key = std::env::var("IGN_API_KEY").unwrap_or_else(|_| "ign_scan_ws".to_string());

    providers.insert(
        Provider::OpenStreetMap,
        vec![TilesKind::Http(HttpTiles::with_options(
            walkers::sources::OpenStreetMap,
            http_options(),
            egui_ctx.to_owned(),
        ))],
    );

    providers.insert(
        Provider::IgnRandonnee25k,
        vec![TilesKind::Http(HttpTiles::with_options(
            IgnRandonnee25k {
                api_key: ign_api_key.clone(),
            },
            http_options(),
            egui_ctx.to_owned(),
        ))],
    );

    providers.insert(
        Provider::OpenStreetMapWithGeoportal,
        vec![
            TilesKind::Http(HttpTiles::with_options(
                walkers::sources::OpenStreetMap,
                http_options(),
                egui_ctx.to_owned(),
            )),
            TilesKind::Http(HttpTiles::with_options(
                walkers::sources::Geoportal,
                http_options(),
                egui_ctx.to_owned(),
            )),
        ],
    );

    // Pass in a mapbox access token at compile time. May or may not be what you want to do,
    // potentially loading it from application settings instead.
    let mapbox_access_token = std::option_env!("MAPBOX_ACCESS_TOKEN");

    // We only show the mapbox map if we have an access token
    if let Some(token) = mapbox_access_token {
        providers.insert(
            Provider::MapboxStreets,
            vec![TilesKind::Http(HttpTiles::with_options(
                walkers::sources::Mapbox {
                    style: walkers::sources::MapboxStyle::Streets,
                    access_token: token.to_string(),
                    high_resolution: false,
                },
                http_options(),
                egui_ctx.to_owned(),
            ))],
        );
        providers.insert(
            Provider::MapboxSatellite,
            vec![TilesKind::Http(HttpTiles::with_options(
                walkers::sources::Mapbox {
                    style: walkers::sources::MapboxStyle::Satellite,
                    access_token: token.to_string(),
                    high_resolution: true,
                },
                http_options(),
                egui_ctx.to_owned(),
            ))],
        );
    }

    providers
}
