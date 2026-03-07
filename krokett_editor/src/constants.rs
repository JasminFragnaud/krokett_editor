use egui::Color32;

pub struct Colors;

pub const GPX_EXTENSION: &str = ".gpx";
pub const WINDOW_WIDTH: f32 = 100.;

impl Colors {
    pub const SEGMENT_HOOVER: Color32 = Color32::from_rgb(14, 214, 85);
    pub const SEGMENT_WITH_DESCRIPTION: Color32 = Color32::from_rgb(30, 100, 190);
    pub const SEGMENT_TO_EXPLORE: Color32 = Color32::from_rgb(120, 5, 240);
    pub const SEGMENT_DEFAULT: Color32 = Color32::from_rgb(255, 111, 0);

    pub fn to_string(color: Color32) -> String {
        if color == Self::SEGMENT_HOOVER {
            "SEGMENT_HOOVER".to_owned()
        } else if color == Self::SEGMENT_WITH_DESCRIPTION {
            "SEGMENT_WITH_DESCRIPTION".to_owned()
        } else if color == Self::SEGMENT_TO_EXPLORE {
            "SEGMENT_TO_EXPLORE".to_owned()
        } else if color == Self::SEGMENT_DEFAULT {
            "SEGMENT_DEFAULT".to_owned()
        } else {
            format!("{},{},{}", color.r(), color.g(), color.b())
        }
    }

    pub fn from_string(color: &str) -> Option<Color32> {
        match color {
            "SEGMENT_HOOVER" => Some(Self::SEGMENT_HOOVER),
            "SEGMENT_WITH_DESCRIPTION" => Some(Self::SEGMENT_WITH_DESCRIPTION),
            "SEGMENT_TO_EXPLORE" => Some(Self::SEGMENT_TO_EXPLORE),
            "SEGMENT_DEFAULT" => Some(Self::SEGMENT_DEFAULT),
            _ => {
                let mut parts = color.split(',').map(str::trim);
                let r = parts.next()?.parse::<u8>().ok()?;
                let g = parts.next()?.parse::<u8>().ok()?;
                let b = parts.next()?.parse::<u8>().ok()?;

                if parts.next().is_some() {
                    return None;
                }

                Some(Color32::from_rgb(r, g, b))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Colors;
    use egui::Color32;

    #[test]
    fn named_colors_parse_and_serialize() {
        assert_eq!(
            Colors::from_string("SEGMENT_HOOVER"),
            Some(Colors::SEGMENT_HOOVER)
        );
        assert_eq!(
            Colors::from_string("SEGMENT_WITH_DESCRIPTION"),
            Some(Colors::SEGMENT_WITH_DESCRIPTION)
        );
        assert_eq!(
            Colors::from_string("SEGMENT_TO_EXPLORE"),
            Some(Colors::SEGMENT_TO_EXPLORE)
        );
        assert_eq!(
            Colors::from_string("SEGMENT_DEFAULT"),
            Some(Colors::SEGMENT_DEFAULT)
        );

        assert_eq!(Colors::to_string(Colors::SEGMENT_HOOVER), "SEGMENT_HOOVER");
        assert_eq!(
            Colors::to_string(Colors::SEGMENT_WITH_DESCRIPTION),
            "SEGMENT_WITH_DESCRIPTION"
        );
        assert_eq!(
            Colors::to_string(Colors::SEGMENT_TO_EXPLORE),
            "SEGMENT_TO_EXPLORE"
        );
        assert_eq!(
            Colors::to_string(Colors::SEGMENT_DEFAULT),
            "SEGMENT_DEFAULT"
        );
    }

    #[test]
    fn unknown_color_serializes_to_rgb_and_parses_back() {
        let color = Color32::from_rgb(1, 2, 3);
        let encoded = Colors::to_string(color);

        assert_eq!(encoded, "1,2,3");
        assert_eq!(Colors::from_string(&encoded), Some(color));
    }

    #[test]
    fn invalid_strings_return_none() {
        assert_eq!(Colors::from_string(""), None);
        assert_eq!(Colors::from_string("1,2"), None);
        assert_eq!(Colors::from_string("1,2,3,4"), None);
        assert_eq!(Colors::from_string("a,b,c"), None);
    }
}
