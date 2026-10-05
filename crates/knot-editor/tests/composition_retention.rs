use knot_capture::KnotPersonaDisplayV1;
use knot_composition::{CollectionItem, ItemKind, retention::CompositionRetainPort};
use knot_editor::{
    CompositionGrant, KnotResidentCompositionPort, KnotResidentSource, KnotSyncFileStore, KnotVault,
};
use personae::{IdentityProvider, InMemoryProvider};

#[path = "../../knot-composition/tests/support/recipe.rs"]
mod recipe_support;

fn persona() -> KnotPersonaDisplayV1 {
    KnotPersonaDisplayV1 {
        stable_id: "persona:test".into(),
        label: "Test".into(),
    }
}
fn adapter(resident: &KnotResidentSource) -> KnotResidentCompositionPort {
    KnotResidentCompositionPort::new(
        persona(),
        resident
            .composition_retention(CompositionGrant::new(8192, 20))
            .unwrap(),
    )
    .unwrap()
}
fn identity() -> ([u8; 32], [u8; 32]) {
    let identity = InMemoryProvider::from_seed([51; 32]);
    (
        identity.master_keypair().to_seed(),
        identity.master_public_key().to_bytes(),
    )
}

#[test]
fn typed_recipe_edits_are_immutable_encrypted_versions_that_reopen() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("sync.redb");
    let vault_path = root.path().join("vault");
    let (seed, writer) = identity();
    let first = CollectionItem::new(
        ItemKind::ProjectionRecipe,
        "Private recipe 346782",
        "Human-readable summary",
    )
    .with_projection_recipe(recipe_support::recipe_material());
    let mut material = first.projection_recipe.clone().unwrap();
    material.snapshot.recipe.definition.arrangement.spacing += 8;
    material
        .snapshot
        .recipe
        .definition
        .provenance
        .source_revision = Some("recipe:revision:2".into());
    let second = CollectionItem::new(
        ItemKind::ProjectionRecipe,
        "Edited recipe",
        "Edited spacing",
    )
    .with_projection_recipe(material);
    let receipts = {
        let vault = KnotVault::open(&vault_path, [52; 32]).unwrap();
        let store = KnotSyncFileStore::open(&database, [53; 32], [writer]).unwrap();
        let resident = KnotResidentSource::from_synced_vault(vault, store.clone(), seed).unwrap();
        let port = adapter(&resident);
        let a = port.retain(port.target(), first.clone()).unwrap();
        let b = port.retain(port.target(), second.clone()).unwrap();
        assert_ne!(a.operation, b.operation);
        assert_eq!(
            port.retain(port.target(), first.clone()).unwrap().operation,
            a.operation
        );
        let read_vault = KnotVault::open(&vault_path, [52; 32]).unwrap();
        assert!(
            pollster::block_on(store.projection(&read_vault))
                .unwrap()
                .documents
                .is_empty()
        );
        (a, b)
    };
    let bytes = std::fs::read(&database).unwrap();
    assert!(
        !bytes
            .windows(first.label.len())
            .any(|window| window == first.label.as_bytes())
    );
    let vault = KnotVault::open(&vault_path, [52; 32]).unwrap();
    let store = KnotSyncFileStore::open(&database, [53; 32], [writer]).unwrap();
    let resident = KnotResidentSource::from_synced_vault(vault, store, seed).unwrap();
    let port = adapter(&resident);
    let items = port.list(port.target()).unwrap();
    assert_eq!(items.len(), 2);
    for (expected, receipt) in [(&first, receipts.0), (&second, receipts.1)] {
        let retained = items
            .iter()
            .find(|retained| retained.item.id == expected.id)
            .unwrap();
        assert_eq!(&retained.item, expected);
        assert_eq!(retained.receipt.operation, receipt.operation);
        retained
            .item
            .projection_recipe
            .as_ref()
            .unwrap()
            .validate()
            .unwrap();
    }
    assert!(!root.path().join("collection.json").exists());
}
#[test]
fn encrypted_reopen_retry_and_no_publishing_exposure() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("sync.redb");
    let (seed, writer) = identity();
    let item = CollectionItem::new(
        ItemKind::Note,
        "private",
        "Secret composition quotation 9274682",
    );
    let receipt = {
        let vault = KnotVault::open(root.path().join("vault"), [52; 32]).unwrap();
        let store = KnotSyncFileStore::open(&path, [53; 32], [writer]).unwrap();
        let read_vault = KnotVault::open(root.path().join("vault"), [52; 32]).unwrap();
        let resident = KnotResidentSource::from_synced_vault(vault, store.clone(), seed).unwrap();
        let port = adapter(&resident);
        let receipt = port.retain(port.target(), item.clone()).unwrap();
        assert!(!receipt.already_retained);
        let retry = port.retain(port.target(), item.clone()).unwrap();
        assert!(retry.already_retained);
        assert_eq!(retry.operation, receipt.operation);
        assert!(
            pollster::block_on(store.projection(&read_vault))
                .unwrap()
                .documents
                .is_empty()
        );
        assert!(read_vault.documents().next().is_none());
        let list = port.list(port.target()).unwrap();
        assert_eq!(list[0].item, item);
        assert_eq!(list[0].author, writer);
        let mut conflict = item.clone();
        conflict.text = "changed".into();
        assert!(port.retain(port.target(), conflict).is_err());
        receipt
    };
    let disk = std::fs::read(&path).unwrap();
    assert!(
        !disk
            .windows(item.text.len())
            .any(|bytes| bytes == item.text.as_bytes())
    );
    let vault = KnotVault::open(root.path().join("vault"), [52; 32]).unwrap();
    let store = KnotSyncFileStore::open(&path, [53; 32], [writer]).unwrap();
    let resident = KnotResidentSource::from_synced_vault(vault, store, seed).unwrap();
    let port = adapter(&resident);
    assert_eq!(port.list(port.target()).unwrap()[0].item, item);
    let retry = port.retain(port.target(), item).unwrap();
    assert!(retry.already_retained);
    assert_eq!(retry.operation, receipt.operation);
}

