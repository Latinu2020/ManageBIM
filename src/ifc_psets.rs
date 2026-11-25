//! Seturile de proprietati standard IFC4 (buildingSMART) cele mai folosite, cu tipul fiecarei
//! proprietati: fereastra Property editor le propune dupa clasa IFC a elementelor selectate, iar
//! exportul IFC le scrie cu tipul corect (IfcBoolean, IfcThermalTransmittanceMeasure...) in loc de text.

/// Tipul unei proprietati (determina controlul din fereastra si valoarea STEP din export).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PKind {
    Label,
    Text,
    Identifier,
    Bool,
    Real,
    Integer,
    /// W/(m²·K)
    Thermal,
    /// 0..1
    Ratio,
    /// grade
    Angle,
    /// m²
    Area,
    /// ani
    Years,
    Date,
    Enum(&'static [&'static str]),
}

impl PKind {
    pub fn hint(&self) -> &'static str {
        match self {
            PKind::Label => "text",
            PKind::Text => "long text",
            PKind::Identifier => "code / reference",
            PKind::Bool => "TRUE / FALSE",
            PKind::Real => "number",
            PKind::Integer => "whole number",
            PKind::Thermal => "U-value, W/(m²·K)",
            PKind::Ratio => "0 – 1",
            PKind::Angle => "degrees",
            PKind::Area => "m²",
            PKind::Years => "years",
            PKind::Date => "YYYY-MM-DD",
            PKind::Enum(_) => "choose",
        }
    }
}

pub struct PDef {
    pub name: &'static str,
    pub kind: PKind,
}

pub struct PsetDef {
    pub name: &'static str,
    /// clasele IFC carora li se aplica ("*" = oricarui element)
    pub classes: &'static [&'static str],
    pub props: &'static [PDef],
}

const STATUS: PKind = PKind::Enum(&["NEW", "EXISTING", "DEMOLISH", "TEMPORARY", "OTHER", "NOTKNOWN", "UNSET"]);

macro_rules! p {
    ($n:expr, $k:expr) => {
        PDef { name: $n, kind: $k }
    };
}

use PKind::*;

