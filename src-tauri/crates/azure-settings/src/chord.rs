//! A key combination, as a value.
//!
//! Chords cross to TypeScript as strings — `"ALT+SHIFT+V"` — rather than
//! as a structure. The capture widget in the interface produces text and
//! the settings file stores text, so a structured wire form would be two
//! translations of the same thing with two chances to disagree.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The modifier keys a chord holds down. Windows has exactly these four
/// available to `RegisterHotKey`, so the set is closed and a struct of
/// flags says so more plainly than a bitfield.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The Windows key. Registrable, but Windows keeps a long list of
    /// `WIN`-chords for itself and refuses those, which is reported like
    /// any other refusal rather than guessed at here.
    pub win: bool,
}

impl Modifiers {
    pub fn none() -> Self {
        Modifiers::default()
    }

    pub fn any(&self) -> bool {
        self.ctrl || self.alt || self.shift || self.win
    }
}

/// The non-modifier half of a chord.
///
/// Deliberately not every key on the keyboard. This is the set a player
/// plausibly binds a display toggle to; anything outside it is rejected by
/// name at parse time rather than registered and silently never fired.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// `A`-`Z` and `0`-`9`, always stored uppercase.
    Char(char),
    /// `F1`-`F24`.
    F(u8),
    Left,
    Right,
    Up,
    Down,
    Space,
    Tab,
    Enter,
    Backspace,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Pause,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChordError {
    Empty,
    /// A chord with no modifier would take the key from every other
    /// program on the machine, including the one you are typing into.
    NoModifier,
    NoKey,
    TwoKeys,
    UnknownPart(String),
}

impl fmt::Display for ChordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChordError::Empty => write!(f, "no keys"),
            ChordError::NoModifier => write!(
                f,
                "needs CTRL, ALT, SHIFT or WIN — a key on its own would be taken from every other program"
            ),
            ChordError::NoKey => write!(f, "needs a key, not only modifiers"),
            ChordError::TwoKeys => write!(f, "only one key besides the modifiers"),
            ChordError::UnknownPart(p) => write!(f, "{p} is not a key Azure can bind"),
        }
    }
}

impl std::error::Error for ChordError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub mods: Modifiers,
    pub key: Key,
}

impl Chord {
    pub fn new(mods: Modifiers, key: Key) -> Result<Chord, ChordError> {
        if !mods.any() {
            return Err(ChordError::NoModifier);
        }
        Ok(Chord { mods, key })
    }

    pub fn parse(text: &str) -> Result<Chord, ChordError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(ChordError::Empty);
        }

        let mut mods = Modifiers::none();
        let mut key = None;

        for part in text.split('+') {
            let part = part.trim();
            if part.is_empty() {
                return Err(ChordError::Empty);
            }
            match part.to_ascii_uppercase().as_str() {
                "CTRL" | "CONTROL" => mods.ctrl = true,
                "ALT" => mods.alt = true,
                "SHIFT" => mods.shift = true,
                "WIN" | "SUPER" | "META" | "CMD" => mods.win = true,
                other => {
                    if key.is_some() {
                        return Err(ChordError::TwoKeys);
                    }
                    key = Some(parse_key(other)?);
                }
            }
        }

        let key = key.ok_or(ChordError::NoKey)?;
        Chord::new(mods, key)
    }
}

fn parse_key(part: &str) -> Result<Key, ChordError> {
    if let Some(n) = part.strip_prefix('F') {
        if let Ok(n) = n.parse::<u8>() {
            if (1..=24).contains(&n) {
                return Ok(Key::F(n));
            }
            return Err(ChordError::UnknownPart(part.to_string()));
        }
    }

    let mut chars = part.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_alphanumeric() {
            return Ok(Key::Char(c.to_ascii_uppercase()));
        }
    }

    Ok(match part {
        "LEFT" => Key::Left,
        "RIGHT" => Key::Right,
        "UP" => Key::Up,
        "DOWN" => Key::Down,
        "SPACE" => Key::Space,
        "TAB" => Key::Tab,
        "ENTER" | "RETURN" => Key::Enter,
        "BACKSPACE" => Key::Backspace,
        "INSERT" | "INS" => Key::Insert,
        "DELETE" | "DEL" => Key::Delete,
        "HOME" => Key::Home,
        "END" => Key::End,
        "PAGEUP" | "PGUP" => Key::PageUp,
        "PAGEDOWN" | "PGDN" => Key::PageDown,
        "PAUSE" => Key::Pause,
        _ => return Err(ChordError::UnknownPart(part.to_string())),
    })
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Char(c) => write!(f, "{c}"),
            Key::F(n) => write!(f, "F{n}"),
            Key::Left => write!(f, "LEFT"),
            Key::Right => write!(f, "RIGHT"),
            Key::Up => write!(f, "UP"),
            Key::Down => write!(f, "DOWN"),
            Key::Space => write!(f, "SPACE"),
            Key::Tab => write!(f, "TAB"),
            Key::Enter => write!(f, "ENTER"),
            Key::Backspace => write!(f, "BACKSPACE"),
            Key::Insert => write!(f, "INSERT"),
            Key::Delete => write!(f, "DELETE"),
            Key::Home => write!(f, "HOME"),
            Key::End => write!(f, "END"),
            Key::PageUp => write!(f, "PAGEUP"),
            Key::PageDown => write!(f, "PAGEDOWN"),
            Key::Pause => write!(f, "PAUSE"),
        }
    }
}

