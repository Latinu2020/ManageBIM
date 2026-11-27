//! Model View Definition (MVD) for IFC export.
//! Each MVD decides which properties are written and whether only specific IFC categories are kept.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Mvd {
    /// all properties, maximum compatibility (default)
    #[default]
    Reference,
    /// design transfer: basic properties + materials and quantities
    DesignTransfer,
    /// coordination / clash: identity, level, system, material
    Coordination,
    /// site / surveying: only the site and the levels
    Site,
}

impl Mvd {
    pub const ALL: [Mvd; 4] = [Mvd::Reference, Mvd::DesignTransfer, Mvd::Coordination, Mvd::Site];

    pub fn code(self) -> &'static str {
        match self {
            Self::Reference => "Reference View",
            Self::DesignTransfer => "Design Transfer View",
            Self::Coordination => "Coordination View 2.0",
            Self::Site => "Site View",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Reference => "Reference View (all properties)",
            Self::DesignTransfer => "Design Transfer View (design + quantities)",
            Self::Coordination => "Coordination View 2.0 (clash / systems)",
            Self::Site => "Site View (location only)",
        }
    }

    pub fn tip(self) -> &'static str {
        match self {
            Self::Reference => "All property sets, names, GUIDs, levels and colours. Best for round-trip and other apps.",
            Self::DesignTransfer => "Adds material and quantity properties; keeps the full geometry.",
            Self::Coordination => "Keeps GlobalId, level, system, material and type - the fields used by clash detection.",
            Self::Site => "Only the site, building, storeys and element placement - no property sets.",
        }
    }

    /// The view name written in the IFC header (FILE_SCHEMA /.ApplicationContext)
    pub fn view_name(self) -> &'static str {
        match self {
            Self::Reference => "Reference View",
            Self::DesignTransfer => "Design Transfer View",
            Self::Coordination => "Coordination View 2.0",
            Self::Site => "Site View",
        }
    }

    /// decides whether a property goes into the file
    pub fn keep_prop(self, group: &str, name: &str) -> bool {
        let g = group.to_ascii_lowercase();
        let n = name.to_ascii_lowercase();
        match self {
            Self::Reference => true,
            Self::Site => false,
            Self::DesignTransfer => {
                !g.contains("managbim") && !g.contains("managebim") || n.contains("material") || n.contains("volume") || n.contains("area") || n.contains("length") || n.contains("guid")
            }
            Self::Coordination => {
                n.contains("globalid") || n.contains("guid") || n.contains("name") || n.contains("level") || n.contains("storey") || n.contains("system") || n.contains("material") || n.contains("type") || g.contains("pset_wallcommon") || g.contains("pset_occupiedspace") || g.contains("managbim") || g.contains("managebim")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtering() {
        assert!(Mvd::Reference.keep_prop("Pset_WallCommon", "FireRating"));
        assert!(!Mvd::Site.keep_prop("Pset_WallCommon", "FireRating"));
        assert!(Mvd::Coordination.keep_prop("ManageBIM", "Source GUID"));
        assert!(Mvd::DesignTransfer.keep_prop("Qto_WallBaseQuantities", "NetVolume"));
    }
}
