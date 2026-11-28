//! 5D cost: a price catalog (code, description, unit, unit price, optional budget) matched to
//! the quantified groups, a priced take-off (quantity x unit price) and a budget vs. cost
//! comparison. The catalog is a CSV or Excel table, so it can be prepared in Excel and reused.
use crate::qto::{Cell, QtoResult, QtyColumn, Sheet};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceItem {
    pub code: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub unit: String,
    pub price: f64,
    /// the planned amount for this item (0 = none)
    #[serde(default)]
    pub budget: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceCatalog {
    pub name: String,
    #[serde(default)]
    pub currency: String,
    pub items: Vec<PriceItem>,
}

/// The state kept in the project: the catalog itself (it is small) and the quantity column used.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CostSetup {
    #[serde(default)]
    pub catalog: Option<PriceCatalog>,
    /// index into `QtoSetup::columns`; 0 is fine when the column list is empty
    #[serde(default)]
    pub column: usize,
}

fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}

/// A number from text: decimal comma or point, thousands separators, currency around it.
fn num(s: &str) -> Option<f64> {
    let t: String = s
        .chars()
        .skip_while(|c| !c.is_ascii_digit() && *c != '-' && *c != '+' && *c != '.' && *c != ',')
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',' || *c == '-' || *c == '+')
        .collect();
    if t.is_empty() {
        return None;
    }
    let (dot, comma) = (t.rfind('.'), t.rfind(','));
    let t = match (dot, comma) {
        // 1.234,56 -> 1234.56
        (Some(d), Some(c)) if c > d => t.replace('.', "").replace(',', "."),
        // 1,234.56 -> 1234.56
        (Some(_), Some(_)) => t.replace(',', ""),
        // 1234,56 -> 1234.56
        (None, Some(_)) => t.replace(',', "."),
        _ => t,
    };
    t.parse::<f64>().ok().filter(|x| x.is_finite())
}

fn column(
    headers: &[String],
    exact: &[&str],
    contains: &[&str],
    skip: Option<usize>,
) -> Option<usize> {
    let ok = |i: usize| Some(i).filter(|i| Some(*i) != skip);
    headers
        .iter()
        .position(|h| exact.contains(&norm(h).as_str()))
        .and_then(ok)
        .or_else(|| {
            headers
                .iter()
                .position(|h| contains.iter().any(|n| norm(h).contains(n)))
                .and_then(ok)
        })
}

