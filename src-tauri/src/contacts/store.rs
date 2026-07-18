//! Platform bridge to the system Contacts store.
//!
//! On macOS this talks to `CNContactStore` via the `objc2` bindings; everywhere
//! else it is a stub that reports "denied" and returns no contacts, so the
//! workspace compiles on any target. Everything here is called from a
//! `spawn_blocking` thread (never the webview/main thread).

use super::{ContactRecord, PermissionStatus};

/// Result of a full store load: the live authorization status plus the records
/// (empty unless authorized).
pub struct LoadedContacts {
    pub status: PermissionStatus,
    pub records: Vec<ContactRecord>,
}

/// Read the current Contacts authorization status *without* prompting.
#[must_use]
pub fn permission_status() -> PermissionStatus {
    imp::permission_status()
}

/// Load every contact, requesting access first if the user has not yet decided.
/// Returns the resulting status and (when authorized) the enumerated records.
#[must_use]
pub fn load_contacts() -> LoadedContacts {
    imp::load_contacts()
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{LoadedContacts, PermissionStatus};
    use crate::contacts::{display_name_from_parts, ContactRecord};

    use base64::Engine;
    use block2::RcBlock;
    use dispatch2::{DispatchSemaphore, DispatchTime};
    use objc2::rc::Retained;
    use objc2::runtime::{Bool, ProtocolObject};
    use objc2::AllocAnyThread;
    use objc2_contacts::{
        CNAuthorizationStatus, CNContact, CNContactEmailAddressesKey, CNContactFamilyNameKey,
        CNContactFetchRequest, CNContactGivenNameKey, CNContactNicknameKey,
        CNContactOrganizationNameKey, CNContactPhoneNumbersKey, CNContactStore,
        CNContactThumbnailImageDataKey, CNEntityType, CNKeyDescriptor,
    };
    use objc2_foundation::{NSArray, NSError};

    use core::cell::RefCell;
    use core::ptr::NonNull;

    fn map_status(status: CNAuthorizationStatus) -> PermissionStatus {
        match status {
            CNAuthorizationStatus::Authorized | CNAuthorizationStatus::Limited => {
                PermissionStatus::Authorized
            }
            CNAuthorizationStatus::Denied => PermissionStatus::Denied,
            CNAuthorizationStatus::Restricted => PermissionStatus::Restricted,
            _ => PermissionStatus::NotDetermined,
        }
    }

    pub(super) fn permission_status() -> PermissionStatus {
        // SAFETY: class method with no preconditions; thread-safe per Apple docs.
        let status =
            unsafe { CNContactStore::authorizationStatusForEntityType(CNEntityType::Contacts) };
        map_status(status)
    }

    pub(super) fn load_contacts() -> LoadedContacts {
        // SAFETY: `+new` on a thread-safe class.
        let store = unsafe { CNContactStore::new() };

        let mut status = permission_status();
        if status == PermissionStatus::NotDetermined {
            // First run: trigger the TCC prompt and block until the user answers.
            request_access(&store);
            status = permission_status();
        }

        if status != PermissionStatus::Authorized {
            return LoadedContacts {
                status,
                records: Vec::new(),
            };
        }

        let records = enumerate(&store).unwrap_or_default();
        LoadedContacts { status, records }
    }

    /// Request contacts access, blocking the calling (background) thread on a
    /// dispatch semaphore until the completion handler fires. The handler runs on
    /// an arbitrary queue (never this thread, so no deadlock); it just signals the
    /// semaphore to wake us. The caller re-reads the authorization status
    /// afterwards, which is the authoritative outcome.
    fn request_access(store: &CNContactStore) {
        let sem = DispatchSemaphore::new(0);
        let sem_for_block = sem.clone();
        let handler = RcBlock::new(move |_granted: Bool, _err: *mut NSError| {
            sem_for_block.signal();
        });

        // SAFETY: valid store; the block matches the expected signature and lives
        // until the handler runs (RcBlock is heap-allocated / reference-counted).
        unsafe {
            store.requestAccessForEntityType_completionHandler(CNEntityType::Contacts, &handler);
        }
        let _ = sem.wait(DispatchTime::FOREVER);
    }

    /// The Contacts properties we fetch. Fetching only what we use is an Apple
    /// best practice (and required — accessing an unfetched key throws).
    fn keys_to_fetch() -> Retained<NSArray<ProtocolObject<dyn CNKeyDescriptor>>> {
        // SAFETY: these are the framework's global `NSString` key constants;
        // `NSString` conforms to `CNKeyDescriptor`.
        let keys: [&ProtocolObject<dyn CNKeyDescriptor>; 7] = unsafe {
            [
                ProtocolObject::from_ref(CNContactGivenNameKey),
                ProtocolObject::from_ref(CNContactFamilyNameKey),
                ProtocolObject::from_ref(CNContactNicknameKey),
                ProtocolObject::from_ref(CNContactOrganizationNameKey),
                ProtocolObject::from_ref(CNContactPhoneNumbersKey),
                ProtocolObject::from_ref(CNContactEmailAddressesKey),
                ProtocolObject::from_ref(CNContactThumbnailImageDataKey),
            ]
        };
        NSArray::from_slice(&keys)
    }

    /// Enumerate every contact, mapping each to a plain [`ContactRecord`]. The
    /// enumeration block runs synchronously on this thread until finished, so a
    /// `RefCell` accumulator is safe (no concurrency).
    fn enumerate(store: &CNContactStore) -> Option<Vec<ContactRecord>> {
        let keys = keys_to_fetch();
        // SAFETY: standard fetch-request construction.
        let request =
            unsafe { CNContactFetchRequest::initWithKeysToFetch(CNContactFetchRequest::alloc(), &keys) };

        let records: RefCell<Vec<ContactRecord>> = RefCell::new(Vec::new());
        let block = RcBlock::new(|contact: NonNull<CNContact>, _stop: NonNull<Bool>| {
            // SAFETY: the framework hands us a valid contact for the block's duration.
            let contact = unsafe { contact.as_ref() };
            if let Some(rec) = contact_to_record(contact) {
                records.borrow_mut().push(rec);
            }
        });

        let mut error: Option<Retained<NSError>> = None;
        // SAFETY: valid store, request, and block; `error` out-param is optional.
        let ok = unsafe {
            store.enumerateContactsWithFetchRequest_error_usingBlock(
                &request,
                Some(&mut error),
                &block,
            )
        };
        // Drop the block before reclaiming the borrowed accumulator.
        drop(block);
        if !ok {
            return None;
        }
        Some(records.into_inner())
    }

    fn contact_to_record(contact: &CNContact) -> Option<ContactRecord> {
        // SAFETY: all keys below were included in `keys_to_fetch`.
        let (given, family, nickname, organization, phones, emails, avatar) = unsafe {
            let given = contact.givenName().to_string();
            let family = contact.familyName().to_string();
            let nickname = contact.nickname().to_string();
            let organization = contact.organizationName().to_string();

            let mut phones = Vec::new();
            for labeled in contact.phoneNumbers().to_vec() {
                phones.push(labeled.value().stringValue().to_string());
            }

            let mut emails = Vec::new();
            for labeled in contact.emailAddresses().to_vec() {
                emails.push(labeled.value().to_string());
            }

            let avatar = contact
                .thumbnailImageData()
                .map(|data| data_url(&data.to_vec()));

            (given, family, nickname, organization, phones, emails, avatar)
        };

        // No endpoints means nothing to key on: skip.
        if phones.is_empty() && emails.is_empty() {
            return None;
        }

        Some(ContactRecord {
            display_name: display_name_from_parts(&given, &family, &nickname, &organization),
            phones,
            emails,
            avatar_data_url: avatar,
        })
    }

    /// Encode raw image bytes as a `data:` URL, sniffing the MIME from the magic
    /// bytes (thumbnails are usually JPEG, occasionally PNG).
    fn data_url(bytes: &[u8]) -> String {
        let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
            "image/png"
        } else if bytes.starts_with(b"GIF8") {
            "image/gif"
        } else {
            // JPEG (FF D8) and anything unrecognized: default to JPEG.
            "image/jpeg"
        };
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        format!("data:{mime};base64,{encoded}")
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::{LoadedContacts, PermissionStatus};

    pub(super) fn permission_status() -> PermissionStatus {
        PermissionStatus::Denied
    }

    pub(super) fn load_contacts() -> LoadedContacts {
        LoadedContacts {
            status: PermissionStatus::Denied,
            records: Vec::new(),
        }
    }
}
