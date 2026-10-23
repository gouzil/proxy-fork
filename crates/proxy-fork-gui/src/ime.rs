use winit::event::Ime;

#[derive(Default)]
pub struct ImeBridge {
    pub preedit: String,
    last_commit: Option<String>,
}
impl ImeBridge {
    pub fn event(&mut self, event: &Ime) -> Option<String> {
        match event {
            Ime::Preedit(text, _) => {
                self.preedit.clone_from(text);
                None
            }
            Ime::Commit(text) => {
                self.preedit.clear();
                self.last_commit = Some(text.clone());
                Some(text.clone())
            }
            Ime::Disabled => {
                self.reset();
                None
            }
            Ime::Enabled => None,
        }
    }
    pub fn suppress_keyboard_text(&mut self, text: Option<&str>) -> bool {
        let duplicate = text.is_some_and(|t| self.last_commit.as_deref() == Some(t));
        self.last_commit = None;
        !self.preedit.is_empty() || duplicate
    }
    pub fn reset(&mut self) {
        self.preedit.clear();
        self.last_commit = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composition_commit_and_focus_do_not_duplicate_text() {
        let mut ime = ImeBridge::default();
        ime.event(&Ime::Preedit("ni".into(), Some((2, 2))));
        assert!(ime.suppress_keyboard_text(Some("n")));
        assert_eq!(ime.event(&Ime::Commit("你".into())), Some("你".into()));
        assert!(ime.suppress_keyboard_text(Some("你")));
        assert!(!ime.suppress_keyboard_text(Some("a")));
        ime.event(&Ime::Preedit("hao".into(), Some((3, 3))));
        ime.reset();
        assert!(ime.preedit.is_empty());
        assert!(!ime.suppress_keyboard_text(Some("a")));
    }
}
