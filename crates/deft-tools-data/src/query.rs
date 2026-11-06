//! jq-syntax queries through jaq.

use deft_core::error::{Result, bad_value};
use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data, unwrap_valr};
use jaq_json::{Val, read};
use serde_json::Value;

/// Run a jq program over `input`; each result is one value.
pub fn query(input: &Value, program: &str) -> Result<Vec<Value>> {
    let text = input.to_string();
    let val = read::parse_single(text.as_bytes())
        .map_err(|e| bad_value("expr", format!("reading input: {e}")))?;
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());
    let loader = Loader::new(defs);
    let arena = Arena::default();
    let file = File {
        code: program,
        path: (),
    };
    let modules = loader
        .load(&arena, file)
        .map_err(|e| bad_value("expr", format!("{program:?} does not parse: {e:?}")))?;
    let filter = Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|e| bad_value("expr", format!("{program:?} does not compile: {e:?}")))?;
    let ctx = Ctx::<data::JustLut<Val>>::new(&filter.lut, Vars::new([]));
    let mut out = Vec::new();
    for item in filter.id.run((ctx, val)).map(unwrap_valr) {
        let v = item.map_err(|e| bad_value("expr", e.to_string()))?;
        let text = v.to_string();
        out.push(serde_json::from_str(&text).unwrap_or(Value::String(text)));
    }
    Ok(out)
}
