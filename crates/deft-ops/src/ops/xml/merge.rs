use crate::structured::{ARRAYS, XML, merge};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "xml.merge",
    summary: "Deep-merge files; later files win",
    input: Input::all(XML, 2, "xml"),
    output: Output::one(&[Format::Xml], "xml"),
    params: &[ARRAYS],
    examples: &["deft xml merge shop.xml override.xml -o merged.xml"],
    tags: &["combine xml", "merge xml", "override config", "overlay"],
    suffix: Some("merged"),
    run: merge,
    ..Op::DEFAULT
};
