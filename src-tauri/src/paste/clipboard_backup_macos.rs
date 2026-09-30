// Full clipboard backup and restore on macOS (every item, every type).
//
// Reference VoiceInk : ClipboardManager.swift saves each NSPasteboardItem's
// types and data before pasting and writes them back afterwards, so an
// image, a copied file or formatted text survives a dictation. Windows does
// the same in clipboard_backup.rs.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString};

/// One pasteboard item: (type identifier, raw bytes) pairs.
type Item = Vec<(String, Vec<u8>)>;

pub struct Backup {
    items: Vec<Item>,
}

impl Backup {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

pub fn backup_all() -> Backup {
    let pb = NSPasteboard::generalPasteboard();
    let items = pb
        .pasteboardItems()
        .map(|arr| {
            arr.to_vec()
                .iter()
                .map(|item| {
                    item.types()
                        .to_vec()
                        .iter()
                        .filter_map(|ty| {
                            item.dataForType(ty)
                                .map(|data| (ty.to_string(), data.to_vec()))
                        })
                        .collect::<Item>()
                })
                .filter(|item| !item.is_empty())
                .collect()
        })
        .unwrap_or_default();
    Backup { items }
}

pub fn restore_all(backup: &Backup) -> bool {
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    if backup.items.is_empty() {
        return true;
    }
    let objects: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = backup
        .items
        .iter()
        .map(|item| {
            let pb_item = NSPasteboardItem::new();
            for (ty, bytes) in item {
                let data = NSData::with_bytes(bytes);
                pb_item.setData_forType(&data, &NSString::from_str(ty));
            }
            ProtocolObject::from_retained(pb_item)
        })
        .collect();
    pb.writeObjects(&NSArray::from_retained_slice(&objects))
}