pub const PSETS: &[PsetDef] = &[
    PsetDef {
        name: "Pset_WallCommon",
        classes: &["IfcWall", "IfcWallStandardCase", "IfcCurtainWall"],
        props: &[
            p!("Reference", Identifier),
            p!("Status", STATUS),
            p!("IsExternal", Bool),
            p!("LoadBearing", Bool),
            p!("FireRating", Label),
            p!("AcousticRating", Label),
            p!("ThermalTransmittance", Thermal),
            p!("Combustible", Bool),
            p!("SurfaceSpreadOfFlame", Label),
            p!("ExtendToStructure", Bool),
            p!("Compartmentation", Bool),
        ],
    },
    PsetDef {
        name: "Pset_SlabCommon",
        classes: &["IfcSlab"],
        props: &[
            p!("Reference", Identifier),
            p!("Status", STATUS),
            p!("IsExternal", Bool),
            p!("LoadBearing", Bool),
            p!("FireRating", Label),
            p!("AcousticRating", Label),
            p!("ThermalTransmittance", Thermal),
            p!("Combustible", Bool),
            p!("SurfaceSpreadOfFlame", Label),
            p!("PitchAngle", Angle),
            p!("Compartmentation", Bool),
        ],
    },
    PsetDef {
        name: "Pset_RoofCommon",
        classes: &["IfcRoof"],
        props: &[p!("Reference", Identifier), p!("Status", STATUS), p!("IsExternal", Bool), p!("FireRating", Label), p!("AcousticRating", Label), p!("ThermalTransmittance", Thermal)],
    },
    PsetDef {
        name: "Pset_BeamCommon",
        classes: &["IfcBeam"],
        props: &[p!("Reference", Identifier), p!("Status", STATUS), p!("IsExternal", Bool), p!("LoadBearing", Bool), p!("FireRating", Label), p!("Slope", Angle), p!("Roll", Angle), p!("ThermalTransmittance", Thermal)],
    },
    PsetDef {
        name: "Pset_ColumnCommon",
        classes: &["IfcColumn"],
        props: &[p!("Reference", Identifier), p!("Status", STATUS), p!("IsExternal", Bool), p!("LoadBearing", Bool), p!("FireRating", Label), p!("Slope", Angle), p!("Roll", Angle), p!("ThermalTransmittance", Thermal)],
    },
    PsetDef {
        name: "Pset_MemberCommon",
        classes: &["IfcMember"],
        props: &[p!("Reference", Identifier), p!("Status", STATUS), p!("IsExternal", Bool), p!("LoadBearing", Bool), p!("FireRating", Label), p!("Slope", Angle), p!("Roll", Angle)],
    },
    PsetDef {
        name: "Pset_PlateCommon",
        classes: &["IfcPlate"],
        props: &[p!("Reference", Identifier), p!("Status", STATUS), p!("IsExternal", Bool), p!("LoadBearing", Bool), p!("FireRating", Label), p!("AcousticRating", Label), p!("ThermalTransmittance", Thermal)],
    },
    PsetDef {
        name: "Pset_DoorCommon",
        classes: &["IfcDoor"],
        props: &[
            p!("Reference", Identifier),
            p!("Status", STATUS),
            p!("IsExternal", Bool),
            p!("FireRating", Label),
            p!("AcousticRating", Label),
            p!("SecurityRating", Label),
            p!("ThermalTransmittance", Thermal),
            p!("GlazingAreaFraction", Ratio),
            p!("HandicapAccessible", Bool),
            p!("FireExit", Bool),
            p!("SelfClosing", Bool),
            p!("SmokeStop", Bool),
            p!("HasDrive", Bool),
            p!("WaterTightnessRating", Label),
            p!("WindLoadRating", Label),
        ],
    },
    PsetDef {
        name: "Pset_WindowCommon",
        classes: &["IfcWindow"],
        props: &[
            p!("Reference", Identifier),
            p!("Status", STATUS),
            p!("IsExternal", Bool),
            p!("FireRating", Label),
            p!("AcousticRating", Label),
            p!("SecurityRating", Label),
            p!("ThermalTransmittance", Thermal),
            p!("GlazingAreaFraction", Ratio),
            p!("HasSillExternal", Bool),
            p!("HasSillInternal", Bool),
            p!("HasDrive", Bool),
            p!("SmokeStop", Bool),
            p!("FireExit", Bool),
            p!("WaterTightnessRating", Label),
            p!("WindLoadRating", Label),
        ],
    },
    PsetDef {
        name: "Pset_StairCommon",
        classes: &["IfcStair", "IfcStairFlight"],
        props: &[p!("Reference", Identifier), p!("Status", STATUS), p!("NumberOfRiser", Integer), p!("NumberOfTreads", Integer), p!("FireRating", Label), p!("HandicapAccessible", Bool), p!("FireExit", Bool), p!("HasNonSkidSurface", Bool)],
    },
    PsetDef {
        name: "Pset_RailingCommon",
        classes: &["IfcRailing"],
        props: &[p!("Reference", Identifier), p!("Status", STATUS), p!("IsExternal", Bool)],
    },
    PsetDef {
        name: "Pset_CoveringCommon",
        classes: &["IfcCovering"],
        props: &[p!("Reference", Identifier), p!("Status", STATUS), p!("IsExternal", Bool), p!("FireRating", Label), p!("AcousticRating", Label), p!("FlammabilityRating", Label), p!("Combustible", Bool), p!("SurfaceSpreadOfFlame", Label), p!("Finish", Label), p!("ThermalTransmittance", Thermal)],
    },
    PsetDef {
        name: "Pset_SpaceCommon",
        classes: &["IfcSpace"],
        props: &[p!("Reference", Identifier), p!("IsExternal", Bool), p!("GrossPlannedArea", Area), p!("NetPlannedArea", Area), p!("PubliclyAccessible", Bool), p!("HandicapAccessible", Bool)],
    },
    PsetDef {
        name: "Pset_BuildingElementProxyCommon",
        classes: &["IfcBuildingElementProxy"],
        props: &[p!("Reference", Identifier), p!("Status", STATUS), p!("IsExternal", Bool), p!("LoadBearing", Bool), p!("FireRating", Label), p!("ThermalTransmittance", Thermal)],
    },
    PsetDef {
        name: "Pset_PipeSegmentTypeCommon",
        classes: &["IfcPipeSegment", "IfcPipeFitting"],
        props: &[p!("Reference", Identifier), p!("Status", STATUS), p!("WorkingPressure", Real), p!("NominalDiameter", Real), p!("Gradient", Real)],
    },
    PsetDef {
        name: "Pset_DuctSegmentTypeCommon",
        classes: &["IfcDuctSegment", "IfcDuctFitting"],
        props: &[p!("Reference", Identifier), p!("Status", STATUS), p!("Shape", PKind::Enum(&["FLATOVAL", "RECTANGULAR", "ROUND", "OTHER", "NOTKNOWN", "UNSET"])), p!("WorkingPressure", Real)],
    },
    PsetDef {
        name: "Pset_CableCarrierSegmentTypeCommon",
        classes: &["IfcCableCarrierSegment", "IfcCableCarrierFitting"],
        props: &[p!("Reference", Identifier), p!("Status", STATUS)],
    },
    // ---- pentru orice element (predare / FM / mediu)
    PsetDef {
        name: "Pset_ManufacturerTypeInformation",
        classes: &["*"],
        props: &[
            p!("Manufacturer", Label),
            p!("ModelLabel", Label),
            p!("ModelReference", Label),
            p!("ArticleNumber", Identifier),
            p!("GlobalTradeItemNumber", Identifier),
            p!("ProductionYear", Label),
            p!("AssemblyPlace", PKind::Enum(&["FACTORY", "OFFSITE", "SITE", "OTHER", "NOTKNOWN", "UNSET"])),
        ],
    },
    PsetDef {
        name: "Pset_ManufacturerOccurrence",
        classes: &["*"],
        props: &[p!("SerialNumber", Identifier), p!("BatchReference", Identifier), p!("BarCode", Identifier), p!("AcquisitionDate", Date)],
    },
    PsetDef {
        name: "Pset_Warranty",
        classes: &["*"],
        props: &[p!("WarrantyIdentifier", Identifier), p!("WarrantyStartDate", Date), p!("WarrantyEndDate", Date), p!("IsExtendedWarranty", Bool), p!("WarrantyPeriod", Label), p!("WarrantyContent", Text), p!("PointOfContact", Label)],
    },
    PsetDef {
        name: "Pset_Condition",
        classes: &["*"],
        props: &[p!("AssessmentDate", Date), p!("AssessmentCondition", Label), p!("AssessmentDescription", Text)],
    },
    PsetDef {
        name: "Pset_ServiceLife",
        classes: &["*"],
        props: &[p!("ServiceLifeDuration", Years), p!("MeanTimeBetweenFailure", Years)],
    },
    PsetDef {
        name: "Pset_EnvironmentalImpactIndicators",
        classes: &["*"],
        props: &[p!("Reference", Identifier), p!("FunctionalUnitReference", Label), p!("Unit", Label), p!("ExpectedServiceLife", Years), p!("ClimateChangePerUnit", Real), p!("TotalPrimaryEnergyConsumptionPerUnit", Real), p!("WaterConsumptionPerUnit", Real)],
    },
];

