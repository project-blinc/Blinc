//! Connection component for the node graph system.
//!
//! Connections wire data flow between nodes by linking an output port
//! of one node to an input port of another.

use crate::ecs::{Component, Entity};
use serde::{Deserialize, Serialize};

/// Connects an output port of one node to an input port of another.
///
/// Connection entities are stored in the World alongside node entities.
/// When the node graph is evaluated, connections determine the data flow
/// between nodes.
///
/// # Example
///
/// ```rust,ignore
/// use blinc_3d::nodegraph::Connection;
///
/// // Connect player.position to camera.target
/// world.spawn().insert(Connection::new(
///     player, "position",
///     camera, "target",
/// ));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    /// Source entity (must have Node component)
    pub from: Entity,
    /// Name of the output port on the source node
    pub from_port: String,
    /// Target entity (must have Node component)
    pub to: Entity,
    /// Name of the input port on the target node
    pub to_port: String,
    /// Whether this connection is currently enabled
    pub enabled: bool,
}

impl Component for Connection {
    // Connections are relatively rare compared to other components,
    // so sparse storage is appropriate
    const STORAGE: crate::ecs::StorageType = crate::ecs::StorageType::Sparse;
}

impl Connection {
    /// Create a new connection between two nodes.
    ///
    /// # Arguments
    ///
    /// * `from` - The source entity (node with the output port)
    /// * `from_port` - Name of the output port on the source
    /// * `to` - The target entity (node with the input port)
    /// * `to_port` - Name of the input port on the target
    pub fn new(
        from: Entity,
        from_port: impl Into<String>,
        to: Entity,
        to_port: impl Into<String>,
    ) -> Self {
        Self {
            from,
            from_port: from_port.into(),
            to,
            to_port: to_port.into(),
            enabled: true,
        }
    }

    /// Create a disabled connection (won't transfer data until enabled).
    pub fn disabled(
        from: Entity,
        from_port: impl Into<String>,
        to: Entity,
        to_port: impl Into<String>,
    ) -> Self {
        Self {
            from,
            from_port: from_port.into(),
            to,
            to_port: to_port.into(),
            enabled: false,
        }
    }

    /// Enable this connection.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable this connection.
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Check if this connection involves a specific entity.
    pub fn involves(&self, entity: Entity) -> bool {
        self.from == entity || self.to == entity
    }

    /// Check if this connection goes from a specific entity.
    pub fn is_from(&self, entity: Entity) -> bool {
        self.from == entity
    }

    /// Check if this connection goes to a specific entity.
    pub fn is_to(&self, entity: Entity) -> bool {
        self.to == entity
    }

    /// Reverse the connection direction (swap from/to).
    pub fn reversed(&self) -> Self {
        Self {
            from: self.to,
            from_port: self.to_port.clone(),
            to: self.from,
            to_port: self.from_port.clone(),
            enabled: self.enabled,
        }
    }
}

/// Builder for creating multiple connections from a single source.
///
/// # Example
///
/// ```rust,ignore
/// let connections = ConnectionBuilder::from(player, "position")
///     .to(camera, "target")
///     .to(enemy, "chase_target")
///     .build();
///
/// for conn in connections {
///     world.spawn().insert(conn);
/// }
/// ```
pub struct ConnectionBuilder {
    from: Entity,
    from_port: String,
    targets: Vec<(Entity, String)>,
}

impl ConnectionBuilder {
    /// Start building connections from a source node and port.
    pub fn from(entity: Entity, port: impl Into<String>) -> Self {
        Self {
            from: entity,
            from_port: port.into(),
            targets: Vec::new(),
        }
    }

    /// Add a target for the connection.
    pub fn to(mut self, entity: Entity, port: impl Into<String>) -> Self {
        self.targets.push((entity, port.into()));
        self
    }

    /// Build all connections.
    pub fn build(self) -> Vec<Connection> {
        self.targets
            .into_iter()
            .map(|(to, to_port)| Connection::new(self.from, self.from_port.clone(), to, to_port))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_creation() {
        let conn = Connection::new(Entity::default(), "out", Entity::default(), "in");
        assert_eq!(conn.from_port, "out");
        assert_eq!(conn.to_port, "in");
        assert!(conn.enabled);
    }

    #[test]
    fn test_connection_disabled() {
        let conn = Connection::disabled(Entity::default(), "out", Entity::default(), "in");
        assert!(!conn.enabled);
    }

    #[test]
    fn test_connection_builder() {
        let source = Entity::default();
        let target1 = Entity::default();
        let target2 = Entity::default();

        let connections = ConnectionBuilder::from(source, "position")
            .to(target1, "target")
            .to(target2, "follow")
            .build();

        assert_eq!(connections.len(), 2);
        assert!(connections.iter().all(|c| c.from == source));
    }

    #[test]
    fn test_connection_serialization() {
        let from_entity = Entity::default();
        let to_entity = Entity::default();
        let conn = Connection::new(from_entity, "output", to_entity, "input");

        let json = serde_json::to_string(&conn).unwrap();
        let restored: Connection = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.from, from_entity);
        assert_eq!(restored.to, to_entity);
        assert_eq!(restored.from_port, "output");
        assert_eq!(restored.to_port, "input");
        assert!(restored.enabled);
    }
}
