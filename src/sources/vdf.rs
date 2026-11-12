//! Minimal Valve KeyValues parsers: text (`.vdf`/`.acf`) and binary (`shortcuts.vdf`).

#[derive(Debug, Clone)]
pub enum Node {
    Str(String),
    Map(Vec<(String, Node)>),
}

impl Node {
    /// Case-insensitive child lookup (Steam is inconsistent with casing).
    pub fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Map(kv) => kv.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Node::Str(_) => None,
        }
    }

    pub fn path(&self, keys: &[&str]) -> Option<&Node> {
        keys.iter().try_fold(self, |n, k| n.get(k))
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Node::Str(s) => Some(s),
            Node::Map(_) => None,
        }
    }

    pub fn entries(&self) -> &[(String, Node)] {
        match self {
            Node::Map(kv) => kv,
            Node::Str(_) => &[],
        }
    }
}

/// Parse text KeyValues. Returns a map holding the top-level pairs.
pub fn parse_text(src: &str) -> Node {
    let mut tokens = Tokens { s: src.as_bytes(), i: 0 };
    Node::Map(parse_map(&mut tokens))
}

struct Tokens<'a> {
    s: &'a [u8],
    i: usize,
}

enum Tok {
    Str(String),
    Open,
    Close,
}

impl Tokens<'_> {
    fn next(&mut self) -> Option<Tok> {
        loop {
            while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
            if self.s[self.i..].starts_with(b"//") {
                while self.i < self.s.len() && self.s[self.i] != b'\n' {
                    self.i += 1;
                }
                continue;
            }
            break;
        }
        let c = *self.s.get(self.i)?;
        self.i += 1;
        match c {
            b'{' => Some(Tok::Open),
            b'}' => Some(Tok::Close),
            b'"' => {
                let mut out = Vec::new();
                while let Some(&c) = self.s.get(self.i) {
                    self.i += 1;
                    match c {
                        b'"' => break,
                        b'\\' => {
                            let e = self.s.get(self.i).copied().unwrap_or(b'\\');
                            self.i += 1;
                            out.push(match e {
                                b'n' => b'\n',
                                b't' => b'\t',
                                other => other,
                            });
                        }
                        c => out.push(c),
                    }
                }
                Some(Tok::Str(String::from_utf8_lossy(&out).into_owned()))
            }
            _ => {
                // Unquoted token.
                let start = self.i - 1;
                while self.i < self.s.len() && !self.s[self.i].is_ascii_whitespace() && !matches!(self.s[self.i], b'{' | b'}' | b'"') {
                    self.i += 1;
                }
                Some(Tok::Str(String::from_utf8_lossy(&self.s[start..self.i]).into_owned()))
            }
        }
    }
}

fn parse_map(t: &mut Tokens) -> Vec<(String, Node)> {
    let mut out = Vec::new();
    while let Some(tok) = t.next() {
        let key = match tok {
            Tok::Str(k) => k,
            Tok::Close => break,
            Tok::Open => continue,
        };
        match t.next() {
            Some(Tok::Str(v)) => out.push((key, Node::Str(v))),
            Some(Tok::Open) => out.push((key, Node::Map(parse_map(t)))),
            _ => break,
        }
    }
    out
}

/// Parse binary KeyValues (used by `shortcuts.vdf`). Ints become strings.
pub fn parse_binary(data: &[u8]) -> Option<Node> {
    let mut i = 0;
    let map = parse_bin_map(data, &mut i)?;
    Some(Node::Map(map))
}

fn read_cstr(d: &[u8], i: &mut usize) -> Option<String> {
    let end = d[*i..].iter().position(|&b| b == 0)? + *i;
    let s = String::from_utf8_lossy(&d[*i..end]).into_owned();
    *i = end + 1;
    Some(s)
}

fn parse_bin_map(d: &[u8], i: &mut usize) -> Option<Vec<(String, Node)>> {
    let mut out = Vec::new();
    loop {
        let ty = *d.get(*i)?;
        *i += 1;
        match ty {
            0x08 => return Some(out),
            0x00 => {
                let k = read_cstr(d, i)?;
                out.push((k, Node::Map(parse_bin_map(d, i)?)));
            }
            0x01 => {
                let k = read_cstr(d, i)?;
                out.push((k, Node::Str(read_cstr(d, i)?)));
            }
            0x02 => {
                let k = read_cstr(d, i)?;
                let b = d.get(*i..*i + 4)?;
                *i += 4;
                let v = u32::from_le_bytes(b.try_into().ok()?);
                out.push((k, Node::Str(v.to_string())));
            }
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_nested_and_case() {
        let n = parse_text(r#""AppState" { "appid" "105600" "Name" "Terraria" "x" { "y" "1" } }"#);
        let app = n.get("appstate").unwrap();
        assert_eq!(app.str("name"), Some("Terraria"));
        assert_eq!(app.path(&["x"]).unwrap().str("y"), Some("1"));
    }

    #[test]
    fn binary_shortcut() {
        let mut d = vec![0x00];
        d.extend(b"shortcuts\0");
        d.push(0x00);
        d.extend(b"0\0");
        d.push(0x02);
        d.extend(b"appid\0");
        d.extend(&0xDEADBEEFu32.to_le_bytes());
        d.push(0x01);
        d.extend(b"AppName\0Celeste\0");
        d.extend([0x08, 0x08, 0x08]);
        let n = parse_binary(&d).unwrap();
        let s = n.path(&["shortcuts", "0"]).unwrap();
        assert_eq!(s.str("appname"), Some("Celeste"));
        assert_eq!(s.str("appid"), Some("3735928559"));
    }
}