/// Seturile potrivite pentru clasa IFC data (cele specifice clasei intai, apoi cele generale).
pub fn psets_for(class: &str) -> Vec<&'static PsetDef> {
    let c = class.trim();
    let mut out: Vec<&'static PsetDef> = PSETS.iter().filter(|p| p.classes.iter().any(|k| k.eq_ignore_ascii_case(c) || (c.len() > 3 && c.to_ascii_lowercase().starts_with(&k.to_ascii_lowercase()) && *k != "*"))).collect();
    out.extend(PSETS.iter().filter(|p| p.classes == ["*"]));
    out
}

pub fn find(pset: &str, prop: &str) -> Option<&'static PDef> {
    PSETS.iter().find(|p| p.name.eq_ignore_ascii_case(pset))?.props.iter().find(|d| d.name.eq_ignore_ascii_case(prop))
}

fn num(v: &str) -> Option<f64> {
    v.trim().replace(',', ".").parse::<f64>().ok().filter(|x| x.is_finite())
}

/// numar STEP: mereu cu punct zecimal (1. / 0.25 / 1.E-5)
fn step_real(x: f64) -> String {
    let s = format!("{x}");
    if s.contains('.') || s.contains('e') || s.contains('E') { s.replace('e', "E") } else { format!("{s}.") }
}

