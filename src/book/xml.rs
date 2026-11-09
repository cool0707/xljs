//! A small XML tree for the parts of a workbook that are read whole (the
//! workbook, the styles, the rules of a sheet), and the reading of a subtree
//! while the rest of a sheet is streamed.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

#[derive(Debug, Default, Clone)]
pub(crate) struct Node {
    /// The local name (without the namespace prefix).
    pub name: String,
    /// The attributes by local name.
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub text: String,
}

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The attribute as a flag (1 / true), with its default.
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

    /// The text of the node and of its descendants.
    pub fn all_text(&self) -> String {
        let mut s = self.text.clone();
        for c in &self.children {
            s.push_str(&c.all_text());
        }
        s
    }

    /// The descendants with the name, outermost first; nested ones under a
    /// match are not searched.
    pub fn find<'a>(&'a self, name: &str, out: &mut Vec<&'a Node>) {
        for c in &self.children {
            if c.name == name {
                out.push(c);
            } else {
                c.find(name, out);
            }
        }
    }
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
        .map(|a| {
            (
                local(a.key.as_ref()),
                a.unescape_value()
                    .map(|v| v.into_owned())
                    .unwrap_or_default(),
            )
        })
        .collect();
    Node {
        name: local(e.name().as_ref()),
        attrs,
        ..Default::default()
    }
}

/// The text of a text-like event (text, CDATA, entity reference).
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

/// Reads the rest of the element that started with top (its Start event
/// was just read) and returns it with its contents.
pub(crate) fn subtree(r: &mut Reader<&[u8]>, top: Node) -> Node {
    let mut stack = vec![top];
    let mut buf = Vec::new();
    loop {
        let ev = r.read_event_into(&mut buf);
        match ev {
            Ok(Event::Start(ref e)) => stack.push(element(e)),
            Ok(Event::Empty(ref e)) => {
                let n = element(e);
                stack.last_mut().unwrap().children.push(n);
            }
            Ok(Event::End(_)) => {
                let n = stack.pop().unwrap();
                match stack.last_mut() {
                    Some(p) => p.children.push(n),
                    None => return n,
                }
            }
            Ok(Event::Eof) | Err(_) => {
                // Unclosed: what was read.
                while stack.len() > 1 {
                    let n = stack.pop().unwrap();
                    stack.last_mut().unwrap().children.push(n);
                }
                return stack.pop().unwrap();
            }
            Ok(ref other) => {
                if let Some(s) = text_of(other) {
                    stack.last_mut().unwrap().text.push_str(&s);
                }
            }
        }
        buf.clear();
    }
}

/// The root element of a document.
pub(crate) fn parse(xml: &[u8]) -> Option<Node> {
    let mut r = Reader::from_reader(xml);
    let mut buf = Vec::new();
    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let top = element(&e);
                drop(e);
                return Some(subtree(&mut r, top));
            }
            Ok(Event::Empty(e)) => return Some(element(&e)),
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
        buf.clear();
    }
}
