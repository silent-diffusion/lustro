//! Mapping client-supplied names (sync-root ids, folder names, mount sources) to providers.

use lustro_core::model::CloudProvider;

/// Guess the provider from any client-supplied hint: a Windows sync-root id such as
/// `OneDrive!S-1-5-21-…!Personal`, a macOS `~/Library/CloudStorage` folder name such as
/// `GoogleDrive-william@example.com`, a display name or an rclone remote name.
/// Unknown providers map to `Other`; they still follow the same rules.
pub fn provider_from_hint(hint: &str) -> CloudProvider {
    let h = hint.to_ascii_lowercase();
    // The provider name is the leading token; ignore account details that could contain
    // another provider's name (e.g. an email address).
    let head = h
        .split(['!', '-', '@', ':', '|', ' ', '_', '.'])
        .next()
        .unwrap_or("");
    let starts = |p: &str| head.starts_with(p) || h.starts_with(p);
    if starts("onedrive") || starts("skydrive") {
        CloudProvider::OneDrive
    } else if starts("googledrive")
        || starts("google drive")
        || starts("drivefs")
        || starts("gdrive")
        || head == "google"
    {
        CloudProvider::GoogleDrive
    } else if starts("icloud") || starts("com~apple~clouddocs") || head == "apple" {
        CloudProvider::ICloud
    } else if starts("dropbox") {
        CloudProvider::Dropbox
    } else if head == "box" || starts("boxdrive") || starts("box drive") {
        CloudProvider::Box
    } else {
        CloudProvider::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use CloudProvider::*;

    #[test]
    fn windows_sync_root_ids() {
        assert_eq!(
            provider_from_hint("OneDrive!S-1-5-21-1-2-3-1001!Personal|ABC"),
            OneDrive
        );
        assert_eq!(
            provider_from_hint("OneDrive!S-1-5-21-1-2-3-1001!Business1|X"),
            OneDrive
        );
        assert_eq!(
            provider_from_hint("iCloudDrive!S-1-5-21-1-2-3-1001!"),
            ICloud
        );
        assert_eq!(provider_from_hint("Dropbox!S-1-5-21-1-2-3-1001!1"), Dropbox);
        assert_eq!(provider_from_hint("Box!S-1-5-21-1-2-3-1001!"), Box);
        assert_eq!(
            provider_from_hint("GoogleDrive!S-1-5-21-1-2-3-1001!"),
            GoogleDrive
        );
        assert_eq!(provider_from_hint("NextcloudSync!S-1-5-21!x"), Other);
    }

    #[test]
    fn macos_cloudstorage_folder_names() {
        assert_eq!(provider_from_hint("OneDrive-Personal"), OneDrive);
        assert_eq!(
            provider_from_hint("GoogleDrive-william@example.com"),
            GoogleDrive
        );
        assert_eq!(provider_from_hint("Dropbox"), Dropbox);
        assert_eq!(provider_from_hint("Box-Box"), Box);
        assert_eq!(provider_from_hint("com~apple~CloudDocs"), ICloud);
        assert_eq!(provider_from_hint("pCloud-william"), Other);
    }

    #[test]
    fn account_details_do_not_confuse_provider() {
        // "box" inside an email or "dropbox" must not be read as Box.
        assert_eq!(provider_from_hint("Dropbox-Business"), Dropbox);
        assert_eq!(
            provider_from_hint("OneDrive-william@dropbox.example"),
            OneDrive
        );
        assert_eq!(provider_from_hint("Inbox"), Other);
    }

    #[test]
    fn rclone_remote_names() {
        assert_eq!(provider_from_hint("onedrive:"), OneDrive);
        assert_eq!(provider_from_hint("gdrive:Photos"), GoogleDrive);
        assert_eq!(provider_from_hint("mys3:bucket"), Other);
    }
}