impl PriceCatalog {
    /// Reads a CSV / Excel table. The header is detected from the usual names; without one the
    /// columns are taken positionally: code, description, unit, price, budget.
    pub fn from_rows(rows: &[Vec<String>]) -> Result<PriceCatalog, String> {
        let header_at = rows.iter().position(|r| {
            r.iter().any(|c| {
                let c = norm(c);
                [
                    "code",
                    "cod",
                    "articol",
                    "price",
                    "pret",
                    "preț",
                    "unit price",
                    "unitate",
                ]
                .contains(&c.as_str())
            })
        });
        let (headers, data): (Vec<String>, &[Vec<String>]) = match header_at {
            Some(h) => (rows[h].iter().map(|c| norm(c)).collect(), &rows[h + 1..]),
            None => (
                vec![
                    "code".into(),
                    "description".into(),
                    "unit".into(),
                    "price".into(),
                    "budget".into(),
                ],
                rows,
            ),
        };
        // the price column is resolved first, so "Unit price" is not taken as the unit
        let i_price = column(
            &headers,
            &[
                "price",
                "pret",
                "preț",
                "unit price",
                "pret unitar",
                "unit_price",
                "tarif",
                "rate",
            ],
            &["price", "pret", "preț", "tarif"],
            None,
        )
        .ok_or("The price catalog has no price / pret column.")?;
        let i_code = column(
            &headers,
            &["code", "cod", "item", "articol", "position", "poz", "id"],
            &["code", "cod", "articol"],
            None,
        )
        .unwrap_or(0);
        let i_desc = column(
            &headers,
            &[
                "description",
                "descriere",
                "denumire",
                "name",
                "nume",
                "desc",
            ],
            &["descri", "denumir"],
            None,
        );
        let i_unit = column(
            &headers,
            &["unit", "um", "ud", "unitate", "u.m.", "u.m"],
            &["unitate"],
            Some(i_price),
        );
        let i_budget = column(&headers, &["budget", "buget"], &["budget", "buget"], None);
        let cell = |r: &Vec<String>, i: Option<usize>| {
            i.and_then(|i| r.get(i))
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };
        let mut items = Vec::new();
        for r in data {
            if r.iter().all(|c| c.trim().is_empty()) {
                continue;
            }
            let Some(price) = num(r.get(i_price).map(String::as_str).unwrap_or("")) else {
                continue;
            };
            let code = cell(r, Some(i_code));
            let description = cell(r, i_desc);
            if code.is_empty() && description.is_empty() {
                continue;
            }
            items.push(PriceItem {
                code,
                description,
                unit: cell(r, i_unit),
                price,
                budget: i_budget
                    .and_then(|i| r.get(i))
                    .and_then(|s| num(s))
                    .unwrap_or(0.0),
            });
        }
        if items.is_empty() {
            return Err("The price catalog has no rows with a price.".into());
        }
        Ok(PriceCatalog {
            name: String::new(),
            currency: String::new(),
            items,
        })
    }

    pub fn load(path: &str) -> Result<PriceCatalog, String> {
        let rows = crate::tables::read_rows(path)?;
        let mut c = PriceCatalog::from_rows(&rows)?;
        c.name = crate::file_name(path);
        Ok(c)
    }

    /// The best price item for a group: exact code, exact description, then containment.
    pub fn find(&self, keys: &[String]) -> Option<usize> {
        let ks: Vec<String> = keys
            .iter()
            .map(|k| norm(k))
            .filter(|k| !k.is_empty() && !k.starts_with("(no "))
            .collect();
        let mut best: Option<(u8, usize)> = None;
        for (i, it) in self.items.iter().enumerate() {
            let code = norm(&it.code);
            let desc = norm(&it.description);
            let mut score = 0u8;
            for k in &ks {
                if !code.is_empty() && *k == code {
                    score = score.max(5);
                }
                if !desc.is_empty() && *k == desc {
                    score = score.max(4);
                }
                if code.len() >= 2 && k.len() >= 2 && (k.contains(&code) || code.contains(k)) {
                    score = score.max(3);
                }
                if desc.len() >= 3 && k.len() >= 3 && (k.contains(&desc) || desc.contains(k)) {
                    score = score.max(2);
                }
            }
            if score > 0 && best.map(|(s, _)| score > s).unwrap_or(true) {
                best = Some((score, i));
            }
        }
        best.map(|(_, i)| i)
    }

    /// The catalog as CSV (UTF-8 BOM, `;` separator, decimal comma - opens in Excel).
    pub fn to_csv(&self) -> String {
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut s = String::from("\u{feff}Code;Description;Unit;Price;Budget\n");
        for it in &self.items {
            s.push_str(&format!(
                "{};{};{};{};{}\n",
                q(&it.code),
                q(&it.description),
                q(&it.unit),
                fmt_num(it.price),
                fmt_num(it.budget)
            ));
        }
        s
    }

