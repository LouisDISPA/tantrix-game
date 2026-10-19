//! One background check per app launch, preserved across menu navigation.
use crate::state::android_updates::{self, Update};
use dioxus::prelude::*;

#[derive(Clone, Copy)]
pub struct AndroidUpdates(pub Resource<Result<Option<Update>, String>>);

pub fn use_android_updates() {
    let release = use_resource(|| async {
        let result = android_updates::check().await;
        if let Err(error) = &result {
            log::warn!("Update check failed: {error}");
        }
        result
    });
    use_context_provider(|| AndroidUpdates(release));
}
