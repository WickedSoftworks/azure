//! Just enough of Valve's KeyValues format to read a Steam library.
//!
//! The format is quoted strings in pairs, with a bare quoted string
//! followed by a brace opening a nested block:
//!
//! ```text
//! "libraryfolders"
//! {
//!     "0"
//!     {
//!         "path"    "C:\\Program Files (x86)\\Steam"
//!     }
//! }
//! ```
//!
//! Written here rather than taken as a dependency because this is the
//! whole format Azure needs, the files it reads are written by Steam
//! rather than by anyone hostile, and a parser this size is easier to
//! keep honest than a crate that also handles binary VDF.

/// A parsed block: its pairs, and its child blocks by name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Block {
    pub pairs: Vec<(String, String)>,
    pub blocks: Vec<(String, Block)>,
}

impl Block {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    pub fn block(&self, key: &str) -> Option<&Block> {
        self.blocks
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, b)| b)
    }

    /// Every child block, in file order. Steam numbers library folders
    /// `"0"`, `"1"`, … and the numbers carry no meaning worth keeping.
    pub fn children(&self) -> impl Iterator<Item = &Block> {
        self.blocks.iter().map(|(_, b)| b)
    }
}

/// Parses a KeyValues document into its root block.
///
/// Never fails. A malformed file yields whatever was well-formed before
/// the damage: a truncated `libraryfolders.vdf` should still produce the
/// libraries it did manage to list, because the alternative is a player
/// with a half-written file seeing no games at all.
pub fn parse(text: &str) -> Block {
    let mut tokens = Tokens { rest: text };
    let mut root = Block::default();
    fill(&mut tokens, &mut root);
    root
}

struct Tokens<'a> {
    rest: &'a str,
}

enum Token {
    Str(String),
    Open,
    Close,
}

impl Tokens<'_> {
    fn next(&mut self) -> Option<Token> {
        loop {
            self.rest = self.rest.trim_start();

            // Comments run to the end of the line.
            if let Some(after) = self.rest.strip_prefix("//") {
                let cut = after.find('\n').map(|i| i + 1).unwrap_or(after.len());
                self.rest = &after[cut..];
                continue;
            }

            let mut chars = self.rest.chars();
            return match chars.next()? {
                '{' => {
                    self.rest = chars.as_str();
                    Some(Token::Open)
                }
                '}' => {
                    self.rest = chars.as_str();
                    Some(Token::Close)
                }
                '"' => {
                    let body = chars.as_str();
                    // A string with no closing quote is not a value. The
                    // file was cut off mid-write, and `G:\Steam` may be
                    // the first half of `G:\SteamLibrary` — handing that
                    // back as a path would point the scan at a directory
                    // that does not exist, or worse, one that does.
                    let (value, after) = quoted(body)?;
                    self.rest = after;
                    Some(Token::Str(value))
                }
                // Unquoted tokens are not something Steam writes, and
                // skipping the character is how a damaged file still
                // yields the part that was intact.
                _ => {
                    self.rest = chars.as_str();
                    continue;
                }
            };
        }
    }
}

/// Reads to the closing quote, honouring `\"` and `\\`.
///
/// `None` when the string never closes, which can only happen at the end
/// of a truncated file.
fn quoted(body: &str) -> Option<(String, &str)> {
    let mut out = String::new();
    let mut chars = body.char_indices();

    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, &body[i + 1..])),
            '\\' => match chars.next() {
                Some((_, 'n')) => out.push('\n'),
                Some((_, 't')) => out.push('\t'),
                Some((_, escaped)) => out.push(escaped),
                None => return None,
            },
            other => out.push(other),
        }
    }

    None
}

/// Fills `into` until the block closes or the tokens run out.
fn fill(tokens: &mut Tokens, into: &mut Block) {
    while let Some(token) = tokens.next() {
        let key = match token {
            Token::Str(key) => key,
            Token::Close => return,
            // A brace with no key before it: nothing sensible to attach
            // it to, so its contents are skipped rather than guessed at.
            Token::Open => {
                let mut discard = Block::default();
                fill(tokens, &mut discard);
                continue;
            }
        };

        match tokens.next() {
            Some(Token::Str(value)) => into.pairs.push((key, value)),
            Some(Token::Open) => {
                let mut child = Block::default();
                fill(tokens, &mut child);
                into.blocks.push((key, child));
            }
            Some(Token::Close) | None => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from the real file on the machine this was written on.
    const LIBRARY_FOLDERS: &str = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"apps"
		{
			"228980"		"908039319"
			"431960"		"826275581"
		}
	}
	"1"
	{
		"path"		"G:\\SteamLibrary"
		"label"		""
		"apps"
		{
			"730"		"71589381210"
		}
	}
}
"#;

    #[test]
    fn the_libraries_come_out_in_order_with_their_paths_unescaped() {
        let root = parse(LIBRARY_FOLDERS);
        let folders = root.block("libraryfolders").expect("root block");
        let paths: Vec<_> = folders.children().filter_map(|b| b.get("path")).collect();
        assert_eq!(
            paths,
            vec![r"C:\Program Files (x86)\Steam", r"G:\SteamLibrary"]
        );
    }

    #[test]
    fn nested_blocks_do_not_leak_into_their_parent() {
        let root = parse(LIBRARY_FOLDERS);
        let folders = root.block("libraryfolders").unwrap();
        let first = folders.children().next().unwrap();
        assert!(first.get("730").is_none(), "an app id is not a library key");
        assert_eq!(first.block("apps").unwrap().pairs.len(), 2);
    }

    #[test]
    fn an_appmanifest_yields_the_name_and_the_install_directory() {
        let acf = r#"
"AppState"
{
	"appid"		"228980"
	"name"		"Steamworks Common Redistributables"
	"installdir"		"Steamworks Shared"
	"InstalledDepots"
	{
		"228982"
		{
			"manifest"		"1522189672835167764"
		}
	}
}
"#;
        let parsed = parse(acf);
        let state = parsed.block("AppState").expect("AppState");
        assert_eq!(state.get("name"), Some("Steamworks Common Redistributables"));
        assert_eq!(state.get("installdir"), Some("Steamworks Shared"));
        assert_eq!(state.get("appid"), Some("228980"));
    }

    #[test]
    fn keys_are_matched_without_regard_to_case() {
        let parsed = parse(r#""AppState" { "InstallDir" "Half-Life" }"#);
        let app = parsed.block("appstate").unwrap();
        assert_eq!(app.get("installdir"), Some("Half-Life"));
    }

    #[test]
    fn a_truncated_file_yields_what_was_intact() {
        // A file cut off mid-write: the first library is complete and
        // should survive, because showing no games at all would be the
        // worse answer.
        let cut = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Steam"
	}
	"1"
	{
		"path"		"G:\\Steam
"#;
        let parsed = parse(cut);
        let folders = parsed.block("libraryfolders").expect("root");
        let paths: Vec<_> = folders.children().filter_map(|b| b.get("path")).collect();
        assert_eq!(paths, vec![r"C:\Steam"]);
    }

    #[test]
    fn comments_and_escaped_quotes_are_handled() {
        let text = r#"
// a comment about the block below
"root"
{
	"quote"		"say \"hello\""
	"tab"		"a\tb"
}
"#;
        let parsed = parse(text);
        let root = parsed.block("root").expect("root");
        assert_eq!(root.get("quote"), Some("say \"hello\""));
        assert_eq!(root.get("tab"), Some("a\tb"));
    }

    #[test]
    fn an_empty_document_is_an_empty_block() {
        assert_eq!(parse(""), Block::default());
        assert_eq!(parse("   \n\t "), Block::default());
    }
}
