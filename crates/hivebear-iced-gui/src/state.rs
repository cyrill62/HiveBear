use hivebear_core::{AppPaths, Config, HardwareProfile};
use hivebear_inference::Orchestrator;
use hivebear_mesh::MeshNode;
use hivebear_persistence::ChatDatabase;
use hivebear_registry::Registry;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

/// Shared backend state. Built once on a background thread, then handed to
/// the UI as an `Arc` and cloned into every spawned task.
pub struct AppState {
    pub config: Mutex<Config>,
    pub profile: HardwareProfile,
    pub orchestrator: Orchestrator,
    pub registry: Registry,
    pub chat_db: ChatDatabase,
    pub paths: AppPaths,
    pub http_client: reqwest::Client,
    pub mesh_node: Mutex<Option<Arc<MeshNode>>>,
    /// Why the last background mesh start failed, if it did. The start runs
    /// detached, so without this a failed bind or registration was only a
    /// log line and the UI could not say why the node never came up.
    pub mesh_error: Arc<Mutex<Option<String>>>,
    rt: tokio::runtime::Handle,
}

impl AppState {
    /// Runs on a background thread (see `ui::HiveBearApp::new`), since
    /// hardware profiling and opening the registry can take seconds and the
    /// window should paint meanwhile.
    ///
    /// Returns an error rather than panicking: these steps all touch the
    /// filesystem, and the UI shows the message instead of the process
    /// silently vanishing.
    pub fn init_with_paths(paths: AppPaths, rt: tokio::runtime::Handle) -> Result<Self, String> {
        paths.ensure_dirs().map_err(|e| {
            format!(
                "Could not create HiveBear's data directories under {}.\n\n{e}",
                paths.data_dir.display()
            )
        })?;

        let config = Config::load();
        let profile = hivebear_core::profile();
        let orchestrator = Orchestrator::with_config(profile.clone(), &config);

        // A corrupt registry index or chat database used to be fatal, which
        // meant the app died during setup with no way for the user to
        // recover. Both are caches of recoverable state, so quarantine the bad
        // file and rebuild instead of refusing to start.
        let registry_file = paths.data_dir.join("registry.json");
        let registry = match rt.block_on(Registry::new(&config, &paths)) {
            Ok(r) => r,
            Err(first) => {
                warn!("Model registry failed to open ({first}); quarantining and rebuilding");
                quarantine(&registry_file);
                rt.block_on(Registry::new(&config, &paths)).map_err(|e| {
                    format!(
                        "Could not open the model registry at {}, even after moving \
                         the existing one aside.\n\n{e}",
                        registry_file.display()
                    )
                })?
            }
        };

        let chat_db = match ChatDatabase::open(&paths.db_file) {
            Ok(db) => db,
            Err(first) => {
                warn!("Chat database failed to open ({first}); quarantining and rebuilding");
                quarantine(&paths.db_file);
                ChatDatabase::open(&paths.db_file).map_err(|e| {
                    format!(
                        "Could not open the chat database at {}, even after moving \
                         the existing one aside.\n\n{e}",
                        paths.db_file.display()
                    )
                })?
            }
        };

        Ok(Self {
            config: Mutex::new(config),
            profile,
            orchestrator,
            registry,
            chat_db,
            paths,
            http_client: reqwest::Client::new(),
            mesh_node: Mutex::new(None),
            mesh_error: Arc::new(Mutex::new(None)),
            rt,
        })
    }

