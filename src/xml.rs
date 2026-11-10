//! A small XML tree (local names only) for the parts of a workbook that are
//! read whole: the workbook, the styles, drawings, charts, the rules of a
//! sheet. Also the reading of a subtree while the rest of a sheet is
//! streamed (src/book).
//!
//! The files come from anywhere: elements nested deeper than MAX_DEPTH are
//! left out, so that walking the tree (recursively) cannot run out of stack.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

/// How deep elements are kept.
pub(crate) const MAX_DEPTH: usize = 256;

#[derive(Debug, Default, Clone)]
pub(crate) struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub text: String,
}

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// A boolean attribute ("1" or "true"), default when missing.
    pub fn flag(&self, name: &str, default: bool) -> bool {
        match self.attr(name) {
            Some(v) => v == "1" || v == "true",
            None => default,
        }
    }

    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// The same as children.
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children(name)
    }

    /// The "val" attribute of the child name (DrawingML's <c:x val="..."/>).
    pub fn val(&self, name: &str) -> Option<&str> {
        self.child(name).and_then(|c| c.attr("val"))
    }

    /// The child name as a boolean: true unless its val is "0" or "false";
    /// None without the child.
    pub fn val_flag(&self, name: &str) -> Option<bool> {
        self.child(name).map(|c| !matches!(c.attr("val"), Some("0" | "false")))
    }

    pub fn path(&self, names: &[&str]) -> Option<&Node> {
        names.iter().try_fold(self, |n, name| n.child(name))
    }

    /// The first element named name, this one or below.
    pub fn find(&self, name: &str) -> Option<&Node> {
        if self.name == name {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(name))
    }

    /// The elements named name below this one (not below those found).
    pub fn find_all<'a>(&'a self, name: &str, out: &mut Vec<&'a Node>) {
        for c in &self.children {
            if c.name == name {
                out.push(c);
            } else {
                c.find_all(name, out);
            }
        }
    }

    pub fn all_text(&self) -> String {
        let mut s = self.text.clone();
        for c in &self.children {
            s.push_str(&c.all_text());
        }
        s
    }

    /// The text of the rich text below (<rich><p>...<t>), paragraphs on
    /// lines of their own.
    pub fn rich_text(&self) -> String {
        fn runs(n: &Node, out: &mut String) {
            if n.name == "t" {
                out.push_str(&n.text);
            }
            for c in &n.children {
                runs(c, out);
            }
        }
        let ps: Vec<String> = self
            .find("rich")
            .map(|r| {
                r.all("p")
                    .map(|p| {
                        let mut s = String::new();
                        runs(p, &mut s);
                        s
                    })
                    .collect()
            })
            .unwrap_or_default();
        ps.join("\n").trim().to_string()
    }
}

/// The cells (column, row; 1-based) of a drawing anchor
/// (xdr:twoCellAnchor, xdr:oneCellAnchor): its from and, when it has one,
/// its to.
pub(crate) fn anchor_cells(a: &Node) -> Option<((u32, u32), Option<(u32, u32)>)> {
    let at = |n: &str| -> Option<(u32, u32)> {
        let m = a.child(n)?;
        let num = |k: &str| m.child(k).and_then(|c| c.all_text().trim().parse::<u32>().ok());
        Some((num("col")?.checked_add(1)?, num("row")?.checked_add(1)?))
    };
    Some((at("from")?, at("to")))
}

fn local(b: &[u8]) -> String {
    let s = String::from_utf8_lossy(b);
    s.rsplit(':').next().unwrap_or("").to_string()
}

pub(crate) fn element(e: &BytesStart) -> Node {
    #[allow(deprecated)]
    let attrs = e
        .attributes()
        .flatten()
        .map(|a| (local(a.key.as_ref()), a.unescape_value().map(|v| v.into_owned()).unwrap_or_default()))
        .collect();
    Node { name: local(e.name().as_ref()), attrs, ..Default::default() }
}

