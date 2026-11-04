// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl KnotSyncHost {
    /// Bind a transport for `signing_seed` and join `store`'s space.
    ///
    /// The transport key is the writer seed, so a device's node id and its
    /// author identity are the same value. That is what lets a paired writer
    /// serve as both the thing admitted and the thing dialled.
    pub async fn open(
        store: &KnotSyncFileStore,
        signing_seed: &[u8; 32],
        config: KnotSyncHostConfig,
    ) -> Result<Self, KnotSyncHostError> {
        Self::open_inner(store, signing_seed, config, None).await
    }

    /// Bind personal-device sync and serve clip evidence only to paired
    /// Personae-derived writer identities.
    pub async fn open_with_evidence(
        store: &KnotSyncFileStore,
        signing_seed: &[u8; 32],
        config: KnotSyncHostConfig,
        blobs: Arc<BlobStore>,
        max_artifact_bytes: u64,
    ) -> Result<Self, KnotSyncHostError> {
        if store.encryption_profile() != KnotEncryptionProfile::PersonalVaultV1 {
            return Err(KnotSyncHostError::Authority(
                "personal pairing cannot authorize a communal Knot space".into(),
            ));
        }
        let readers = BlobReadAuthorizer::new();
        Self::open_with_scoped_evidence(
            store,
            signing_seed,
            config,
            blobs,
            readers,
            max_artifact_bytes,
        )
        .await
    }

    /// Bind personal-device sync against a caller-shared serving authorizer.
    ///
    /// The content-retention actor receives the same handle and binds each
    /// retained hash to this space. This keeps one custody truth across local
    /// authoring and remote serving.
    pub async fn open_with_scoped_evidence(
        store: &KnotSyncFileStore,
        signing_seed: &[u8; 32],
        config: KnotSyncHostConfig,
        blobs: Arc<BlobStore>,
        readers: BlobReadAuthorizer,
        max_artifact_bytes: u64,
    ) -> Result<Self, KnotSyncHostError> {
        if store.encryption_profile() != KnotEncryptionProfile::PersonalVaultV1 {
            return Err(KnotSyncHostError::Authority(
                "personal pairing cannot authorize a communal Knot space".into(),
            ));
        }
        let scope = BlobScope::new(store.space_id());
        readers.replace_readers(scope, config.authority.evidence_readers());
        let sources = BlobPeerAuthorizer::from_peers(config.authority.evidence_sources());
        Self::open_inner(
            store,
            signing_seed,
            config,
            Some(KnotEvidenceHost {
                blobs,
                readers,
                scope,
                sources,
                max_artifact_bytes,
            }),
        )
        .await
    }

    /// Bind a communal space with an authorizer materialized from Gemot facts.
    pub async fn open_with_communal_evidence(
        store: &KnotSyncFileStore,
        signing_seed: &[u8; 32],
        config: KnotSyncHostConfig,
        blobs: Arc<BlobStore>,
        max_artifact_bytes: u64,
    ) -> Result<Self, KnotSyncHostError> {
        if store.encryption_profile() != KnotEncryptionProfile::CommonsDataV1 {
            return Err(KnotSyncHostError::Authority(
                "Gemot authority requires a communal Knot space".into(),
            ));
        }
        if config.authority.source() != KnotAuthoritySource::GemotCapabilities {
            return Err(KnotSyncHostError::Authority(
                "communal Knot host requires Gemot capability authority".into(),
            ));
        }
        if store.admitted_writers() != config.authority.writers().collect::<Vec<_>>() {
            return Err(KnotSyncHostError::Authority(
                "communal store writers do not match materialized Gemot authority".into(),
            ));
        }
        let sources = BlobPeerAuthorizer::from_peers(config.authority.evidence_sources());
        let scope = BlobScope::new(store.space_id());
        let readers = BlobReadAuthorizer::new();
        readers.replace_readers(scope, config.authority.evidence_readers());
        Self::open_inner(
            store,
            signing_seed,
            config,
            Some(KnotEvidenceHost {
                blobs,
                readers,
                scope,
                sources,
                max_artifact_bytes,
            }),
        )
        .await
    }

    pub(super) async fn open_inner(
        store: &KnotSyncFileStore,
        signing_seed: &[u8; 32],
        config: KnotSyncHostConfig,
        evidence: Option<KnotEvidenceHost>,
    ) -> Result<Self, KnotSyncHostError> {
        validate_authority_source(store.encryption_profile(), config.authority.source())?;
        // Borrowed into the builder's one cleared copy (Mere vault lock ruling 49).
        let mut builder = P2pandaTransport::builder_from_seed_ref(signing_seed).gossip();
        if let Some(evidence) = &evidence {
            builder =
                builder.scoped_blobs(&evidence.blobs, evidence.scope, evidence.readers.clone());
        }
        let network = P2pandaOverlayHost::bind(
            builder,
            sync_overlay_topic(store.space_id()),
            &P2pandaHostPolicy {
                relay_urls: config.relay_urls,
                ..P2pandaHostPolicy::default()
            },
        )
        .await
        .map_err(|error| KnotSyncHostError::Transport(error.to_string()))?;
        network
            .seed_peers(config.authority.writers())
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))?;

        // The cached-address rung, as Graphshell has it: a device that has
        // connected once can redial after both ends restart with no discovery
        // working at all.
        for (expected, hint) in config.authority.route_hints() {
            match network.add_peer_ticket(hint).await {
                Ok(peer) if peer.to_bytes() == *expected => {},
                Ok(peer) => tracing::warn!(
                    expected = %crate::hex32(expected),
                    actual = %crate::hex32(&peer.to_bytes()),
                    "a stored dial hint named another peer; skipping it"
                ),
                Err(error) => tracing::warn!(
                    %error,
                    "a stored dial hint was unusable; skipping it"
                ),
            }
        }

        let (endpoint, gossip) = network
            .transport()
            .sync_parts()
            .ok_or_else(|| KnotSyncHostError::Transport("gossip is unavailable".into()))?;
        let joined = store.join(endpoint, gossip).await?;
        Ok(Self {
            joined,
            network,
            store: store.clone(),
            space_id: store.space_id(),
            evidence,
            authority: config.authority,
        })
    }

    /// This device's node id, which is also its writer key: what the other
    /// devices must admit.
    pub fn node_id(&self) -> [u8; 32] {
        self.network.local_peer_id().to_bytes()
    }

    /// Stable Knot space carried by this host.
    pub fn space_id(&self) -> [u8; 32] {
        self.space_id
    }

    pub fn sync_status(&self) -> SyncStatus {
        self.joined.sync_status()
    }

    /// Leave LogSync and close the transport before a persistent blob store is
    /// reopened by another resident.
    pub async fn close(self) -> Result<(), KnotSyncHostError> {
        let Self {
            joined, network, ..
        } = self;
        joined.leave_and_wait().await?;
        network
            .close()
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))
    }

    /// Current evidence-fetch admission handle, when this host serves blobs.
    pub fn evidence_authorizer(&self) -> Option<BlobReadAuthorizer> {
        self.evidence
            .as_ref()
            .map(|evidence| evidence.readers.clone())
    }

    /// Bind an already-retained reference to this host's serving scope.
    ///
    /// Startup uses this while replaying resident documents whose evidence was
    /// retained on an earlier run. It changes authority only; bytes are not
    /// copied or opened a second time.
    pub fn retain_evidence_custody(
        &self,
        reference: &KnotClipEvidenceRef,
    ) -> Result<bool, KnotSyncHostError> {
        let evidence = self
            .evidence
            .as_ref()
            .ok_or_else(|| KnotSyncHostError::EvidenceBlob("blob serving is disabled".into()))?;
        let hash = reference
            .blob_hash()
            .map_err(KnotSyncHostError::EvidenceReference)?;
        Ok(evidence.readers.retain(evidence.scope, hash))
    }

    /// Revision currently applied by operation, evidence, and route policy.
    pub fn authority_revision(&self) -> [u8; 32] {
        self.authority.revision()
    }

    /// Apply one new authority materialization to every live consumer.
    pub async fn apply_authority(
        &mut self,
        next: KnotSpaceAuthoritySnapshot,
    ) -> Result<bool, KnotSyncHostError> {
        validate_authority_source(self.store.encryption_profile(), next.source())?;
        if self.authority.revision() == next.revision() {
            return Ok(false);
        }

        let previous_writers = self.authority.writers().collect::<BTreeSet<_>>();
        let next_writers = next.writers().collect::<BTreeSet<_>>();
        match next.source() {
            KnotAuthoritySource::PersonalPairing => {
                for writer in previous_writers.difference(&next_writers) {
                    self.store.deny_writer(writer);
                }
                for writer in next_writers.difference(&previous_writers).copied() {
                    self.store.admit_writer(writer);
                }
            },
            KnotAuthoritySource::GemotCapabilities => {
                self.store
                    .replace_admitted_writers(next_writers.iter().copied());
            },
        }
        if let Some(evidence) = &self.evidence {
            evidence
                .readers
                .replace_readers(evidence.scope, next.evidence_readers());
            evidence.sources.replace(next.evidence_sources());
        }

        for writer in previous_writers.difference(&next_writers).copied() {
            if let Err(error) = self.network.remove_peer(writer).await {
                tracing::warn!(
                    %error,
                    writer = %crate::hex32(&writer),
                    "authority was revoked but its stale route could not be detached"
                );
            }
        }
        for writer in next_writers.difference(&previous_writers).copied() {
            if let Err(error) = self.network.add_peer(writer).await {
                tracing::warn!(
                    %error,
                    writer = %crate::hex32(&writer),
                    "authority was granted but its route is not yet available"
                );
            }
        }
        for (expected, hint) in next.route_hints() {
            match self.network.add_peer_ticket(hint).await {
                Ok(peer) if peer.to_bytes() == *expected => {},
                Ok(peer) => tracing::warn!(
                    expected = %crate::hex32(expected),
                    actual = %crate::hex32(&peer.to_bytes()),
                    "an authority route hint named another peer; ignoring the route"
                ),
                Err(error) => tracing::warn!(
                    %error,
                    "an authority route hint was unavailable; authority still applied"
                ),
            }
        }
        self.authority = next;
        Ok(true)
    }

    /// Read and verify local evidence bytes before exposing them to a caller.
    pub async fn read_evidence(
        &self,
        reference: &KnotClipEvidenceRef,
    ) -> Result<Vec<u8>, KnotSyncHostError> {
        let evidence = self
            .evidence
            .as_ref()
            .ok_or_else(|| KnotSyncHostError::EvidenceBlob("blob serving is disabled".into()))?;
        if reference.byte_size > evidence.max_artifact_bytes {
            return Err(KnotSyncHostError::EvidenceTooLarge {
                actual: reference.byte_size,
                limit: evidence.max_artifact_bytes,
            });
        }
        let hash = reference
            .blob_hash()
            .map_err(KnotSyncHostError::EvidenceReference)?;
        let bytes = evidence
            .blobs
            .get_bytes(hash)
            .await
            .map_err(|error| KnotSyncHostError::EvidenceBlob(error.to_string()))?;
        reference
            .verify_bytes(&bytes)
            .map_err(KnotSyncHostError::EvidenceReference)?;
        Ok(bytes.to_vec())
    }

    /// Fetch one reference from an authorized peer and verify it before it can
    /// be read through [`Self::read_evidence`].
    pub async fn fetch_evidence(
        &self,
        reference: &KnotClipEvidenceRef,
        writer: [u8; 32],
    ) -> Result<KnotEvidenceFetchReceipt, KnotSyncHostError> {
        let evidence = self
            .evidence
            .as_ref()
            .ok_or_else(|| KnotSyncHostError::EvidenceBlob("blob serving is disabled".into()))?;
        if !evidence.sources.allows(&writer) {
            return Err(KnotSyncHostError::EvidenceUnauthorized);
        }
        if reference.byte_size > evidence.max_artifact_bytes {
            return Err(KnotSyncHostError::EvidenceTooLarge {
                actual: reference.byte_size,
                limit: evidence.max_artifact_bytes,
            });
        }
        let hash = reference
            .blob_hash()
            .map_err(KnotSyncHostError::EvidenceReference)?;
        if evidence
            .blobs
            .has(hash)
            .await
            .map_err(|error| KnotSyncHostError::EvidenceBlob(error.to_string()))?
        {
            self.read_evidence(reference).await?;
            evidence.readers.retain(evidence.scope, hash);
            return Ok(KnotEvidenceFetchReceipt {
                reference: reference.clone(),
                status: KnotEvidenceFetchStatus::AlreadyPresent,
            });
        }
        let peer = PeerID::from_bytes(&writer)
            .map_err(|error| KnotSyncHostError::EvidenceBlob(error.to_string()))?;
        let tag = evidence_tag(self.space_id, reference);
        evidence
            .blobs
            .fetch_from_named(self.network.transport(), peer, hash, tag.as_bytes())
            .await
            .map_err(|error| KnotSyncHostError::EvidenceBlob(error.to_string()))?;
        if let Err(error) = self.read_evidence(reference).await {
            let _ = evidence.blobs.release(tag.as_bytes()).await;
            return Err(error);
        }
        evidence.readers.retain(evidence.scope, hash);
        Ok(KnotEvidenceFetchReceipt {
            reference: reference.clone(),
            status: KnotEvidenceFetchStatus::Fetched,
        })
    }

    /// Resolve every clip evidence reference carried by a replicated Djot
    /// document from one authorized peer.
    pub async fn fetch_document_evidence(
        &self,
        document: &VaultDocument,
        writer: [u8; 32],
    ) -> Result<Vec<KnotEvidenceFetchReceipt>, KnotSyncHostError> {
        let references = clip_evidence_references(&document.body)
            .map_err(KnotSyncHostError::EvidenceReference)?;
        let mut receipts = Vec::with_capacity(references.len());
        for reference in references {
            receipts.push(self.fetch_evidence(&reference, writer).await?);
        }
        Ok(receipts)
    }

    /// A ticket for the across-network case a relay cannot serve. Rebuilt on
    /// every bind, so it is a bootstrap value and never a stored one.
    pub async fn ticket(&self) -> Result<String, KnotSyncHostError> {
        self.network
            .ticket()
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))
    }

    /// Which paired devices the transport currently associates with this
    /// space, and whether each is merely known or actually talking.
    ///
    /// Pairing records identity; this reports reachability, which is the fact
    /// a writer key cannot carry on its own.
    pub async fn known_peers(&self) -> Result<Vec<KnownPeer>, KnotSyncHostError> {
        self.network
            .known_peers()
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))
    }

    /// Where the endpoint currently believes `writer` lives, as a ticket, if
    /// it holds any addresses for it. The value the cached-address rung
    /// persists back into settings.
    pub async fn peer_ticket(&self, writer: [u8; 32]) -> Result<Option<String>, KnotSyncHostError> {
        self.network
            .peer_ticket(writer)
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))
    }

    /// Write back the addresses of devices this host is actually talking to.
    ///
    /// The other half of the cached-address rung: [`open`](Self::open) seeds
    /// stored hints, and this is what puts them there in the first place.
    /// Without it a hint only ever arrives if something outside Knot records
    /// one.
    ///
    /// Three disciplines, each of which the Graphshell lane learned the hard
    /// way:
    ///
    /// - **Connected peers only**, per [`writers_to_refresh`].
    /// - **Only on change.** [`KnotSyncHost::peer_ticket`] sorts addresses
    ///   before serialising, so an unchanged address set yields an identical
    ///   string and costs no settings write.
    /// - **Reload before saving.** The settings file has a second writer: a
    ///   `--pair-writer` invocation can land between the caller's read and
    ///   this write, so the refresh loads the latest, modifies, and saves
    ///   rather than persisting a snapshot taken seconds ago.
    pub async fn refresh_dial_hints(&self, settings_file: &std::path::Path) {
        let peers = match self.known_peers().await {
            Ok(peers) => peers,
            Err(error) => {
                tracing::warn!(%error, "could not read the peer directory");
                return;
            },
        };

        for writer in writers_to_refresh(&peers) {
            let ticket = match self.peer_ticket(writer).await {
                Ok(Some(ticket)) => ticket,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(%error, "could not read a peer's current address");
                    continue;
                },
            };
            if self.authority.route_hint(&writer) == Some(ticket.as_str()) {
                continue;
            }
            let mut latest = match crate::KnotSettings::load(settings_file) {
                Ok(latest) => latest,
                Err(error) => {
                    tracing::warn!(%error, "could not reload settings to refresh a hint");
                    continue;
                },
            };
            let Some(live) = latest.sync.as_mut() else {
                continue;
            };
            // `remember_endpoint` ignores a writer that is no longer paired,
            // so an unpair landing in this window cannot be undone by a route.
            if !live.remember_endpoint(writer, &ticket) {
                continue;
            }
            match latest.save(settings_file) {
                Ok(()) => tracing::info!(
                    writer = %crate::hex32(&writer),
                    "recorded a fresh dial hint for a connected device"
                ),
                Err(error) => tracing::warn!(%error, "could not persist a refreshed dial hint"),
            }
        }
    }
}