    /// A snapshot of the current configuration.
    pub fn config(&self) -> Config {
        self.config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Start the mesh node: register with the coordination server and begin heartbeats.
    ///
    /// Uses the same persistent Ed25519 identity as device-key auth.
    /// Safe to call multiple times — no-ops if already running.
    pub fn start_mesh(&self) -> Result<(), String> {
        {
            let existing = self.mesh_node.lock().unwrap_or_else(|e| e.into_inner());
            if existing.as_ref().is_some_and(|n| n.is_running()) {
                return Ok(());
            }
        }

        let config = self.config();
        if !config.mesh.enabled {
            return Err("Mesh is disabled in settings".into());
        }

        let tier = hivebear_mesh::MeshTier::from_str_lossy(&config.mesh.tier);
        let identity_path = self.paths.data_dir.join("node_identity.key");
        let identity = hivebear_mesh::NodeIdentity::load_or_generate(&identity_path)
            .map_err(|e| format!("Failed to load identity: {e}"))?;

        let security_mode = hivebear_mesh::MeshSecurityMode::default();
        let transport: Arc<dyn hivebear_mesh::MeshTransport> =
            Arc::new(hivebear_mesh::transport::quic::QuicTransport::new(
                identity.node_id.clone(),
                security_mode,
                None,
            ));
        // Hand the client the identity: /register demands a signature over
        // "register:{node_id}:{timestamp}" and rejects anything without one.
        let discovery: Arc<dyn hivebear_mesh::discovery::PeerDiscovery> = Arc::new(
            hivebear_mesh::discovery::server::CoordinationServerClient::with_identity(
                config.mesh.coordination_server.clone(),
                Arc::new(identity.clone()),
            ),
        );

        let reputation_path = Some(self.paths.data_dir.join("reputation.json"));
        let node = Arc::new(MeshNode::with_identity(
            identity,
            transport,
            discovery,
            tier,
            reputation_path,
        ));

        let listen_addr: std::net::SocketAddr = format!("0.0.0.0:{}", config.mesh.port)
            .parse()
            .map_err(|e| format!("Invalid mesh port {}: {e}", config.mesh.port))?;
        let total_vram: u64 = self.profile.gpus.iter().map(|g| g.vram_bytes).sum();

        let local_info = hivebear_mesh::PeerInfo {
            node_id: node.local_id.clone(),
            hardware: self.profile.clone(),
            available_memory_bytes: self.profile.memory.available_bytes,
            available_vram_bytes: total_vram,
            network_bandwidth_mbps: 100.0,
            latency_ms: None,
            tier,
            reputation_score: 1.0,
            addr: listen_addr,
            external_addr: None,
            nat_type: hivebear_mesh::NatType::Unknown,
            latency_map: std::collections::HashMap::new(),
            serving_model_id: None,
            swarm_id: None,
            draft_capability: None,
        };

        // Start in the background — never blocks the caller, which is often
        // the UI thread. Spawned on the runtime handle explicitly because the
        // caller is not itself inside the runtime.
        let mesh = Arc::clone(&node);
        let mesh_error = Arc::clone(&self.mesh_error);
        *mesh_error.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.rt.spawn(async move {
            match mesh.start(listen_addr, local_info).await {
                Ok(()) => {
                    mesh.start_maintenance();
                    info!("Mesh node {} running in background", mesh.local_id);
                }
                Err(e) => {
                    warn!("Background mesh start failed (non-fatal): {e}");
                    *mesh_error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e.to_string());
                }
            }
        });
        info!("Mesh node starting in background");

        *self.mesh_node.lock().unwrap_or_else(|e| e.into_inner()) = Some(node);
        Ok(())
    }

    /// Stop the mesh node gracefully: deregister and disconnect all peers.
    pub async fn stop_mesh(&self) -> Result<(), String> {
        let node = self
            .mesh_node
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(node) = node {
            node.stop()
                .await
                .map_err(|e| format!("Failed to stop mesh: {e}"))?;
            info!("Mesh node stopped");
        }
        Ok(())
    }
}

/// Move a file out of the way so it can be recreated from scratch, keeping the
/// original for diagnosis. Best-effort: if the rename fails there is nothing
/// useful left to do, and the caller reports the follow-up error.
fn quarantine(path: &std::path::Path) {
    if !path.exists() {
        return;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut backup = path.to_path_buf();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    backup.set_file_name(format!("{name}.corrupt-{stamp}"));

    match std::fs::rename(path, &backup) {
        Ok(()) => warn!("Moved {} to {}", path.display(), backup.display()),
        Err(e) => warn!("Could not move {} aside: {e}", path.display()),
    }
}
