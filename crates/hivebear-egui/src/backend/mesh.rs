use crate::error::CmdResult;
use crate::state::AppState;

/// Live connection status (as opposed to the mesh config).
#[derive(Clone, Debug, Default)]
pub struct MeshConnectionStatus {
    /// Whether the mesh node is listening locally.
    ///
    /// This is NOT the same as being on the hive — see `registered`. Showing
    /// "Connected to Hive" off this flag alone stayed true while the
    /// coordination server was unreachable.
    pub running: bool,
    /// Whether the coordination server has acknowledged this node.
    pub registered: bool,
    /// Number of directly connected peers.
    pub peer_count: usize,
    /// The node's hex-encoded public key (if running).
    pub node_id: Option<String>,
    /// Why the last start attempt failed, if it did.
    pub last_error: Option<String>,
}

/// Start the mesh node: register with the coordination server and begin heartbeats.
///
/// Clicking Join is the opt-in. The mesh is off by default (so a first
/// launch never binds a socket or raises a firewall prompt), so joining also
/// turns it on and remembers to rejoin at the next launch.
pub fn join_mesh(state: &AppState) -> CmdResult<MeshConnectionStatus> {
    set_mesh_opt_in(state, true)?;
    state.start_mesh()?;
    Ok(connection_status(state))
}

/// Stop the mesh node: deregister and disconnect, and stop rejoining at launch.
pub async fn leave_mesh(state: &AppState) -> CmdResult<()> {
    state.stop_mesh().await?;
    set_mesh_opt_in(state, false)
}

/// Flip the Settings-level mesh switch. Disabling also takes the node off
/// the hive now rather than at the next restart.
pub async fn set_mesh_enabled(state: &AppState, enabled: bool) -> CmdResult<()> {
    {
        let mut config = state
            .config
            .lock()
            .map_err(|_| String::from("Config lock poisoned"))?;
        config.mesh.enabled = enabled;
        config
            .save()
            .map_err(|e| format!("Failed to save config: {e}"))?;
    }
    if !enabled && connection_status(state).running {
        leave_mesh(state).await?;
    }
    Ok(())
}

/// Persist whether the user wants to be on the mesh.
///
/// Leaving keeps `enabled` as it is (that is the Settings switch) and only
/// clears `auto_join`; joining sets both.
fn set_mesh_opt_in(state: &AppState, joined: bool) -> CmdResult<()> {
    let mut config = state
        .config
        .lock()
        .map_err(|_| String::from("Config lock poisoned"))?;
    let enabled = config.mesh.enabled || joined;
    if config.mesh.enabled == enabled && config.mesh.auto_join == joined {
        return Ok(());
    }
    config.mesh.enabled = enabled;
    config.mesh.auto_join = joined;
    config
        .save()
        .map_err(|e| format!("Failed to save config: {e}"))
}

/// The live mesh connection status. Cheap: reads in-memory state only.
pub fn connection_status(state: &AppState) -> MeshConnectionStatus {
    let last_error = state
        .mesh_error
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let node = state.mesh_node.lock().unwrap_or_else(|e| e.into_inner());
    match node.as_ref() {
        Some(n) => MeshConnectionStatus {
            running: n.is_running(),
            registered: n.is_registered(),
            peer_count: n.peer_count(),
            node_id: Some(n.local_id.to_hex()),
            last_error,
        },
        None => MeshConnectionStatus {
            last_error,
            ..Default::default()
        },
    }
}
