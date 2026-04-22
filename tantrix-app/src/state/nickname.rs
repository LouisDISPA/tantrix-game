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

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const STORAGE_KEY: &str = "tantrix-nickname";

const ADJECTIVES: &[&str] = &[
    "Sneaky", "Grumpy", "Happy", "Jolly", "Dizzy", "Sleepy", "Cranky",
    "Chubby", "Wobbly", "Silly", "Clumsy", "Sassy", "Groovy", "Plucky",
    "Feisty", "Curious", "Fancy", "Rowdy", "Mighty", "Perky", "Zesty",
    "Bouncy", "Wacky", "Breezy", "Spicy", "Sparkly", "Swift", "Drowsy",
    "Dapper", "Nifty", "Quirky", "Crispy", "Salty", "Cheery", "Moody",
    "Snazzy", "Chunky", "Glossy", "Spunky", "Whimsy",
];

const NOUNS: &[&str] = &[
    "Wizard", "Penguin", "Hamster", "Pickle", "Muffin", "Otter", "Goblin",
    "Noodle", "Wombat", "Pirate", "Panda", "Yeti", "Ninja", "Potato",
    "Raccoon", "Turnip", "Badger", "Taco", "Ferret", "Dumpling", "Walrus",
    "Gnome", "Gremlin", "Marmot", "Narwhal", "Platypus", "Quokka", "Sloth",
    "Axolotl", "Cactus", "Waffle", "Pumpkin", "Toadstool", "Gecko",
    "Llama", "Capybara", "Octopus", "Hedgehog", "Muppet", "Biscuit",
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
        return local_storage().and_then(|s| s.get_item(STORAGE_KEY).ok().flatten());
    }
    #[cfg(not(target_arch = "wasm32"))]
    None
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}
