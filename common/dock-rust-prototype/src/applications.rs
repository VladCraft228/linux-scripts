//! Installed-application picker state. Pinning never executes a launcher.
#[derive(Clone)]
pub struct Application {
    pub key: String,
    pub name: String,
    pub pinned: bool,
}
#[derive(Default)]
pub struct Browser {
    pub items: Vec<Application>,
    pub query: String,
    pub selected: usize,
    pub offset: usize,
    pub message: String,
}
impl Browser {
    pub fn new(items: Vec<Application>) -> Self {
        Self { items, ..Self::default() }
    }
    pub fn filtered(&self) -> Vec<usize> {
        let query = self.query.to_lowercase();
        self.items.iter().enumerate().filter(|(_, item)| {
            item.name.to_lowercase().contains(&query) || item.key.to_lowercase().contains(&query)
        }).map(|(i, _)| i).collect()
    }
    pub fn changed(&mut self) {
        self.selected = 0;
        self.offset = 0;
        self.message.clear();
    }
    pub fn move_selection(&mut self, delta: isize, page: usize) {
        let count = self.filtered().len();
        self.selected = self.selected.saturating_add_signed(delta).min(count.saturating_sub(1));
        if self.selected < self.offset { self.offset = self.selected; }
        if self.selected >= self.offset + page { self.offset = self.selected + 1 - page; }
    }
    pub fn scroll(&mut self, delta: isize, page: usize) {
        let count = self.filtered().len();
        self.offset = self.offset.saturating_add_signed(delta).min(count.saturating_sub(page));
        self.selected = self.selected.max(self.offset).min((self.offset + page).min(count).saturating_sub(1));
    }
    pub fn selected_key(&self) -> Option<String> {
        let index = *self.filtered().get(self.selected)?;
        let item = &self.items[index];
        (!item.pinned).then(|| item.key.clone())
    }
    pub fn pinned(&mut self, key: &str, added: bool) {
        if let Some(item) = self.items.iter_mut().find(|item| item.key == key) {
            item.pinned = true;
            self.message = if added { format!("Added {} to Dock", item.name) } else { "Already pinned to Dock".into() };
        }
    }
}
/// Core X11 printable keysyms: Latin, Unicode and legacy Cyrillic (including UA).
/// Compose/dead keys and input-method preedit are intentionally not implemented.
pub fn character(symbol: u32) -> Option<char> {
    match symbol {
        0x20..=0x7e | 0xa0..=0xff => char::from_u32(symbol),
        0x01000100..=0x0110ffff => char::from_u32(symbol & 0x00ffffff).filter(|c| !c.is_control()),
        0x06a1..=0x06af => "ђѓёєѕіїјљњћќґўџ".chars().nth((symbol - 0x06a1) as usize),
        0x06b1..=0x06bf => "ЂЃЁЄЅІЇЈЉЊЋЌҐЎЏ".chars().nth((symbol - 0x06b1) as usize),
        0x06c0..=0x06df => "юабцдефгхийклмнопярстужвьызшэщчъ".chars().nth((symbol - 0x06c0) as usize),
        0x06e0..=0x06ff => "ЮАБЦДЕФГХИЙКЛМНОПЯРСТУЖВЬЫЗШЭЩЧЪ".chars().nth((symbol - 0x06e0) as usize),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filters_names_and_ids_without_case() {
        let mut browser = Browser::new(vec![Application { key: "org.example.editor".into(), name: "Редактор".into(), pinned: false }]);
        browser.query = "РЕД".into();
        assert_eq!(browser.filtered(), vec![0]);
        browser.query = "EDITOR".into();
        assert_eq!(browser.filtered(), vec![0]);
        browser.pinned("org.example.editor", true);
        assert!(browser.selected_key().is_none());
    }
    #[test]
    fn maps_ukrainian_letters() {
        assert_eq!(character(0x06a4), Some('є'));
        assert_eq!(character(0x06a6), Some('і'));
        assert_eq!(character(0x06a7), Some('ї'));
        assert_eq!(character(0x06ad), Some('ґ'));
        assert_eq!(character(0x01000456), Some('і'));
        assert_eq!(character(0xff08), None);
    }
}
