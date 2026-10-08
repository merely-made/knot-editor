//! Knot-resident retained composition authority. This never opens a persona,
//! enables replication, or projects private material into publishable documents.
use super::{KnotResidentSource, VaultSyncAuthority};
use crate::KnotSyncCipher;
use knot_capture::{
    KnotPersonaDisplayV1, KnotRetainEncryptionV1, KnotRetainError, KnotRetainTargetV1,
};
use knot_composition::{
    CollectionItem,
    retention::{CompositionReceipt, CompositionRetainPort, RetainedCompositionItem},
};
use std::sync::{Arc, Mutex};

pub struct CompositionGrant {
    max_item_bytes: usize,
    max_items: usize,
}
impl CompositionGrant {
    pub fn new(max_item_bytes: usize, max_items: usize) -> Self {
        Self {
            max_item_bytes: max_item_bytes.min(knot_composition::MAX_STORE_BYTES),
            max_items: max_items.min(knot_composition::MAX_ITEMS),
        }
    }
}

#[derive(Clone)]
pub struct KnotCompositionPort {
    resident: KnotResidentSource,
    grant: Arc<Mutex<Option<CompositionGrant>>>,
}
impl KnotResidentSource {
    pub fn composition_retention(
        &self,
        grant: CompositionGrant,
    ) -> Result<KnotCompositionPort, KnotRetainError> {
        super::capture_retention::destination_for_state(&self.state()).map_err(error)?;
        Ok(KnotCompositionPort {
            resident: self.clone(),
            grant: Arc::new(Mutex::new(Some(grant))),
        })
    }
}
impl KnotCompositionPort {
    pub fn revoke(&self) {
        self.grant.lock().unwrap_or_else(|p| p.into_inner()).take();
    }
}

pub struct KnotResidentCompositionPort {
    target: KnotRetainTargetV1,
    port: KnotCompositionPort,
}
impl KnotResidentCompositionPort {
    pub fn new(
        persona: KnotPersonaDisplayV1,
        port: KnotCompositionPort,
    ) -> Result<Self, KnotRetainError> {
        let target = {
            let grant = port.grant.lock().unwrap_or_else(|p| p.into_inner());
            if grant.is_none() {
                return Err(error("composition grant revoked"));
            }
            target(persona, &port.resident.state())?
        };
        Ok(Self { target, port })
    }

