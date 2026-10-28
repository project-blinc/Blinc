//! Named outputs live until their ordered paint pass ends.
use super::{Bounds, GpuRenderer, LayerTexture, Node};
use crate::{
    GpuImageInstance,
    primitives::{DynamicImage, RetainedImage},
};
use blinc_core::{LayerConfig, LayerId, Rect};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub(super) struct Outputs {
    pub wanted: HashSet<LayerId>,
    pub textures: HashMap<LayerId, Output>,
}

impl Outputs {
    pub fn new(items: &[Node]) -> Self {
        fn visit(items: &[Node], wanted: &mut HashSet<LayerId>) {
            for node in items {
                match node {
                    Node::Sample(sample) => {
                        wanted.insert(sample.id);
                    }
                    Node::Layer(_, children) => visit(children, wanted),
                    _ => {}
                }
            }
        }
        let mut wanted = HashSet::new();
        visit(items, &mut wanted);
        Self {
            wanted,
            textures: HashMap::new(),
        }
    }
}

pub(super) struct Output {
    pub texture: LayerTexture,
    pub origin: [f32; 2],
    pub local_origin: [f32; 2],
    pub opacity: f32,
}

pub(super) struct Sample {
    pub id: LayerId,
    pub source: Rect,
    pub paint: DynamicImage,
}

impl Sample {
    pub fn new(id: LayerId, source: Rect, dest: Rect, paint: Option<&DynamicImage>) -> Self {
        Self {
            id,
            source,
            paint: paint.cloned().unwrap_or_else(|| DynamicImage {
                paint_order: None,
                retained: None,
                data: Vec::new(),
                width: 1,
                height: 1,
                dest,
                transform: [1.0, 0.0, 0.0, 1.0],
                opacity: 1.0,
                corner_radius: 0.0,
                clip_bounds: [-10000.0, -10000.0, 100000.0, 100000.0],
                clip_radius: [0.0; 4],
                clip_aux: Vec::new(),
            }),
        }
    }
}

/// Named output includes transparent margins and remains available offscreen.
pub(super) fn frame(config: &LayerConfig, viewport: Bounds) -> Option<Bounds> {
    let min = config.position.map_or(viewport.min, |p| [p.x, p.y]);
    let size = config.size.map_or(
        [
            viewport.max[0] - viewport.min[0],
            viewport.max[1] - viewport.min[1],
        ],
        |s| [s.width, s.height],
    );
    let max = [min[0] + size[0], min[1] + size[1]];
    (min.into_iter().chain(max).all(f32::is_finite) && size[0] > 0.0 && size[1] > 0.0)
        .then_some(Bounds { min, max })
}

impl GpuRenderer {
    pub(super) fn draw_named_sample(
        &mut self,
        target: &wgpu::TextureView,
        sample: &Sample,
        origin: [f32; 2],
        outputs: &Outputs,
    ) {
        let Some(output) = outputs.textures.get(&sample.id) else {
            return;
        };
        let source = sample.source;
        let x = source.x() + output.local_origin[0] - output.origin[0];
        let y = source.y() + output.local_origin[1] - output.origin[1];
        let (w, h) = output.texture.size;
        if ![x, y, source.width(), source.height()]
            .into_iter()
            .all(f32::is_finite)
            || source.width() <= 0.0
            || source.height() <= 0.0
            || x < 0.0
            || y < 0.0
            || x + source.width() > w as f32
            || y + source.height() > h as f32
        {
            return;
        }
        let mut image = sample.paint.clone();
        image.width = w;
        image.height = h;
        image.opacity *= output.opacity;
        image.retained = Some(Arc::new(RetainedImage {
            view: output.texture.view.clone(),
            style: GpuImageInstance {
                src_uv: [
                    x / w as f32,
                    y / h as f32,
                    (x + source.width()) / w as f32,
                    (y + source.height()) / h as f32,
                ],
                ..Default::default()
            },
        }));
        self.render_dynamic_image_at(target, &image, origin);
    }
}
