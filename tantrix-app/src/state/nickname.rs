//! Auto-generated player nicknames.
//!
//! Picks one adjective and one noun from curated lists and joins them to
//! produce names like "Grumpy Wombat" or "Sneaky Pickle". Auto-generated
//! names are never persisted — each new tab or refresh gets a fresh
//! random combo. Only names the user explicitly commits from the lobby
//! editor get written to `localStorage`, so a custom "Louis" sticks
//! across sessions but two browser tabs don't collide on the same
//! generated default.

use getrandom::fill;

/// The longest nickname, in characters: the lobby's editor stops there, and
/// the room refuses longer ones (`tantrix-room`).
pub const MAX_CHARS: u32 = 24;

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const STORAGE_KEY: &str = "tantrix-nickname-v2";

/// Older key used before auto-generated names were treated as ephemeral.
/// We delete it on startup so stale cached values (like the notorious
/// "Clumsy Pickle" that everyone ended up with) don't override fresh
/// random picks.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const LEGACY_STORAGE_KEY: &str = "tantrix-nickname";

const ADJECTIVES: &[&str] = &[
    "Sneaky", "Grumpy", "Happy", "Jolly", "Dizzy", "Sleepy", "Cranky", "Chubby", "Wobbly", "Silly",
    "Clumsy", "Sassy", "Groovy", "Plucky", "Feisty", "Curious", "Fancy", "Rowdy", "Mighty",
    "Perky", "Zesty", "Bouncy", "Wacky", "Breezy", "Spicy", "Sparkly", "Swift", "Drowsy", "Dapper",
    "Nifty", "Quirky", "Crispy", "Salty", "Cheery", "Moody", "Snazzy", "Chunky", "Glossy",
    "Spunky", "Whimsy",
];

const NOUNS: &[&str] = &[
    "Wizard",
    "Penguin",
    "Hamster",
    "Pickle",
    "Muffin",
    "Otter",
    "Goblin",
    "Noodle",
    "Wombat",
    "Pirate",
    "Panda",
    "Yeti",
    "Ninja",
    "Potato",
    "Raccoon",
    "Turnip",
    "Badger",
    "Taco",
    "Ferret",
    "Dumpling",
    "Walrus",
    "Gnome",
    "Gremlin",
    "Marmot",
    "Narwhal",
    "Platypus",
    "Quokka",
    "Sloth",
    "Axolotl",
    "Cactus",
    "Waffle",
    "Pumpkin",
    "Toadstool",
    "Gecko",
    "Llama",
    "Capybara",
    "Octopus",
    "Hedgehog",
    "Muppet",
    "Biscuit",
];

/// Generate a fresh `Adjective Noun` nickname.
///
/// Uses 32 bits of entropy per pick from `getrandom` — the modulo bias
/// against a list of ~40 entries is ~2⁻²⁷, effectively zero.
pub fn generate() -> String {
    let adj = ADJECTIVES[pick_index(ADJECTIVES.len())];
    let noun = NOUNS[pick_index(NOUNS.len())];
    format!("{adj} {noun}")
}

fn pick_index(len: usize) -> usize {
    let mut buf = [0u8; 4];
    let _ = fill(&mut buf);
    (u32::from_le_bytes(buf) as usize) % len.max(1)
}

/// Return the user's previously-committed nickname (from `localStorage`)
/// or a fresh random one. Auto-generated names are *not* cached here —
/// only names the user explicitly edited in the lobby land in storage.
pub fn load_or_generate() -> String {
    clear_legacy();
    if let Some(stored) = load_stored() {
        if !stored.trim().is_empty() {
            return stored;
        }
    }
    generate()
}

/// Cache the nickname in `localStorage` for next session. Silent no-op off
/// the web or when storage is unavailable (e.g. private browsing on some
/// browsers).
pub fn persist(nickname: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(storage) = local_storage() {
            let _ = storage.set_item(STORAGE_KEY, nickname);
        }
    }
    #[cfg(any(feature = "desktop", feature = "android"))]
    if let Err(error) = super::native_settings::save("nickname", nickname) {
        log::warn!("{error}");
    }
    let _ = nickname;
}

fn load_stored() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        local_storage().and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
    }
    #[cfg(all(
        not(target_arch = "wasm32"),
        any(feature = "desktop", feature = "android")
    ))]
    {
        super::native_settings::load("nickname")
    }
    #[cfg(not(any(target_arch = "wasm32", feature = "desktop", feature = "android")))]
    {
        None
    }
}

fn clear_legacy() {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(s) = local_storage() {
            let _ = s.remove_item(LEGACY_STORAGE_KEY);
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

#[cfg(test)]
mod tests {
    use super::{generate, pick_index, ADJECTIVES, MAX_CHARS, NOUNS};
    #[cfg(not(any(feature = "desktop", feature = "android")))]
    use super::{load_or_generate, persist};

    /// `name` split into its adjective and noun, from the lists.
    fn words(name: &str) -> Option<(&str, &str)> {
        let (adjective, noun) = name.split_once(' ')?;
        (ADJECTIVES.contains(&adjective) && NOUNS.contains(&noun)).then_some((adjective, noun))
    }

    #[test]
    fn a_generated_name_is_an_adjective_and_a_noun() {
        for _ in 0..50 {
            let name = generate();
            assert!(words(&name).is_some(), "{name}");
        }
    }

    #[test]
    fn every_generated_name_fits_the_lobby() {
        let longest = |list: &[&str]| list.iter().map(|w| w.chars().count()).max().unwrap();
        assert!(longest(ADJECTIVES) + 1 + longest(NOUNS) <= MAX_CHARS as usize);
        // One capitalised word each, so a name splits back into its two.
        for word in ADJECTIVES.iter().chain(NOUNS) {
            assert!(word.chars().all(|c| c.is_ascii_alphabetic()), "{word}");
            assert!(word.starts_with(|c: char| c.is_ascii_uppercase()), "{word}");
        }
    }

    #[test]
    fn the_word_lists_have_no_repeats() {
        for list in [ADJECTIVES, NOUNS] {
            let mut sorted = list.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), list.len());
        }
    }

    #[test]
    fn picks_stay_in_range() {
        for len in [1, 2, 40] {
            assert!((0..200).all(|_| pick_index(len) < len), "{len}");
        }
        // An empty list never divides by zero.
        assert_eq!(pick_index(0), 0);
    }

    #[cfg(not(any(feature = "desktop", feature = "android")))]
    #[test]
    fn off_the_web_nothing_is_stored_so_a_fresh_name_is_drawn() {
        persist("Louis");
        let name = load_or_generate();
        assert!(words(&name).is_some(), "{name}");
    }
}
