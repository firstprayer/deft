use crate::structured::{SORT_KEYS, XML, format};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "xml.format",
    summary: "Tidy a file: --sort-keys",
    input: Input::each(XML, "xml"),
    output: Output::one(&[Format::Xml], "xml"),
    params: &[SORT_KEYS],
    examples: &["deft xml format shop.xml -o tidy.xml"],
    tags: &["tidy xml", "pretty print xml", "sort keys"],
    suffix: Some("formatted"),
    run: format,
    ..Op::DEFAULT
};
