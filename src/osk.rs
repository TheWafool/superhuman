//! On-screen keyboard for the pad: a QWERTY grid plus a row of actions.
//! Physical keyboards type straight into `text` too.

/// Character rows. Data, so other layouts are a table change.
pub const ROWS: &[&str] = &["1234567890", "qwertyuiop", "asdfghjkl'", "zxcvbnm,.-"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Shift,
    Space,
    Delete,
    Done,
}

/// The action row under the letters, with each key's width in letter keys.
pub const ACTIONS: &[(Key, f32)] = &[(Key::Shift, 2.0), (Key::Space, 4.0), (Key::Delete, 2.0), (Key::Done, 2.0)];

pub enum Out {
    Nothing,
    Changed,
    Done,
    /// Focus moved off the top edge.
    LeaveUp,
}

#[derive(Debug, Clone, Default)]
pub struct Osk {
    pub text: String,
    pub row: usize,
    pub col: usize,
    pub caps: bool,
    /// Show dots instead of the text (API keys).
    pub mask: bool,
}

impl Osk {
    pub fn new(text: &str, mask: bool) -> Self {
        Self { text: text.to_owned(), row: 1, col: 0, caps: false, mask }
    }

    fn action_row(&self) -> bool {
        self.row == ROWS.len()
    }

    pub fn key_at(&self, row: usize, col: usize) -> Key {
        if row == ROWS.len() {
            return ACTIONS[col].0;
        }
        let c = ROWS[row].chars().nth(col).unwrap_or(' ');
        Key::Char(if self.caps { c.to_ascii_uppercase() } else { c })
    }

    pub fn up(&mut self) -> Out {
        if self.row == 0 {
            return Out::LeaveUp;
        }
        if self.action_row() {
            self.col = self.col_from_action();
        }
        self.row -= 1;
        Out::Nothing
    }

    pub fn down(&mut self) {
        if self.action_row() {
            return;
        }
        self.row += 1;
        if self.action_row() {
            self.col = self.action_from_col();
        }
    }

    pub fn left(&mut self) {
        self.col = self.col.saturating_sub(1);
    }

    pub fn right(&mut self) {
        let len = if self.action_row() { ACTIONS.len() } else { ROWS[self.row].chars().count() };
        self.col = (self.col + 1).min(len - 1);
    }

    /// Letter column under the middle of the focused action key, and back.
    fn col_from_action(&self) -> usize {
        let start: f32 = ACTIONS[..self.col].iter().map(|a| a.1).sum();
        (start + ACTIONS[self.col].1 / 2.0) as usize
    }

    fn action_from_col(&self) -> usize {
        let mut x = 0.0;
        for (i, a) in ACTIONS.iter().enumerate() {
            x += a.1;
            if (self.col as f32) < x {
                return i;
            }
        }
        ACTIONS.len() - 1
    }

    pub fn press(&mut self) -> Out {
        match self.key_at(self.row, self.col) {
            Key::Char(c) => self.push(c),
            Key::Space => self.push(' '),
            Key::Delete => self.delete(),
            Key::Shift => {
                self.caps = !self.caps;
                Out::Nothing
            }
            Key::Done => Out::Done,
        }
    }

    pub fn push(&mut self, c: char) -> Out {
        if self.text.chars().count() >= 64 {
            return Out::Nothing;
        }
        self.text.push(c);
        Out::Changed
    }

    pub fn delete(&mut self) -> Out {
        self.text.pop();
        Out::Changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_and_moves_between_rows() {
        let mut k = Osk::new("", false);
        k.press(); // 'q'
        k.down();
        k.down();
        k.down(); // action row, under column 0 → Shift
        assert_eq!(k.key_at(k.row, k.col), Key::Shift);
        k.press();
        k.up();
        k.press(); // 'Z'
        assert_eq!(k.text, "qZ");
    }
}
