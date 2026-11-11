//! This module turns the keys this terminal hands moon into the physical keys libghostty
//! encodes from, for the shell a tab shows. libghostty owns every encoding rule - application
//! cursor keys, the Kitty keyboard protocol and the rest - so this only says which key it was.

use libghostty_vt::key::{Key as VtKey, Mods};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Describes a key as libghostty is told about it: the physical key, the modifiers held, the
/// text it types, and the key's own character before shift.
pub struct VtKeystroke {
    pub key: VtKey,
    pub mods: Mods,
    pub text: Option<String>,
    pub unshifted: Option<char>,
    /// Says whether shift went into making the character, which the program is not told about
    /// again.
    pub shift_consumed: bool,
}

/// Returns the keystroke for a key event, or `None` for a key no shell is sent.
pub fn vt_keystroke_of(event: &KeyEvent) -> Option<VtKeystroke> {
    let mods = mods_of(event.modifiers);
    let named = |key: VtKey| VtKeystroke {
        key,
        mods,
        text: None,
        unshifted: None,
        shift_consumed: false,
    };
    Some(match event.code {
        KeyCode::Enter => named(VtKey::Enter),
        KeyCode::Tab => named(VtKey::Tab),
        KeyCode::BackTab => VtKeystroke {
            mods: mods | Mods::SHIFT,
            ..named(VtKey::Tab)
        },
        KeyCode::Backspace => named(VtKey::Backspace),
        KeyCode::Esc => named(VtKey::Escape),
        KeyCode::Left => named(VtKey::ArrowLeft),
        KeyCode::Right => named(VtKey::ArrowRight),
        KeyCode::Up => named(VtKey::ArrowUp),
        KeyCode::Down => named(VtKey::ArrowDown),
        KeyCode::Home => named(VtKey::Home),
        KeyCode::End => named(VtKey::End),
        KeyCode::PageUp => named(VtKey::PageUp),
        KeyCode::PageDown => named(VtKey::PageDown),
        KeyCode::Delete => named(VtKey::Delete),
        KeyCode::Insert => named(VtKey::Insert),
        KeyCode::F(number) => named(function_key(number)?),
        KeyCode::Char(character) => {
            let (key, unshifted) = physical_key_of(character);
            // With ctrl or alt held the program is told the chord, not handed text.
            let types_text = !event
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
            let shift_consumed = event.modifiers.contains(KeyModifiers::SHIFT)
                || unshifted.is_some_and(|unshifted| unshifted != character);
            VtKeystroke {
                key,
                mods,
                text: types_text.then(|| character.to_string()),
                unshifted,
                shift_consumed: shift_consumed && types_text,
            }
        }
        _ => return None,
    })
}

fn mods_of(modifiers: KeyModifiers) -> Mods {
    let mut mods = Mods::empty();
    if modifiers.contains(KeyModifiers::SHIFT) {
        mods |= Mods::SHIFT;
    }
    if modifiers.contains(KeyModifiers::CONTROL) {
        mods |= Mods::CTRL;
    }
    if modifiers.contains(KeyModifiers::ALT) {
        mods |= Mods::ALT;
    }
    if modifiers.contains(KeyModifiers::SUPER) {
        mods |= Mods::SUPER;
    }
    mods
}

fn function_key(number: u8) -> Option<VtKey> {
    Some(match number {
        1 => VtKey::F1,
        2 => VtKey::F2,
        3 => VtKey::F3,
        4 => VtKey::F4,
        5 => VtKey::F5,
        6 => VtKey::F6,
        7 => VtKey::F7,
        8 => VtKey::F8,
        9 => VtKey::F9,
        10 => VtKey::F10,
        11 => VtKey::F11,
        12 => VtKey::F12,
        _ => return None,
    })
}

/// Returns the key on a US layout a character is typed with, and that key's own character.
/// Characters on no key are typed as text on no key at all.
fn physical_key_of(character: char) -> (VtKey, Option<char>) {
    let lower = character.to_ascii_lowercase();
    let letter = match lower {
        'a' => Some(VtKey::A),
        'b' => Some(VtKey::B),
        'c' => Some(VtKey::C),
        'd' => Some(VtKey::D),
        'e' => Some(VtKey::E),
        'f' => Some(VtKey::F),
        'g' => Some(VtKey::G),
        'h' => Some(VtKey::H),
        'i' => Some(VtKey::I),
        'j' => Some(VtKey::J),
        'k' => Some(VtKey::K),
        'l' => Some(VtKey::L),
        'm' => Some(VtKey::M),
        'n' => Some(VtKey::N),
        'o' => Some(VtKey::O),
        'p' => Some(VtKey::P),
        'q' => Some(VtKey::Q),
        'r' => Some(VtKey::R),
        's' => Some(VtKey::S),
        't' => Some(VtKey::T),
        'u' => Some(VtKey::U),
        'v' => Some(VtKey::V),
        'w' => Some(VtKey::W),
        'x' => Some(VtKey::X),
        'y' => Some(VtKey::Y),
        'z' => Some(VtKey::Z),
        _ => None,
    };
    if let Some(key) = letter {
        return (key, Some(lower));
    }
    let (key, unshifted) = match character {
        '0' | ')' => (VtKey::Digit0, '0'),
        '1' | '!' => (VtKey::Digit1, '1'),
        '2' | '@' => (VtKey::Digit2, '2'),
        '3' | '#' => (VtKey::Digit3, '3'),
        '4' | '$' => (VtKey::Digit4, '4'),
        '5' | '%' => (VtKey::Digit5, '5'),
        '6' | '^' => (VtKey::Digit6, '6'),
        '7' | '&' => (VtKey::Digit7, '7'),
        '8' | '*' => (VtKey::Digit8, '8'),
        '9' | '(' => (VtKey::Digit9, '9'),
        ' ' => (VtKey::Space, ' '),
        '-' | '_' => (VtKey::Minus, '-'),
        '=' | '+' => (VtKey::Equal, '='),
        '[' | '{' => (VtKey::BracketLeft, '['),
        ']' | '}' => (VtKey::BracketRight, ']'),
        '\\' | '|' => (VtKey::Backslash, '\\'),
        ';' | ':' => (VtKey::Semicolon, ';'),
        '\'' | '"' => (VtKey::Quote, '\''),
        '`' | '~' => (VtKey::Backquote, '`'),
        ',' | '<' => (VtKey::Comma, ','),
        '.' | '>' => (VtKey::Period, '.'),
        '/' | '?' => (VtKey::Slash, '/'),
        _ => return (VtKey::Unidentified, None),
    };
    (key, Some(unshifted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_letter_types_itself_on_its_key() {
        let stroke = vt_keystroke_of(&KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
            .expect("expected a keystroke");
        assert_eq!(stroke.key, VtKey::Q);
        assert_eq!(stroke.text.as_deref(), Some("q"));
        assert!(!stroke.shift_consumed);
    }

    #[test]
    fn a_shifted_character_says_its_shift_went_into_it() {
        let stroke = vt_keystroke_of(&KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT))
            .expect("expected a keystroke");
        assert_eq!(stroke.key, VtKey::Slash);
        assert_eq!(stroke.unshifted, Some('/'));
        assert!(stroke.shift_consumed);
    }

    #[test]
    fn a_chord_with_ctrl_types_no_text() {
        let stroke = vt_keystroke_of(&KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
            .expect("expected a keystroke");
        assert_eq!(stroke.key, VtKey::C);
        assert!(stroke.text.is_none());
        assert!(stroke.mods.contains(Mods::CTRL));
    }
}