    pub fn to_xlsx(&self) -> Vec<u8> {
        let rows: Vec<Vec<Cell>> = self
            .items
            .iter()
            .map(|it| {
                vec![
                    Cell::S(it.code.clone()),
                    Cell::S(it.description.clone()),
                    Cell::S(it.unit.clone()),
                    Cell::N(it.price),
                    Cell::N(it.budget),
                ]
            })
            .collect();
        let sheet = Sheet {
            name: "Prices".into(),
            title: format!(
                "{} - price catalog",
                if self.name.is_empty() {
                    "ManageBIM"
                } else {
                    &self.name
                }
            ),
            headers: ["Code", "Description", "Unit", "Unit price", "Budget"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            widths: vec![16.0, 44.0, 12.0, 14.0, 14.0],
            rows,
            total_from: None,
        };
        crate::qto::xlsx(&[sheet])
    }
}

fn fmt_num(x: f64) -> String {
    let s = format!("{x:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A priced group: the quantity from the take-off, the matched price item and the amounts.
#[derive(Clone, Debug, Serialize)]
pub struct PricedRow {
    pub keys: Vec<String>,
    pub code: String,
    pub description: String,
    pub quantity: Option<f64>,
    pub unit: String,
    pub price: Option<f64>,
    pub amount: Option<f64>,
    pub budget: Option<f64>,
    /// budget - amount (negative = over budget)
    pub variance: Option<f64>,
    pub matched: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PricedQto {
    pub catalog: String,
    pub currency: String,
    pub column: String,
    pub rows: Vec<PricedRow>,
    pub amount_total: f64,
    pub budget_total: f64,
    pub variance_total: f64,
    pub matched: usize,
    pub unmatched: usize,
    pub missing_quantity: usize,
}

/// The quantity column to use by default: volume, then plan area, length, surface, count,
/// then the first numeric property.
pub fn default_column(q: &QtoResult) -> usize {
    for want in [
        QtyColumn::Volume,
        QtyColumn::PlanArea,
        QtyColumn::Length,
        QtyColumn::Surface,
        QtyColumn::Count,
    ] {
        if let Some(i) = q.setup.columns.iter().position(|c| *c == want) {
            return i;
        }
    }
    q.setup
        .columns
        .iter()
        .position(|c| matches!(c, QtyColumn::Prop { .. }))
        .unwrap_or(0)
}

pub fn price(q: &QtoResult, cat: &PriceCatalog, column: usize) -> PricedQto {
    let column = column.min(q.setup.columns.len().saturating_sub(1));
    let mut out = PricedQto {
        catalog: cat.name.clone(),
        currency: cat.currency.clone(),
        column: q.column_title(column),
        ..Default::default()
    };
    for r in &q.rows {
        let quantity = r.values.get(column).copied().flatten();
        let item = cat.find(&r.keys);
        let (code, description, unit, price, budget) = match item.and_then(|i| cat.items.get(i)) {
            Some(it) => (
                it.code.clone(),
                it.description.clone(),
                it.unit.clone(),
                Some(it.price),
                Some(it.budget),
            ),
            None => (
                String::new(),
                String::new(),
                q.units.get(column).cloned().unwrap_or_default(),
                None,
                None,
            ),
        };
        let amount = match (quantity, price) {
            (Some(q), Some(p)) => Some(q * p),
            _ => None,
        };
        let variance = budget
            .filter(|b| *b != 0.0)
            .map(|b| b - amount.unwrap_or(0.0));
        out.rows.push(PricedRow {
            keys: r.keys.clone(),
            code,
            description,
            quantity,
            unit,
            price,
            amount,
            budget: budget.filter(|b| *b != 0.0),
            variance,
            matched: item.is_some(),
        });
        if item.is_some() {
            out.matched += 1;
        } else {
            out.unmatched += 1;
        }
        if quantity.is_none() {
            out.missing_quantity += 1;
        }
        out.amount_total += amount.unwrap_or(0.0);
        out.budget_total += budget.filter(|b| *b != 0.0).unwrap_or(0.0);
    }
    out.variance_total = out.budget_total - out.amount_total;
    out
}

impl PricedQto {
    pub fn to_csv(&self) -> String {
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut s = String::from(
            "\u{feff}Group;Code;Description;Quantity;Unit;Unit price;Amount;Budget;Variance\n",
        );
        for r in &self.rows {
            s.push_str(&format!(
                "{};{};{};{};{};{};{};{};{}\n",
                q(&r.keys.join(" / ")),
                q(&r.code),
                q(&r.description),
                r.quantity.map(fmt_num).unwrap_or_default(),
                q(&r.unit),
                r.price.map(fmt_num).unwrap_or_default(),
                r.amount.map(fmt_num).unwrap_or_default(),
                r.budget.map(fmt_num).unwrap_or_default(),
                r.variance.map(fmt_num).unwrap_or_default(),
            ));
        }
        s.push_str(&format!(
            "{};;;;;;{};{};{}\n",
            q("TOTAL"),
            fmt_num(self.amount_total),
            fmt_num(self.budget_total),
            fmt_num(self.variance_total)
        ));
        s
    }

    pub fn to_xlsx(&self, title: &str) -> Vec<u8> {
        let rows: Vec<Vec<Cell>> = self
            .rows
            .iter()
            .map(|r| {
                vec![
                    Cell::S(r.keys.join(" / ")),
                    Cell::S(r.code.clone()),
                    Cell::S(r.description.clone()),
                    r.quantity.map(Cell::N).unwrap_or(Cell::Empty),
                    Cell::S(r.unit.clone()),
                    r.price.map(Cell::N).unwrap_or(Cell::Empty),
                    r.amount.map(Cell::N).unwrap_or(Cell::Empty),
                    r.budget.map(Cell::N).unwrap_or(Cell::Empty),
                    r.variance.map(Cell::N).unwrap_or(Cell::Empty),
                ]
            })
            .collect();
        let sheet = Sheet {
            name: "Cost".into(),
            title: format!(
                "{title} - 5D cost ({})",
                if self.column.is_empty() {
                    "quantity"
                } else {
                    &self.column
                }
            ),
            headers: [
                "Group",
                "Code",
                "Description",
                "Quantity",
                "Unit",
                "Unit price",
                "Amount",
                "Budget",
                "Variance",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            widths: vec![34.0, 14.0, 40.0, 12.0, 10.0, 12.0, 14.0, 14.0, 14.0],
            rows,
            total_from: Some(5),
        };
        crate::qto::xlsx(&[sheet])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(text: &str) -> Vec<Vec<String>> {
        crate::tables::csv_rows(text)
    }

    #[test]
    fn reads_a_catalog_with_decimal_comma() {
        let r = rows("Cod;Denumire;UM;Pret unitar;Buget\nW01;Perete caramida;m3;\"1.250,50\";100000\nC01;Planseu beton;m2;320,75;\n");
        let c = PriceCatalog::from_rows(&r).unwrap();
        assert_eq!(c.items.len(), 2);
        assert_eq!(c.items[0].code, "W01");
        assert!((c.items[0].price - 1250.5).abs() < 1e-6);
        assert!((c.items[0].budget - 100000.0).abs() < 1e-6);
        assert!((c.items[1].price - 320.75).abs() < 1e-6);
    }

    #[test]
    fn matches_exact_code_before_description() {
        let mut c = PriceCatalog::default();
        c.items.push(PriceItem {
            code: "W01".into(),
            description: "Wall".into(),
            unit: "m3".into(),
            price: 10.0,
            budget: 0.0,
        });
        c.items.push(PriceItem {
            code: "C01".into(),
            description: "Slab".into(),
            unit: "m2".into(),
            price: 20.0,
            budget: 0.0,
        });
        assert_eq!(c.find(&["W01".to_string()]), Some(0));
        assert_eq!(c.find(&["(no ifc type)".to_string()]), None);
    }

    #[test]
    fn numbers_with_currency_and_thousands() {
        assert_eq!(num("€ 1.234,56"), Some(1234.56));
        assert_eq!(num("1,234.56 EUR"), Some(1234.56));
        assert_eq!(num("12,5"), Some(12.5));
        assert_eq!(num(""), None);
    }
}