/// Modifiers render in a fixed order so that two chords that are the same
/// chord always render the same string. `Eq` compares the flags, but the
/// interface compares what it shows.
impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mods.ctrl {
            write!(f, "CTRL+")?;
        }
        if self.mods.alt {
            write!(f, "ALT+")?;
        }
        if self.mods.shift {
            write!(f, "SHIFT+")?;
        }
        if self.mods.win {
            write!(f, "WIN+")?;
        }
        write!(f, "{}", self.key)
    }
}

impl Serialize for Chord {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Chord {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Chord, D::Error> {
        let text = String::deserialize(d)?;
        Chord::parse(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> Chord {
        Chord::parse(text).expect(text)
    }

    #[test]
    fn the_defaults_the_interface_used_to_advertise_all_parse() {
        for text in [
            "ALT+SHIFT+V",
            "ALT+SHIFT+RIGHT",
            "ALT+SHIFT+LEFT",
            "CTRL+ALT+SHIFT+R",
        ] {
            assert_eq!(parsed(text).to_string(), text, "{text} should round-trip");
        }
    }

    #[test]
    fn a_chord_renders_the_way_it_was_parsed_whatever_order_it_arrived_in() {
        assert_eq!(parsed("shift+alt+ctrl+r").to_string(), "CTRL+ALT+SHIFT+R");
        assert_eq!(parsed("R+ALT").to_string(), "ALT+R");
    }

    #[test]
    fn spelling_variants_land_on_the_same_chord() {
        assert_eq!(parsed("control+del"), parsed("CTRL+DELETE"));
        assert_eq!(parsed("win+pgup"), parsed("SUPER+PAGEUP"));
        assert_eq!(parsed("alt+return"), parsed("ALT+ENTER"));
    }

    #[test]
    fn a_key_with_no_modifier_is_refused() {
        assert_eq!(Chord::parse("V"), Err(ChordError::NoModifier));
        assert_eq!(Chord::parse("F1"), Err(ChordError::NoModifier));
    }

    #[test]
    fn the_refusal_says_why_rather_than_that_it_is_invalid() {
        let said = Chord::parse("V").unwrap_err().to_string();
        assert!(said.contains("every other program"), "got: {said}");
    }

    #[test]
    fn modifiers_alone_are_not_a_chord() {
        assert_eq!(Chord::parse("CTRL+ALT"), Err(ChordError::NoKey));
    }

    #[test]
    fn two_keys_are_refused_rather_than_one_being_dropped() {
        assert_eq!(Chord::parse("ALT+A+B"), Err(ChordError::TwoKeys));
    }

    #[test]
    fn a_key_azure_cannot_bind_is_named_in_the_refusal() {
        assert_eq!(
            Chord::parse("ALT+SCROLLLOCK"),
            Err(ChordError::UnknownPart("SCROLLLOCK".into()))
        );
        assert_eq!(
            Chord::parse("ALT+F25"),
            Err(ChordError::UnknownPart("F25".into()))
        );
    }

    #[test]
    fn empty_input_is_empty_not_a_missing_modifier() {
        assert_eq!(Chord::parse(""), Err(ChordError::Empty));
        assert_eq!(Chord::parse("   "), Err(ChordError::Empty));
        assert_eq!(Chord::parse("ALT+"), Err(ChordError::Empty));
    }

    #[test]
    fn digits_and_function_keys_are_bindable() {
        assert_eq!(parsed("ALT+4").key, Key::Char('4'));
        assert_eq!(parsed("CTRL+F12").key, Key::F(12));
    }

    #[test]
    fn a_chord_crosses_the_wire_as_the_string_it_displays() {
        let chord = parsed("ALT+SHIFT+V");
        let json = serde_json::to_string(&chord).unwrap();
        assert_eq!(json, "\"ALT+SHIFT+V\"");
        assert_eq!(serde_json::from_str::<Chord>(&json).unwrap(), chord);
    }

    #[test]
    fn a_stored_chord_that_is_no_longer_valid_fails_to_deserialise() {
        assert!(serde_json::from_str::<Chord>("\"V\"").is_err());
    }
}
