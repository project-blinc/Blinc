//! The distance functions and colour helpers Blinc's SDF shaders share.

russl::russl! {
    pub fn apply_css_filter(color: Vec4, filter_a: Vec4, filter_b: Vec4) -> Vec4 {
        let mut rgb = color.rgb;

        // Grayscale: desaturate using luminance weights
        let grayscale = filter_a.x;
        if grayscale > 0.0 {
            let lum = dot(rgb, Vec3::new(0.2126, 0.7152, 0.0722));
            rgb = mix(rgb, Vec3::new(lum, lum, lum), grayscale);
        }

        // Sepia: apply sepia tone matrix
        let sepia = filter_a.z;
        if sepia > 0.0 {
            let sepia_r = dot(rgb, Vec3::new(0.393, 0.769, 0.189));
            let sepia_g = dot(rgb, Vec3::new(0.349, 0.686, 0.168));
            let sepia_b = dot(rgb, Vec3::new(0.272, 0.534, 0.131));
            rgb = mix(rgb, Vec3::new(sepia_r, sepia_g, sepia_b), sepia);
        }

        // Invert
        let invert = filter_a.y;
        if invert > 0.0 {
            rgb = mix(rgb, Vec3::splat(1.0) - rgb, invert);
        }

        // Hue-rotate: rotate in RGB space using rotation matrix
        let hue_rad = filter_a.w;
        if abs(hue_rad) > 0.001 {
            let cos_h = cos(hue_rad);
            let sin_h = sin(hue_rad);
            let w = Vec3::new(0.2126, 0.7152, 0.0722);
            // Rodrigues-style hue rotation matrix
            let r = Vec3::new(
                cos_h + (1.0 - cos_h) * w.x,
                (1.0 - cos_h) * w.x * w.y - sin_h * w.z,
                (1.0 - cos_h) * w.x * w.z + sin_h * w.y
            );
            let g = Vec3::new(
                (1.0 - cos_h) * w.x * w.y + sin_h * w.z,
                cos_h + (1.0 - cos_h) * w.y,
                (1.0 - cos_h) * w.y * w.z - sin_h * w.x
            );
            let b = Vec3::new(
                (1.0 - cos_h) * w.x * w.z - sin_h * w.y,
                (1.0 - cos_h) * w.y * w.z + sin_h * w.x,
                cos_h + (1.0 - cos_h) * w.z
            );
            rgb = Vec3::new(dot(rgb, r), dot(rgb, g), dot(rgb, b));
        }

        // Brightness
        let brightness = filter_b.x;
        rgb = rgb * brightness;

        // Contrast
        let contrast = filter_b.y;
        rgb = (rgb - Vec3::splat(0.5)) * contrast + Vec3::splat(0.5);

        // Saturate
        let saturate = filter_b.z;
        if abs(saturate - 1.0) > 0.001 {
            let lum = dot(rgb, Vec3::new(0.2126, 0.7152, 0.0722));
            rgb = mix(Vec3::new(lum, lum, lum), rgb, saturate);
        }

        return Vec4::new(clamp(rgb, Vec3::splat(0.0), Vec3::splat(1.0)), color.a);
    }

    // The colour of a border at `rel`, taken from the box's centre in its own
    // frame, when each side has its own. It is the side whose edge the point is
    // nearest in widths, so two sides meet on the diagonal at a corner as CSS
    // draws them. `widths` is top, right, bottom, left.
    pub fn border_side_color(
        rel: Vec2,
        half_size: Vec2,
        widths: Vec4,
        top: Vec4,
        right: Vec4,
        bottom: Vec4,
        left: Vec4
    ) -> Vec4 {
        let mut color = top;
        let mut nearest = (rel.y + half_size.y) / max(widths.x, 0.0001);
        let to_right = (half_size.x - rel.x) / max(widths.y, 0.0001);
        if to_right < nearest {
            nearest = to_right;
            color = right;
        }
        let to_bottom = (half_size.y - rel.y) / max(widths.z, 0.0001);
        if to_bottom < nearest {
            nearest = to_bottom;
            color = bottom;
        }
        if (rel.x + half_size.x) / max(widths.w, 0.0001) < nearest {
            color = left;
        }
        return color;
    }

    pub fn erf(x: f32) -> f32 {
        let s = sign(x);
        let a = abs(x);
        let t = 1.0 / (1.0 + 0.3275911 * a);
        let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * exp(-a * a);
        return s * y;
    }

    pub fn shadow_circle(p: Vec2, center: Vec2, radius: f32, sigma: f32) -> f32 {
        let dist = length(p - center);

        if sigma < 0.001 {
            // No blur - hard edge
            return if dist < radius { 1.0 } else { 0.0 };
        }

        // Gaussian falloff from circle edge
        // erf gives cumulative distribution, we want shadow inside and fading outside
        let d = 0.5 * sqrt(2.0) * sigma;
        return 0.5 * (1.0 + erf((radius - dist) / d));
    }

    // Corner radii of a box shadow grown by `spread`, as CSS does: a corner grows
    // by the spread only as far as it is already round, so a square corner stays
    // square.
    pub fn shadow_spread_radius(radius: Vec4, spread: f32) -> Vec4 {
        if spread > 0.0 {
            let u = min(radius / spread, Vec4::splat(1.0)) - Vec4::splat(1.0);
            return radius + spread * (Vec4::splat(1.0) + u * u * u);
        }
        return max(radius + Vec4::splat(spread), Vec4::splat(0.0));
    }

    pub fn quarter_ellipse_sdf(point: Vec2, radii: Vec2) -> f32 {
        // Avoid division by zero
        let safe_radii = max(radii, Vec2::splat(0.001));
        // Map to unit circle space
        let circle_vec = point / safe_radii;
        let unit_circle_sdf = length(circle_vec) - 1.0;
        // Scale back using average radius for distance approximation
        return unit_circle_sdf * (safe_radii.x + safe_radii.y) * -0.5;
    }

    pub fn sd_circle(p: Vec2, center: Vec2, radius: f32) -> f32 {
        return length(p - center) - radius;
    }

    pub fn sd_ellipse(p: Vec2, center: Vec2, radii: Vec2) -> f32 {
        let p_centered = p - center;
        let p_norm = p_centered / radii;
        let dist = length(p_norm);
        return (dist - 1.0) * min(radii.x, radii.y);
    }

    pub fn sd_rounded_rect(p: Vec2, origin: Vec2, size: Vec2, radius: Vec4) -> f32 {
        let half_size = size * 0.5;
        let center = origin + half_size;
        let rel = p - center;  // Relative position from center (signed)
        let q = abs(rel) - half_size;

        // Select corner radius based on quadrant
        // radius: (top-left, top-right, bottom-right, bottom-left)
        // In screen coords: Y increases downward, so rel.y < 0 means top half
        let mut r: f32;
        if rel.y < 0.0 {
            // Top half (y is above center)
            if rel.x > 0.0 {
                r = radius.y; // top-right
            } else {
                r = radius.x; // top-left
            }
        } else {
            // Bottom half (y is below center)
            if rel.x > 0.0 {
                r = radius.z; // bottom-right
            } else {
                r = radius.w; // bottom-left
            }
        }

        // Clamp radius to half the minimum dimension
        r = min(r, min(half_size.x, half_size.y));

        let q_adjusted = q + Vec2::splat(r);
        return length(max(q_adjusted, Vec2::splat(0.0))) + min(max(q_adjusted.x, q_adjusted.y), 0.0) - r;
    }

    pub fn superellipse_dist(t: Vec2, p: f32, r: f32) -> f32 {
        let m = max(t.x, t.y);
        if m <= 1e-20 {
            // At the corner's inner centre the arc is exactly r away, and
            // the box bounds the result anyway.
            return -r;
        }
        let a = t.x / m;
        let b = t.y / m;
        let s = pow(a, p) + pow(b, p);
        let sp = pow(s, 1.0 / p);
        let f = m * sp;
        // grad |t|_p = ((t.x/f)^(p-1), (t.y/f)^(p-1)), written against the
        // factored form so nothing divides by an underflowed f. pow(0, 0) is
        // undefined in WGSL and p == 1 would reach it on the axes, so the
        // bevel takes its own exact branch below.
        let g = Vec2::new(pow(a / sp, p - 1.0), pow(b / sp, p - 1.0));
        return (f - 1.0) * r / max(length(g), 1e-6);
    }

    pub fn sd_shaped_rect(p: Vec2, origin: Vec2, size: Vec2, radius: Vec4, shape: Vec4) -> f32 {
        let half_size = size * 0.5;
        let center = origin + half_size;
        let rel = p - center;
        let q = abs(rel) - half_size;

        // Select corner radius and shape based on quadrant
        let mut r: f32;
        let mut n: f32;
        if rel.y < 0.0 {
            if rel.x > 0.0 {
                r = radius.y; n = shape.y;  // top-right
            } else {
                r = radius.x; n = shape.x;  // top-left
            }
        } else {
            if rel.x > 0.0 {
                r = radius.z; n = shape.z;  // bottom-right
            } else {
                r = radius.w; n = shape.w;  // bottom-left
            }
        }

        r = min(r, min(half_size.x, half_size.y));

        // Sharp box, exact. Bounds every corner shape below, which is what
        // keeps the interior continuous without a region gate.
        let box_d = length(max(q, Vec2::splat(0.0))) + min(max(q.x, q.y), 0.0);
        let qa = q + Vec2::splat(r);
        let rr = max(r, 0.001);

        // Notch: the box less the unbounded quadrant past the step, so its
        // only edges are the step's two faces. A union of two bars
        // misreported depth near the reentrant corner.
        if n <= -100.0 {
            let nq = -qa;
            let cut = length(max(nq, Vec2::splat(0.0))) + min(max(nq.x, nq.y), 0.0);
            return max(box_d, -cut);
        }

        // Square: the box itself.
        if n >= 100.0 {
            return box_d;
        }

        // Circle: already exact, and cheaper than the general form.
        if abs(n - 1.0) < 0.01 {
            return length(max(qa, Vec2::splat(0.0))) + min(max(qa.x, qa.y), 0.0) - r;
        }

        // Scoop: a concave arc centred on the corner's TIP, carved out of the
        // box across the whole quadrant. Negating the convex superellipse
        // about the inner centre instead left the tip filled and cut a disc
        // out of the interior.
        if n < 0.0 {
            let p_exp = pow(2.0, min(abs(n), 5.0));
            let t = abs(q) / rr;
            return max(box_d, -superellipse_dist(t, p_exp, r));
        }

        // Bevel: exact, and keeps p == 1 out of the general path.
        if abs(n) < 0.01 {
            return max(box_d, (qa.x + qa.y - r) / sqrt(2.0));
        }

        // Convex superellipse, clamped to the corner quadrant and bounded by
        // the box.
        //
        // Outside the corner square the curve does not apply, and the general form below reduces
        // to these values exactly, so the interior and straight edges (nearly every fragment of a
        // large rounded rect) skip its `pow` calls. Past both axes the arc is exactly r away.
        if qa.x <= 0.0 && qa.y <= 0.0 {
            return max(box_d, -r);
        }
        if qa.x <= 0.0 || qa.y <= 0.0 {
            return box_d;
        }
        let p_exp = pow(2.0, min(abs(n), 5.0));
        let t = max(qa, Vec2::splat(0.0)) / rr;
        return max(box_d, superellipse_dist(t, p_exp, r));
    }
}