    fn execute<T>(
        &self,
        expected: &KnotRetainTargetV1,
        action: impl FnOnce(&super::VaultSource, &CompositionGrant) -> Result<T, KnotRetainError>,
    ) -> Result<T, KnotRetainError> {
        if expected != &self.target {
            return Err(error(
                "composition destination does not match selected target",
            ));
        }
        let grant = self.port.grant.lock().unwrap_or_else(|p| p.into_inner());
        let grant = grant
            .as_ref()
            .ok_or_else(|| error("composition grant revoked"))?;
        let state = self.port.resident.state();
        if target(self.target.persona.clone(), &state)? != self.target {
            return Err(error("composition destination changed"));
        }
        action(&state, grant)
    }
}
impl CompositionRetainPort for KnotResidentCompositionPort {
    fn target(&self) -> &KnotRetainTargetV1 {
        &self.target
    }
    fn retain(
        &self,
        expected: &KnotRetainTargetV1,
        item: CollectionItem,
    ) -> Result<CompositionReceipt, KnotRetainError> {
        self.execute(expected, |state, grant| {
            item.validate().map_err(error)?;
            let size = serde_json::to_vec(&item).map_err(error)?.len();
            if size > grant.max_item_bytes {
                return Err(error("composition item exceeds grant byte limit"));
            }
            let (store, seed, cipher) = authority(state)?;
            let (operation, already_retained) =
                pollster::block_on(store.retain_composition_with_cipher(
                    &seed,
                    cipher,
                    &item,
                    grant.max_items,
                    knot_composition::MAX_STORE_BYTES,
                ))
                .map_err(error)?;
            Ok(CompositionReceipt {
                target: self.target.clone(),
                item_id: item.id,
                operation,
                already_retained,
            })
        })
    }
    fn list(
        &self,
        expected: &KnotRetainTargetV1,
    ) -> Result<Vec<RetainedCompositionItem>, KnotRetainError> {
        self.execute(expected, |state, grant| {
            let (store, _, cipher) = authority(state)?;
            let (items, _) = pollster::block_on(store.organized_composition_with_cipher(
                cipher,
                grant.max_items,
                knot_composition::MAX_STORE_BYTES,
            ))
            .map_err(error)?;
            items
                .into_iter()
                .map(
                    |(item, operation, writer, organization, organization_revision)| {
                        if serde_json::to_vec(&item).map_err(error)?.len() > grant.max_item_bytes {
                            return Err(error("composition item exceeds grant byte limit"));
                        }
                        if serde_json::to_vec(&organization).map_err(error)?.len()
                            > grant.max_item_bytes
                        {
                            return Err(error("organization exceeds grant byte limit"));
                        }
                        Ok(RetainedCompositionItem {
                            receipt: CompositionReceipt {
                                target: self.target.clone(),
                                item_id: item.id.clone(),
                                operation,
                                already_retained: true,
                            },
                            item,
                            author: writer,
                            organization,
                            organization_revision,
                        })
                    },
                )
                .collect()
        })
    }
    fn organize(
        &self,
        expected: &KnotRetainTargetV1,
        change: knot_composition::retention::OrganizeComposition,
    ) -> Result<CompositionReceipt, KnotRetainError> {
        self.execute(expected, |state, grant| {
            change.organization.validate().map_err(error)?;
            if serde_json::to_vec(&change.organization)
                .map_err(error)?
                .len()
                > grant.max_item_bytes
            {
                return Err(error("organization exceeds grant byte limit"));
            }
            let (store, seed, cipher) = authority(state)?;
            let (operation, already_retained) =
                pollster::block_on(store.organize_composition_with_cipher(
                    &seed,
                    cipher,
                    &change,
                    grant.max_items,
                    knot_composition::MAX_STORE_BYTES,
                ))
                .map_err(error)?;
            Ok(CompositionReceipt {
                target: self.target.clone(),
                item_id: change.item_id,
                operation,
                already_retained,
            })
        })
    }
}
fn authority(
    state: &super::VaultSource,
) -> Result<(&crate::KnotSyncFileStore, &[u8; 32], KnotSyncCipher<'_>), KnotRetainError> {
    match state.sync.as_ref() {
        Some(VaultSyncAuthority::Personal {
            store,
            signing_seed,
        }) => Ok((
            store,
            &**signing_seed,
            KnotSyncCipher::Personal(&state.vault),
        )),
        Some(VaultSyncAuthority::Commons {
            store,
            signing_seed,
            keys,
        }) => Ok((store, &**signing_seed, KnotSyncCipher::CommonsData(keys))),
        None => Err(error("composition sync authority unavailable")),
    }
}
fn target(
    persona: KnotPersonaDisplayV1,
    state: &super::VaultSource,
) -> Result<KnotRetainTargetV1, KnotRetainError> {
    let destination = super::capture_retention::destination_for_state(state).map_err(error)?;
    Ok(KnotRetainTargetV1 {
        persona,
        space_id: destination.space_id,
        writer: destination.writer,
        encryption: match destination.encryption {
            crate::KnotEncryptionProfile::PersonalVaultV1 => {
                KnotRetainEncryptionV1::PersonalVaultV1
            },
            crate::KnotEncryptionProfile::CommonsDataV1 => KnotRetainEncryptionV1::CommonsDataV1,
        },
    })
}
fn error(error: impl std::fmt::Display) -> KnotRetainError {
    KnotRetainError(error.to_string())
}
