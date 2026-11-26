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
                format!("{} m²", f3(n.length() * 0.5))
            }
        }
    }
}

pub fn f3(v: f32) -> String {
    format!("{v:.3}")
}

/// Unitatea de afisare a lungimilor (masuratori, distante, coordonate).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Unit { #[default] M, Mm, In, Ft }

impl Unit {
    pub fn label(self) -> &'static str {
        match self { Unit::M => "m", Unit::Mm => "mm", Unit::In => "in", Unit::Ft => "ft" }
    }
    /// factor metri -> unitate
    fn factor(self) -> f64 {
        match self { Unit::M => 1.0, Unit::Mm => 1000.0, Unit::In => 39.37007874, Unit::Ft => 3.280839895 }
    }
}

static UNIT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0); // Unit::M
static PREC: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(3); // zecimale

pub fn set_unit(u: Unit) { UNIT.store(u as u8, std::sync::atomic::Ordering::Relaxed); }
pub fn unit() -> Unit {
    match UNIT.load(std::sync::atomic::Ordering::Relaxed) { 1 => Unit::Mm, 2 => Unit::In, 3 => Unit::Ft, _ => Unit::M }
}
pub fn set_precision(p: u8) { PREC.store(p.clamp(0, 6), std::sync::atomic::Ordering::Relaxed); }
pub fn precision() -> u8 { PREC.load(std::sync::atomic::Ordering::Relaxed) }

/// compatibilitate cu setarile vechi (units_mm)
pub fn set_units_mm(mm: bool) { set_unit(if mm { Unit::Mm } else { Unit::M }); }
pub fn units_mm() -> bool { unit() == Unit::Mm }

/// Lungime (metri) in unitatea aleasa, cu sufix.
pub fn fmt_len(m: f64) -> String {
    let u = unit();
    format!("{:.*} {}", precision() as usize, m * u.factor(), u.label())
}

/// Numar fara unitate, in unitatea aleasa (ex. componentele ΔX / ΔY / ΔZ).
pub fn fmt_num(m: f64) -> String {
    format!("{:.*}", precision() as usize, m * unit().factor())
}

pub fn unit_label() -> &'static str { unit().label() }
