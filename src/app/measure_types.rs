//! Tools and measurements: the active tool, measure kinds and the display units (m / mm / in / ft).
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,
    /// selectie cu fereastra: stanga->dreapta = complet in interior, dreapta->stanga = atinge (ca AutoCAD)
    Window,
    Measure(MeasureKind),
    Redline(crate::markup::RedKind),
    RedErase,
    /// click pe podea: puncte ale traseului de mers
    WalkPath,
    /// click in 3D: plaseaza o lumina punctuala pe suprafata aleasa
    PlaceLight,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum MeasureKind {
    PointToPoint,
    Polyline,
    Angle,
    Area,
}

impl MeasureKind {
    pub fn label(&self) -> &'static str {
        match self {
            MeasureKind::PointToPoint => "Point to Point",
            MeasureKind::Polyline => "Point Line",
            MeasureKind::Angle => "Angle",
            MeasureKind::Area => "Area",
        }
    }
    pub fn needed(&self) -> Option<usize> {
        match self {
            MeasureKind::PointToPoint => Some(2),
            MeasureKind::Angle => Some(3),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Measure {
    pub kind: MeasureKind,
    pub pts: Vec<[f32; 3]>,
}

impl Measure {
    pub fn value(&self) -> String {
        let p: Vec<Vec3> = self.pts.iter().map(|p| Vec3::from(*p)).collect();
        match self.kind {
            MeasureKind::PointToPoint | MeasureKind::Polyline => {
                if p.len() < 2 {
                    return String::new();
                }
                let tot: f32 = p.windows(2).map(|w| (w[1] - w[0]).length()).sum();
                if self.kind == MeasureKind::PointToPoint {
                    let d = p[1] - p[0];
                    format!("{}  (ΔX {}  ΔY {}  ΔZ {})", fmt_len(tot as f64), fmt_num(d.x as f64), fmt_num(d.y as f64), fmt_num(d.z as f64))
                } else {
                    format!("{}  ({} segments)", fmt_len(tot as f64), p.len() - 1)
                }
            }
            MeasureKind::Angle => {
                if p.len() < 3 {
                    return String::new();
                }
                let a = (p[0] - p[1]).normalize_or_zero();
                let b = (p[2] - p[1]).normalize_or_zero();
                format!("{:.2}°", a.dot(b).clamp(-1.0, 1.0).acos().to_degrees())
            }
            MeasureKind::Area => {
                if p.len() < 3 {
                    return String::new();
                }
                let mut n = Vec3::ZERO;
                for i in 0..p.len() {
                    n += p[i].cross(p[(i + 1) % p.len()]);
                }
                fmt_area((n.length() * 0.5) as f64)
            }
        }
    }
}

pub fn f3(v: f32) -> String {
    format!("{v:.3}")
}

/// Display unit for lengths (measurements, distances, coordinates).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Unit { #[default] M, Mm, In, Ft, Cm, FtIn }

impl Unit {
    /// every unit, in the order shown in the pickers: (code saved in the settings, label)
    pub const ALL: [(&'static str, &'static str); 6] = [("m", "m"), ("cm", "cm"), ("mm", "mm"), ("ft-in", "ft-in  (10' 4 1/2\")"), ("ft", "ft"), ("in", "in")];

    pub fn label(self) -> &'static str {
        match self { Unit::M => "m", Unit::Mm => "mm", Unit::In => "in", Unit::Ft | Unit::FtIn => "ft", Unit::Cm => "cm" }
    }
    pub fn code(self) -> &'static str {
        match self { Unit::M => "m", Unit::Mm => "mm", Unit::In => "in", Unit::Ft => "ft", Unit::Cm => "cm", Unit::FtIn => "ft-in" }
    }
    pub fn from_code(c: &str) -> Unit {
        match c { "mm" => Unit::Mm, "in" => Unit::In, "ft" => Unit::Ft, "cm" => Unit::Cm, "ft-in" => Unit::FtIn, _ => Unit::M }
    }
    /// metres -> unit
    fn factor(self) -> f64 {
        match self { Unit::M => 1.0, Unit::Mm => 1000.0, Unit::Cm => 100.0, Unit::In => 39.37007874, Unit::Ft | Unit::FtIn => 3.280839895 }
    }
    fn imperial(self) -> bool {
        matches!(self, Unit::In | Unit::Ft | Unit::FtIn)
    }
}

static UNIT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0); // Unit::M
static PREC: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(3); // decimals

pub fn set_unit(u: Unit) { UNIT.store(u as u8, std::sync::atomic::Ordering::Relaxed); }
pub fn unit() -> Unit {
    match UNIT.load(std::sync::atomic::Ordering::Relaxed) { 1 => Unit::Mm, 2 => Unit::In, 3 => Unit::Ft, 4 => Unit::Cm, 5 => Unit::FtIn, _ => Unit::M }
}
pub fn set_precision(p: u8) { PREC.store(p.clamp(0, 6), std::sync::atomic::Ordering::Relaxed); }
pub fn precision() -> u8 { PREC.load(std::sync::atomic::Ordering::Relaxed) }

/// compatibility with the old settings (units_mm)
pub fn set_units_mm(mm: bool) { set_unit(if mm { Unit::Mm } else { Unit::M }); }
pub fn units_mm() -> bool { unit() == Unit::Mm }

/// Feet and inches with the inches rounded to 1/16 (e.g. 10' 4 1/2", -2' 0 3/16").
pub fn fmt_ft_in(m: f64) -> String {
    let sign = if m < 0.0 { "-" } else { "" };
    let sixteenths = (m.abs() * 39.37007874 * 16.0).round() as i64;
    let (feet, rest) = (sixteenths / (12 * 16), sixteenths % (12 * 16));
    let (inch, frac) = (rest / 16, rest % 16);
    let frac = if frac == 0 {
        String::new()
    } else {
        let g = [8, 4, 2, 1].into_iter().find(|g| frac % g == 0).unwrap_or(1);
        format!(" {}/{}", frac / g, 16 / g)
    };
    format!("{sign}{feet}' {inch}{frac}\"")
}

/// Length (metres) in the chosen unit, with its suffix.
pub fn fmt_len(m: f64) -> String {
    let u = unit();
    if u == Unit::FtIn {
        return fmt_ft_in(m);
    }
    format!("{:.*} {}", precision() as usize, m * u.factor(), u.label())
}

/// Number without the unit, in the chosen unit (e.g. the ΔX / ΔY / ΔZ components).
pub fn fmt_num(m: f64) -> String {
    if unit() == Unit::FtIn {
        return fmt_ft_in(m);
    }
    format!("{:.*}", precision() as usize, m * unit().factor())
}

/// Area (m²): m² for the metric units, ft² for the imperial ones.
pub fn fmt_area(m2: f64) -> String {
    if unit().imperial() {
        format!("{:.*} ft²", precision().min(2) as usize, m2 * 10.763910417)
    } else {
        format!("{:.*} m²", precision().min(3) as usize, m2)
    }
}

pub fn unit_label() -> &'static str { unit().label() }

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn feet_inches_and_areas() {
        assert_eq!(fmt_ft_in(0.0), "0' 0\"");
        assert_eq!(fmt_ft_in(3.1623), "10' 4 1/2\"");
        assert_eq!(fmt_ft_in(-0.3048), "-1' 0\"");
        assert_eq!(Unit::from_code("ft-in"), Unit::FtIn);
        for (c, _) in Unit::ALL {
            assert_eq!(Unit::from_code(c).code(), c);
        }
    }
}
