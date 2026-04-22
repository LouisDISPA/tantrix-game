//! Auto-generated player nicknames.
//!
//! Picks one adjective and one noun from curated lists and joins them to
//! produce names like "Grumpy Wombat" or "Sneaky Pickle". The last chosen
//! nickname is cached in `localStorage` so a single browser keeps the same
//! identity across sessions.

use getrandom::fill;

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const STORAGE_KEY: &str = "tantrix-nickname";

const ADJECTIVES: &[&str] = &[
    "Sneaky", "Grumpy", "Happy", "Jolly", "Dizzy", "Sleepy", "Cranky",
    "Chubby", "Wobbly", "Silly", "Clumsy", "Sassy", "Groovy", "Plucky",
    "Feisty", "Curious", "Fancy", "Rowdy", "Mighty", "Perky", "Zesty",
    "Bouncy", "Wacky", "Breezy", "Spicy", "Sparkly", "Sleepy", "Swift",
    "Drowsy", "Dapper", "Nifty", "Quirky", "Crispy",
];

const NOUNS: &[&str] = &[
    "Wizard", "Penguin", "Hamster", "Pickle", "Muffin", "Otter", "Goblin",
    "Noodle", "Wombat", "Pirate", "Panda", "Yeti", "Ninja", "Potato",
    "Raccoon", "Turnip", "Badger", "Taco", "Ferret", "Dumpling", "Walrus",
    "Gnome", "Gremlin", "Marmot", "Narwhal", "Platypus", "Quokka", "Sloth",
    "Axolotl", "Cactus", "Waffle", "Pumpkin", "Toadstool",
];

/// Generate a fresh `Adjective Noun` nickname.
pub fn generate() -> String {
    let mut buf = [0u8; 2];
    let _ = fill(&mut buf);
    let adj = ADJECTIVES[(buf[0] as usize) % ADJECTIVES.len()];
    let noun = NOUNS[(buf[1] as usize) % NOUNS.len()];
    format!("{adj} {noun}")
}

/// Return the last-used nickname (from `localStorage`) or generate a new
/// one if none is stored yet.
pub fn load_or_generate() -> String {
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
    let _ = nickname;
}

fn load_stored() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        return local_storage()
            .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten());
    }
    #[cfg(not(target_arch = "wasm32"))]
    None
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}
