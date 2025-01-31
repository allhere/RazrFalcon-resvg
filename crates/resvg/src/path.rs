// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::rc::Rc;

use crate::paint_server::Paint;
use crate::render::Context;
use crate::tree::{Node, OptionLog};

pub struct FillPath {
    pub paint: Paint,
    pub rule: tiny_skia::FillRule,
    pub anti_alias: bool,
    pub path: Rc<tiny_skia::Path>,
}

pub struct StrokePath {
    pub paint: Paint,
    pub stroke: tiny_skia::Stroke,
    pub anti_alias: bool,
    pub path: Rc<tiny_skia::Path>,
}

pub fn convert(
    upath: &usvg::Path,
    text_bbox: Option<tiny_skia::NonZeroRect>,
    children: &mut Vec<Node>,
) -> Option<usvg::BBox> {
    let anti_alias = upath.rendering_mode.use_shape_antialiasing();

    let mut bounding_box = upath.bounding_box.log_none(|| {
        log::warn!(
            "Node bounding box should be already calculated. \
            See `Tree::calculate_bounding_boxes`"
        )
    })?;
    if let Some(text_bbox) = text_bbox {
        bounding_box = text_bbox.to_rect();
    }

    let fill_path = upath
        .fill
        .as_ref()
        .and_then(|ufill| convert_fill_path(ufill, upath.data.clone(), bounding_box, anti_alias));

    let stroke_path = upath.stroke.as_ref().and_then(|ustroke| {
        convert_stroke_path(ustroke, upath.data.clone(), bounding_box, anti_alias)
    });

    if fill_path.is_none() && stroke_path.is_none() {
        return None;
    }

    let mut layer_bbox = usvg::BBox::from(bounding_box);

    if stroke_path.is_some() {
        if let Some(stroke_bbox) = upath.stroke_bounding_box {
            layer_bbox = layer_bbox.expand(stroke_bbox);
        }
    }

    // Do not add hidden paths, but preserve the bbox.
    // visibility=hidden still affects the bbox calculation.
    if upath.visibility != usvg::Visibility::Visible {
        return Some(layer_bbox);
    }

    if upath.paint_order == usvg::PaintOrder::FillAndStroke {
        if let Some(path) = fill_path {
            children.push(Node::FillPath(path));
        }

        if let Some(path) = stroke_path {
            children.push(Node::StrokePath(path));
        }
    } else {
        if let Some(path) = stroke_path {
            children.push(Node::StrokePath(path));
        }

        if let Some(path) = fill_path {
            children.push(Node::FillPath(path));
        }
    }

    Some(layer_bbox)
}

fn convert_fill_path(
    ufill: &usvg::Fill,
    path: Rc<tiny_skia::Path>,
    object_bbox: tiny_skia::Rect,
    anti_alias: bool,
) -> Option<FillPath> {
    // Horizontal and vertical lines cannot be filled. Skip.
    if path.bounds().width() == 0.0 || path.bounds().height() == 0.0 {
        return None;
    }

    let rule = match ufill.rule {
        usvg::FillRule::NonZero => tiny_skia::FillRule::Winding,
        usvg::FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
    };

    let paint =
        crate::paint_server::convert(&ufill.paint, ufill.opacity, object_bbox.to_non_zero_rect())?;

    let path = FillPath {
        paint,
        rule,
        anti_alias,
        path,
    };

    Some(path)
}

fn convert_stroke_path(
    ustroke: &usvg::Stroke,
    path: Rc<tiny_skia::Path>,
    object_bbox: tiny_skia::Rect,
    anti_alias: bool,
) -> Option<StrokePath> {
    // Zero-sized stroke path is not an error, because linecap round or square
    // would produce the shape either way.
    // TODO: Find a better way to handle it.

    let paint = crate::paint_server::convert(
        &ustroke.paint,
        ustroke.opacity,
        object_bbox.to_non_zero_rect(),
    )?;

    let path = StrokePath {
        paint,
        stroke: ustroke.to_tiny_skia(),
        anti_alias,
        path,
    };

    Some(path)
}

pub fn render_fill_path(
    path: &FillPath,
    blend_mode: tiny_skia::BlendMode,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) -> Option<()> {
    let pattern_pixmap;
    let mut paint = tiny_skia::Paint::default();
    match path.paint {
        Paint::Shader(ref shader) => {
            paint.shader = shader.clone(); // TODO: avoid clone
        }
        Paint::Pattern(ref pattern) => {
            let (patt_pix, patt_ts) =
                crate::paint_server::prepare_pattern_pixmap(pattern, ctx, transform)?;

            pattern_pixmap = patt_pix;
            paint.shader = tiny_skia::Pattern::new(
                pattern_pixmap.as_ref(),
                tiny_skia::SpreadMode::Repeat,
                tiny_skia::FilterQuality::Bicubic,
                pattern.opacity.get(),
                patt_ts,
            )
        }
    }

    paint.anti_alias = path.anti_alias;
    paint.blend_mode = blend_mode;

    pixmap.fill_path(&path.path, &paint, path.rule, transform, None);

    Some(())
}

pub fn render_stroke_path(
    path: &StrokePath,
    blend_mode: tiny_skia::BlendMode,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) -> Option<()> {
    let pattern_pixmap;
    let mut paint = tiny_skia::Paint::default();
    match path.paint {
        Paint::Shader(ref shader) => {
            paint.shader = shader.clone(); // TODO: avoid clone
        }
        Paint::Pattern(ref pattern) => {
            let (patt_pix, patt_ts) =
                crate::paint_server::prepare_pattern_pixmap(pattern, ctx, transform)?;

            pattern_pixmap = patt_pix;
            paint.shader = tiny_skia::Pattern::new(
                pattern_pixmap.as_ref(),
                tiny_skia::SpreadMode::Repeat,
                tiny_skia::FilterQuality::Bicubic,
                pattern.opacity.get(),
                patt_ts,
            )
        }
    }

    paint.anti_alias = path.anti_alias;
    paint.blend_mode = blend_mode;

    // TODO: fallback to a stroked path when possible

    pixmap.stroke_path(&path.path, &paint, &path.stroke, transform, None);

    Some(())
}
