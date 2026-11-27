//! The Rhai script engine of the Script console.
//!
//! The script does not receive `&mut App` (Rhai cannot borrow something like that); instead
//!it receives a snapshot of the model (`Input`) and returns a list of `Action`
//! that the app applies after execution, in order.

use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// select_where("Category", "Walls") -> new selection
    SelectWhere(String, String),
    /// select_ids([...]) - global indices from Input
    SelectIds(Vec<i32>),
    /// set_prop("FireRating", "EI30") on the current selection
    SetProp(String, String),
    CreateClash(String, f64),
    ZoomToSelection,
    ExportBcf(String),
    ExportExcel(String),
}

#[derive(Clone, Default)]
pub struct Input {
    /// (global index, properties). The keys: "Category", "Name", "Level", "GlobalId",
    /// the property name and "group|property" (e.g. "Pset_WallCommon|FireRating").
    pub elements: Vec<(i32, Vec<(String, String)>)>,
    /// the global indices selected at this moment
    pub selected: Vec<i32>,
}

#[derive(Clone, Default)]
pub struct Output {
    pub logs: Vec<String>,
    pub actions: Vec<Action>,
}

/// Runs a script and returns the actions to apply.
pub fn run(source: &str, input: &Input) -> Result<Output, String> {
    let out = Rc::new(RefCell::new(Output::default()));
    let mut engine = rhai::Engine::new();
    // a mistakenly written `loop {}` no longer blocks the app: the script stops with an error
    engine.set_max_operations(50_000_000);
    engine.set_max_call_levels(64);
    engine.set_max_string_size(10_000_000);
    engine.set_max_array_size(5_000_000);

    {
        let o = out.clone();
        engine.register_fn("log", move |msg: &str| {
            o.borrow_mut().logs.push(msg.to_string());
        });
    }
    {
        let n = input.selected.len() as i32;
        engine.register_fn("selected_count", move || n);
    }
    {
        let elems = input.elements.clone();
        engine.register_fn("find_where_empty", move |key: &str| -> rhai::Array {
            elems
                .iter()
                .filter(|(_, props)| !props.iter().any(|(k, v)| k.eq_ignore_ascii_case(key) && !v.trim().is_empty()))
                .map(|(i, _)| rhai::Dynamic::from(*i))
                .collect()
        });
    }
    {
        let elems = input.elements.clone();
        let o = out.clone();
        engine.register_fn("select_where", move |key: &str, value: &str| -> i32 {
            let ids = select_matching(&elems, key, value);
            let n = ids.len() as i32;
            o.borrow_mut().actions.push(Action::SelectIds(ids));
            n
        });
    }
    {
        let o = out.clone();
        engine.register_fn("select_ids", move |ids: rhai::Array| {
            // Rhai literals are i64 ([1, 2, 3]); find_where_empty returns i32 - we accept both
            let ids: Vec<i32> = ids.iter().filter_map(|d| d.as_int().ok().and_then(|x| i32::try_from(x).ok()).or_else(|| d.clone().try_cast::<i32>())).collect();
            o.borrow_mut().actions.push(Action::SelectIds(ids));
        });
        // `len()` for arrays (Rhai with reduced features does not register it)
        engine.register_fn("len", |a: rhai::Array| -> i64 { a.len() as i64 });
    }
    {
        let o = out.clone();
        engine.register_fn("set_prop", move |name: &str, value: &str| {
            o.borrow_mut().actions.push(Action::SetProp(name.to_string(), value.to_string()));
        });
    }
    {
        let o = out.clone();
        engine.register_fn("create_clash_test", move |name: &str, tol: f64| {
            o.borrow_mut().actions.push(Action::CreateClash(name.to_string(), tol));
        });
    }
    {
        let o = out.clone();
        engine.register_fn("zoom_to_selection", move || {
            o.borrow_mut().actions.push(Action::ZoomToSelection);
        });
    }
    {
        let o = out.clone();
        engine.register_fn("export_bcf", move |path: &str| {
            o.borrow_mut().actions.push(Action::ExportBcf(path.to_string()));
        });
    }
    {
        let o = out.clone();
        engine.register_fn("export_excel", move |path: &str| {
            o.borrow_mut().actions.push(Action::ExportExcel(path.to_string()));
        });
    }

    engine.run(source).map_err(|e| e.to_string())?;
    let result = out.borrow().clone();
    Ok(result)
}

fn select_matching(elems: &[(i32, Vec<(String, String)>)], key: &str, value: &str) -> Vec<i32> {
    elems
        .iter()
        .filter(|(_, props)| props.iter().any(|(k, v)| k.eq_ignore_ascii_case(key) && v.eq_ignore_ascii_case(value)))
        .map(|(i, _)| *i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> Input {
        Input {
            elements: vec![
                (0, vec![("Category".into(), "Walls".into()), ("FireRating".into(), "EI30".into())]),
                (1, vec![("Category".into(), "Doors".into()), ("FireRating".into(), "EI30".into())]),
                (2, vec![("Category".into(), "Doors".into())]),
            ],
            selected: vec![0],
        }
    }

    #[test]
    fn select_where_returns_and_records_ids() {
        let out = run("let n = select_where(\"Category\", \"Doors\"); log(\"n=\" + n);", &input()).unwrap();
        assert_eq!(out.actions, vec![Action::SelectIds(vec![1, 2])]);
        assert_eq!(out.logs, vec!["n=2"]);
    }

    #[test]
    fn find_where_empty_and_set_prop() {
        let out = run("for i in find_where_empty(\"FireRating\") { select_ids([i]); set_prop(\"NeedsReview\", \"yes\"); }", &input()).unwrap();
        assert!(out.actions.iter().any(|a| matches!(a, Action::SelectIds(v) if v == &[2])));
        assert!(out.actions.iter().any(|a| matches!(a, Action::SetProp(n, v) if n == "NeedsReview" && v == "yes")));
    }

    #[test]
    fn syntax_error_is_reported() {
        assert!(run("select_where(", &input()).is_err());
    }

    #[test]
    fn example_scripts_run() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/scripts");
        for name in ["01_select_fire_doors.rhai", "02_flag_missing_data.rhai", "03_clash_suite.rhai"] {
            let src = std::fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
            let out = run(&src, &input()).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(!out.actions.is_empty(), "{name} produced no actions");
        }
    }
}