pub fn parse_bool(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "true" | "t" | "yes" | "y" | "1" | "da" | ".t." => Some(true),
        "false" | "f" | "no" | "n" | "0" | "nu" | ".f." => Some(false),
        _ => None,
    }
}

/// Valoarea STEP tipizata pentru o proprietate standard (None = nu e standard sau valoarea nu se
/// potriveste tipului: exportul o scrie atunci ca IfcLabel, nu se pierde).
pub fn step_value(pset: &str, prop: &str, v: &str, esc: &dyn Fn(&str) -> String) -> Option<String> {
    let d = find(pset, prop)?;
    Some(match d.kind {
        Bool => format!("IFCBOOLEAN({})", if parse_bool(v)? { ".T." } else { ".F." }),
        Real => format!("IFCREAL({})", step_real(num(v)?)),
        Integer => format!("IFCINTEGER({})", v.trim().parse::<i64>().ok()?),
        Thermal => format!("IFCTHERMALTRANSMITTANCEMEASURE({})", step_real(num(v)?)),
        Ratio => format!("IFCNORMALISEDRATIOMEASURE({})", step_real(num(v)?.clamp(0.0, 1.0))),
        Angle => format!("IFCPLANEANGLEMEASURE({})", step_real(num(v)?)),
        Area => format!("IFCAREAMEASURE({})", step_real(num(v)?)),
        Years => format!("IFCREAL({})", step_real(num(v)?)),
        Identifier => format!("IFCIDENTIFIER({})", esc(v)),
        Date => format!("IFCDATE({})", esc(v.trim())),
        Text => format!("IFCTEXT({})", esc(v)),
        Label | Enum(_) => format!("IFCLABEL({})", esc(v)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_values() {
        let e = |s: &str| format!("'{s}'");
        assert_eq!(step_value("Pset_WallCommon", "IsExternal", "yes", &e).as_deref(), Some("IFCBOOLEAN(.T.)"));
        assert_eq!(step_value("Pset_WallCommon", "ThermalTransmittance", "0,25", &e).as_deref(), Some("IFCTHERMALTRANSMITTANCEMEASURE(0.25)"));
        assert_eq!(step_value("Pset_StairCommon", "NumberOfRiser", "18", &e).as_deref(), Some("IFCINTEGER(18)"));
        assert_eq!(step_value("Pset_WallCommon", "LoadBearing", "maybe", &e), None);
        assert_eq!(step_value("My set", "X", "1", &e), None);
        assert_eq!(step_value("Pset_WallCommon", "FireRating", "REI 60", &e).as_deref(), Some("IFCLABEL('REI 60')"));
        assert_eq!(step_real(2.0), "2.");
    }

    #[test]
    fn psets_by_class() {
        let w: Vec<&str> = psets_for("IfcWallStandardCase").iter().map(|p| p.name).collect();
        assert_eq!(w[0], "Pset_WallCommon");
        assert!(w.contains(&"Pset_ManufacturerTypeInformation"));
        assert!(!w.contains(&"Pset_DoorCommon"));
    }
}