#[test]
fn denied_revoked_locked_and_wrong_destination_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let (seed, writer) = identity();
    let vault = KnotVault::open(root.path().join("vault"), [52; 32]).unwrap();
    let store = KnotSyncFileStore::open(root.path().join("sync.redb"), [53; 32], [writer]).unwrap();
    let resident = KnotResidentSource::from_synced_vault(vault, store.clone(), seed).unwrap();
    let item = CollectionItem::new(ItemKind::Passage, "quote", "text");
    let authority = resident
        .composition_retention(CompositionGrant::new(8192, 20))
        .unwrap();
    let port = KnotResidentCompositionPort::new(persona(), authority.clone()).unwrap();
    let mut wrong = port.target().clone();
    wrong.space_id = [99; 32];
    assert!(port.retain(&wrong, item.clone()).is_err());
    assert!(port.list(&wrong).is_err());
    let narrow = KnotResidentCompositionPort::new(
        persona(),
        resident
            .composition_retention(CompositionGrant::new(1, 1))
            .unwrap(),
    )
    .unwrap();
    assert!(narrow.retain(narrow.target(), item.clone()).is_err());
    store.deny_writer(&writer);
    assert!(port.retain(port.target(), item.clone()).is_err());
    assert!(port.list(port.target()).is_err());
    store.admit_writer(writer);
    authority.revoke();
    assert!(port.retain(port.target(), item.clone()).is_err());
    assert!(port.list(port.target()).is_err());
    let fresh = adapter(&resident);
    resident.session(None).lock_vault();
    assert!(fresh.retain(fresh.target(), item).is_err());
    assert!(fresh.list(fresh.target()).is_err());
}

#[test]
fn unsynced_resident_is_not_silently_promoted_to_a_destination() {
    let root = tempfile::tempdir().unwrap();
    let resident = KnotResidentSource::from_vault(
        KnotVault::open(root.path().join("vault"), [52; 32]).unwrap(),
    );
    assert!(
        resident
            .composition_retention(CompositionGrant::new(8192, 20))
            .is_err()
    );
}

#[test]
fn concurrent_retries_share_one_operation_and_other_devices_are_readable() {
    let root = tempfile::tempdir().unwrap();
    let (seed, writer) = identity();
    let other = InMemoryProvider::from_seed([63; 32]);
    let other_writer = other.master_public_key().to_bytes();
    let vault = KnotVault::open(root.path().join("vault"), [52; 32]).unwrap();
    let store = KnotSyncFileStore::open(
        root.path().join("sync.redb"),
        [53; 32],
        [writer, other_writer],
    )
    .unwrap();
    let item = CollectionItem::new(ItemKind::Note, "another device", "retained elsewhere");
    pollster::block_on(store.author(
        other.master_keypair().to_seed(),
        &vault,
        &knot_editor::KnotSyncEvent::RetainCompositionV1 {
            item_json: serde_json::to_string(&item).unwrap(),
        },
    ))
    .unwrap();
    let resident = KnotResidentSource::from_synced_vault(vault, store.clone(), seed).unwrap();
    let ports: Vec<_> = (0..4).map(|_| adapter(&resident)).collect();
    let own = CollectionItem::new(ItemKind::Note, "local", "retry me");
    let receipts = std::thread::scope(|scope| {
        ports
            .iter()
            .map(|port| scope.spawn(|| port.retain(port.target(), own.clone()).unwrap()))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        receipts
            .iter()
            .filter(|receipt| !receipt.already_retained)
            .count(),
        1
    );
    assert!(
        receipts
            .iter()
            .all(|receipt| receipt.operation == receipts[0].operation)
    );
    let list = ports[0].list(ports[0].target()).unwrap();
    let foreign = list.iter().find(|retained| retained.item == item).unwrap();
    assert_eq!(foreign.author, other_writer);
    assert_eq!(&foreign.receipt.target, ports[0].target());
}

#[test]
fn commons_keys_are_required_for_both_reads_and_writes() {
    let root = tempfile::tempdir().unwrap();
    let (seed, writer) = identity();
    let vault = KnotVault::open(root.path().join("vault"), [52; 32]).unwrap();
    let store =
        KnotSyncFileStore::open_commons(root.path().join("sync.redb"), [53; 32], [writer]).unwrap();
    let mut keys = stickleback::DataKeyring::new();
    keys.rotate_random().unwrap();
    let resident = KnotResidentSource::from_communal_vault(vault, store, seed, keys).unwrap();
    let port = adapter(&resident);
    let item = CollectionItem::new(
        ItemKind::Note,
        "commons",
        "retained privately in this space",
    );
    port.retain(port.target(), item.clone()).unwrap();
    assert_eq!(port.list(port.target()).unwrap()[0].item, item);
    // Replacement installs the keyring, then correctly refuses to project
    // already-retained ciphertext whose epoch has become unavailable.
    assert!(
        resident
            .session(None)
            .replace_communal_keys(stickleback::DataKeyring::new())
            .is_err()
    );
    assert!(port.list(port.target()).is_err());
    assert!(port.retain(port.target(), item).is_err());
}
