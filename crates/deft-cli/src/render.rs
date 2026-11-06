//! Result rendering: one compact line by default, JSON with --json.

use deft_core::error::DeftError;
use deft_core::value::format_bytes;
use deft_ops::runtime::{Item, Report};
use serde_json::{Map, Value, json};
use std::fmt::Write as _;

fn seconds(ms: u128) -> String {
    format!("{:.2}s", ms as f64 / 1000.0)
}

fn error_lines(out: &mut String, e: &DeftError, prefix: &str) {
    let _ = writeln!(out, "error {} {prefix}{}", e.code, e.message);
    if let Some(h) = &e.hint {
        let _ = writeln!(out, "hint: {h}");
    }
}

/// Key facts of a report as `k=v` pairs, skipping empty values.
fn fact_line(facts: &Map<String, Value>) -> String {
    let mut parts = Vec::new();
    for (k, v) in facts {
        match v {
            Value::Null => {}
            Value::String(s) if s.contains(' ') => parts.push(format!("{k}={s:?}")),
            Value::String(s) => parts.push(format!("{k}={s}")),
            other => parts.push(format!("{k}={other}")),
        }
    }
    parts.join(" ")
}

fn common_dir(item: &Item) -> String {
    match item
        .outputs
        .first()
        .and_then(|o| std::path::Path::new(&o.path).parent())
    {
        Some(p) if !p.as_os_str().is_empty() => format!("{}/", p.display()),
        _ => "./".into(),
    }
}

fn item_line(op: &str, item: &Item, out: &mut String) {
    let input = if item.inputs.len() > 2 {
        format!("{} files", item.inputs.len())
    } else {
        item.inputs.join(" ")
    };
    if let Some(e) = &item.error {
        let prefix = if e.message.contains(input.as_str()) || input.is_empty() {
            String::new()
        } else {
            format!("{input}: ")
        };
        error_lines(out, e, &prefix);
        return;
    }
    let target = match item.outputs.len() {
        0 => None,
        1 => Some(item.outputs[0].path.clone()),
        n if item.note.is_empty() => Some(format!("{} ({n} files)", common_dir(item))),
        _ => Some(common_dir(item)),
    };
    let mut line = format!("ok {op}");
    if !input.is_empty() {
        let _ = write!(line, " {input}");
    }
    if let Some(t) = target {
        let _ = write!(line, " -> {t}");
    }
    if !item.note.is_empty() {
        let _ = write!(line, "  {}", item.note);
    } else if let [o] = item.outputs.as_slice()
        && let Some(b) = o.bytes
    {
        let _ = write!(line, "  {}", format_bytes(b));
    }
    let _ = writeln!(out, "{line}");
    if item.outputs.is_empty() && !item.facts.is_empty() {
        let _ = writeln!(out, "{}", fact_line(&item.facts));
    }
    for w in &item.warnings {
        let _ = writeln!(out, "warning: {w}");
    }
}

pub fn text(report: &Report) -> String {
    let mut out = String::new();
    let op = report.op.id;
    if report.dry_run {
        for item in &report.items {
            if let Some(e) = &item.error {
                error_lines(&mut out, e, "");
                continue;
            }
            let _ = write!(
                out,
                "plan {op} {} -> {}",
                item.inputs.join(" "),
                item.planned.join(" ")
            );
            if !item.note.is_empty() {
                let _ = write!(out, "  {}", item.note);
            }
            out.push('\n');
        }
        return out;
    }
    let time = seconds(report.elapsed_ms);
    if report.items.len() > 3 {
        // Batch: one summary line, failures listed.
        let failed: Vec<&Item> = report.items.iter().filter(|i| i.error.is_some()).collect();
        let done = report.items.len() - failed.len();
        let before: u64 = report
            .items
            .iter()
            .filter(|i| i.error.is_none())
            .filter_map(|i| i.input_bytes)
            .sum();
        let after: u64 = report
            .items
            .iter()
            .flat_map(|i| &i.outputs)
            .filter_map(|o| o.bytes)
            .sum();
        let dir = report
            .items
            .iter()
            .find(|i| !i.outputs.is_empty())
            .map(common_dir)
            .unwrap_or_default();
        let state = if failed.is_empty() { "ok" } else { "partial" };
        let _ = write!(out, "{state} {op} {done} of {} files", report.items.len());
        if !dir.is_empty() {
            let _ = write!(
                out,
                " -> {dir}  {} -> {}",
                format_bytes(before),
                format_bytes(after)
            );
        }
        let _ = writeln!(out, "  {time}");
        for item in failed {
            item_line(op, item, &mut out);
        }
        return out;
    }
    for item in &report.items {
        item_line(op, item, &mut out);
    }
    if report.items.len() == 1
        && report.items[0].error.is_none()
        && !report.items[0].outputs.is_empty()
    {
        // Append timing to the result line.
        if let Some(pos) = out.find('\n') {
            out.insert_str(pos, &format!("  {time}"));
        }
    }
    out
}

fn error_json(e: &DeftError) -> Value {
    json!({"code": e.code.as_str(), "message": e.message, "hint": e.hint})
}

pub fn json(report: &Report) -> String {
    let items: Vec<Value> = report
        .items
        .iter()
        .map(|i| {
            let mut m = Map::new();
            m.insert("ok".into(), json!(i.error.is_none()));
            m.insert("inputs".into(), json!(i.inputs));
            if report.dry_run {
                m.insert("planned".into(), json!(i.planned));
            }
            let outputs: Vec<Value> = i
                .outputs
                .iter()
                .map(|o| {
                    let mut om = Map::new();
                    om.insert("path".into(), json!(o.path));
                    om.insert("format".into(), json!(o.format.map(|f| f.label())));
                    om.insert("bytes".into(), json!(o.bytes));
                    om.extend(o.facts.clone());
                    Value::Object(om)
                })
                .collect();
            m.insert("outputs".into(), Value::Array(outputs));
            m.insert("facts".into(), Value::Object(i.facts.clone()));
            m.insert("warnings".into(), json!(i.warnings));
            if let Some(e) = &i.error {
                m.insert("error".into(), error_json(e));
            }
            Value::Object(m)
        })
        .collect();
    let mut root = json!({
        "v": 1,
        "ok": report.ok(),
        "op": report.op.id,
        "items": items,
        "elapsed_ms": report.elapsed_ms as u64,
    });
    if !report.chain.is_empty() {
        root["chain"] = json!(report.chain);
    }
    if report.dry_run {
        root["dry_run"] = json!(true);
    }
    format!("{root}\n")
}

pub fn error_text(e: &DeftError) -> String {
    let mut out = String::new();
    error_lines(&mut out, e, "");
    out
}

pub fn error_json_doc(op: Option<&str>, e: &DeftError) -> String {
    format!(
        "{}\n",
        json!({"v": 1, "ok": false, "op": op, "error": error_json(e)})
    )
}
