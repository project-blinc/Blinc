//! Project foreground draws into the ordered renderer's local cursor space.
use super::{LayerCommand, LayerCommandEntry, PrimitiveBatch};

impl PrimitiveBatch {
    /// Whether the separately composed foreground stream has content.
    pub fn has_foreground_content(&self) -> bool {
        !self.foreground_primitives.is_empty()
            || !self.foreground_paths.indices.is_empty()
            || !self.foreground_dynamic_images.is_empty()
    }

    /// Whether foreground command indices must survive a cache update.
    pub fn has_foreground_layers(&self) -> bool {
        !self.foreground_layer_commands.is_empty()
    }

    /// Record a command using foreground-local primitive and path cursors.
    pub fn push_foreground_layer_command(&mut self, command: LayerCommand) {
        self.foreground_layer_commands.push(LayerCommandEntry {
            primitive_index: self.foreground_primitives.len(),
            path_vertex_index: self.foreground_paths.vertices.len(),
            path_index_count: self.foreground_paths.indices.len(),
            foreground_primitive_index: self.foreground_primitives.len(),
            foreground_path_vertex_index: self.foreground_paths.vertices.len(),
            foreground_path_index_count: self.foreground_paths.indices.len(),
            command,
        });
    }

    /// Move foreground draws and commands into a renderable batch. Aux rows
    /// retain their original offsets; separate prepared text stays outside.
    pub fn take_foreground(&mut self) -> Self {
        let mut foreground = Self::new();
        foreground.primitives = std::mem::take(&mut self.foreground_primitives);
        foreground.paths = std::mem::take(&mut self.foreground_paths);
        foreground.dynamic_images = std::mem::take(&mut self.foreground_dynamic_images);
        foreground.layer_commands = std::mem::take(&mut self.foreground_layer_commands);
        if !foreground.primitives.is_empty() || !foreground.paths.indices.is_empty() {
            foreground.aux_data = self.aux_data.clone();
        }
        foreground
    }

    /// Snapshot the foreground stream for a separate overlay dispatch.
    pub fn foreground_batch(&self) -> Self {
        let mut foreground = Self::new();
        foreground.primitives = self.foreground_primitives.clone();
        foreground.paths = self.foreground_paths.clone();
        foreground.dynamic_images = self.foreground_dynamic_images.clone();
        foreground.layer_commands = self.foreground_layer_commands.clone();
        if self.has_foreground_content() {
            foreground.aux_data = self.aux_data.clone();
        }
        foreground
    }
}