fn text_of(ev: &Event) -> Option<String> {
    match ev {
        Event::Text(t) => t.xml10_content().ok().map(|s| s.into_owned()),
        Event::CData(t) => t.decode().ok().map(|s| s.into_owned()),
        Event::GeneralRef(g) => Some(match g.resolve_char_ref() {
            Ok(Some(c)) => c.to_string(),
            _ => match g.decode().as_deref() {
                Ok("amp") => "&".into(),
                Ok("lt") => "<".into(),
                Ok("gt") => ">".into(),
                Ok("quot") => "\"".into(),
                Ok("apos") => "'".into(),
                _ => String::new(),
            },
        }),
        _ => None,
    }
}

/// Reads the rest of the element top (its start just read). An error or the
/// end of the file: what was read, or None when strict.
fn build(r: &mut Reader<&[u8]>, top: Node, strict: bool) -> Option<Node> {
    let mut stack = vec![top];
    // Elements open below MAX_DEPTH, left out.
    let mut skipped = 0usize;
    let mut buf = Vec::new();
    loop {
        let ev = r.read_event_into(&mut buf);
        match ev {
            Ok(Event::Start(ref e)) => {
                if skipped > 0 || stack.len() >= MAX_DEPTH {
                    skipped += 1;
                } else {
                    stack.push(element(e));
                }
            }
            Ok(Event::Empty(ref e)) => {
                if skipped == 0 && stack.len() < MAX_DEPTH {
                    let n = element(e);
                    stack.last_mut()?.children.push(n);
                }
            }
            Ok(Event::End(_)) => {
                if skipped > 0 {
                    skipped -= 1;
                } else {
                    let n = stack.pop()?;
                    match stack.last_mut() {
                        Some(p) => p.children.push(n),
                        None => return Some(n),
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => {
                if strict {
                    return None;
                }
                // Unclosed: what was read.
                while stack.len() > 1 {
                    let n = stack.pop()?;
                    stack.last_mut()?.children.push(n);
                }
                return stack.pop();
            }
            Ok(ref other) => {
                if skipped == 0
                    && let Some(s) = text_of(other)
                {
                    stack.last_mut()?.text.push_str(&s);
                }
            }
        }
        buf.clear();
    }
}

/// Reads the rest of the element top (its start just read by the caller,
/// which streams the file); unclosed elements end where the file does.
pub(crate) fn subtree(r: &mut Reader<&[u8]>, top: Node) -> Node {
    let name = top.name.clone();
    build(r, top, false).unwrap_or(Node { name, ..Default::default() })
}

fn root(xml: &[u8], strict: bool) -> Option<Node> {
    let mut r = Reader::from_reader(xml);
    let mut buf = Vec::new();
    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let top = element(&e);
                drop(e);
                return build(&mut r, top, strict);
            }
            Ok(Event::Empty(e)) => return Some(element(&e)),
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
        buf.clear();
    }
}

/// The root element of a document; None when it is not well-formed.
pub(crate) fn parse(xml: &[u8]) -> Option<Node> {
    root(xml, true)
}

/// The root element of a document, as much of it as can be read.
pub(crate) fn parse_partial(xml: &[u8]) -> Option<Node> {
    root(xml, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trees() {
        let n = parse(br#"<?xml version="1.0"?><a:r x:k="v &amp; w"><b val="0"/><c>t&lt;<d>u</d></c></a:r>"#).unwrap();
        assert_eq!((n.name.as_str(), n.attr("k")), ("r", Some("v & w")));
        assert_eq!(n.val_flag("b"), Some(false));
        assert_eq!(n.child("c").unwrap().all_text(), "t<u");
        assert_eq!(n.find("d").map(|d| d.text.as_str()), Some("u"));
        assert!(parse(b"<a><b></a>").is_none());
        let p = parse_partial(b"<a><b>x").unwrap();
        assert_eq!(p.child("b").unwrap().text, "x");
    }

    #[test]
    fn deep_nesting_is_cut() {
        let n = 100_000;
        let xml = format!("<r>{}x{}<after/></r>", "<e>".repeat(n), "</e>".repeat(n));
        let root = parse(xml.as_bytes()).unwrap();
        let mut depth = 0;
        let mut cur = &root;
        while let Some(c) = cur.child("e") {
            cur = c;
            depth += 1;
        }
        assert_eq!(depth, MAX_DEPTH - 1);
        assert!(root.child("after").is_some(), "what follows the deep part is kept");
        assert_eq!(root.all_text(), "");
    }
}
