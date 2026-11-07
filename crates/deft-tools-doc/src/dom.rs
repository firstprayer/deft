//! Small helpers over a parsed HTML tree.

use markup5ever_rcdom::{Handle, NodeData};

/// The element's name, or `None` for text and other nodes.
pub(crate) fn tag(node: &Handle) -> Option<&str> {
    match &node.data {
        NodeData::Element { name, .. } => Some(&name.local),
        _ => None,
    }
}

pub(crate) fn is(node: &Handle, names: &[&str]) -> bool {
    tag(node).is_some_and(|t| names.contains(&t))
}

pub(crate) fn attr(node: &Handle, name: &str) -> Option<String> {
    match &node.data {
        NodeData::Element { attrs, .. } => attrs
            .borrow()
            .iter()
            .find(|a| &*a.name.local == name)
            .map(|a| a.value.to_string()),
        _ => None,
    }
}

pub(crate) fn children(node: &Handle) -> Vec<Handle> {
    node.children.borrow().clone()
}

/// Every element under `node`, in document order, `node` excluded.
pub(crate) fn descendants(node: &Handle) -> Vec<Handle> {
    let mut out = Vec::new();
    fn walk(n: &Handle, out: &mut Vec<Handle>) {
        for c in n.children.borrow().iter() {
            if tag(c).is_some() {
                out.push(c.clone());
            }
            walk(c, out);
        }
    }
    walk(node, &mut out);
    out
}

/// The text as a browser shows it in preformatted text: `<br>` is a line break.
pub(crate) fn text(node: &Handle) -> String {
    let mut out = String::new();
    fn walk(n: &Handle, out: &mut String) {
        match &n.data {
            NodeData::Text { contents } => out.push_str(&contents.borrow()),
            NodeData::Element { name, .. } if &*name.local == "br" => out.push('\n'),
            NodeData::Element { name, .. }
                if matches!(&*name.local, "script" | "style" | "template") => {}
            _ => {
                for c in n.children.borrow().iter() {
                    walk(c, out);
                }
            }
        }
    }
    walk(node, &mut out);
    out
}

pub(crate) fn parent(node: &Handle) -> Option<Handle> {
    let weak = node.parent.take();
    let up = weak.as_ref().and_then(|w| w.upgrade());
    node.parent.set(weak);
    up
}

/// The parents of a node, nearest first.
pub(crate) fn ancestors(node: &Handle) -> Vec<Handle> {
    let mut out = Vec::new();
    let mut at = parent(node);
    while let Some(p) = at {
        at = parent(&p);
        out.push(p);
    }
    out
}

/// Take a node out of the tree.
pub(crate) fn detach(node: &Handle) {
    if let Some(p) = parent(node) {
        p.children
            .borrow_mut()
            .retain(|c| !std::rc::Rc::ptr_eq(c, node));
    }
    node.parent.set(None);
}
