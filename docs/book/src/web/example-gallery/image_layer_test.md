# Image Layer Test

Tests the rendering order of images vs primitives (paths, backgrounds).
This helps debug z-order issues where images may render above/below other elements.

**Solution for rendering elements ON TOP of images:**
Use `.foreground()` on any element that needs to render above images.
The render order is: Background primitives → Images → Foreground primitives

<div class="blinc-example" data-example="image_layer_test" data-title="Blinc image_layer_test example"></div>

> **Tip:** Some demos are best viewed in a full browser window. Click "Open in a new tab" below for the full experience.

[Open in a new tab](../../examples/image_layer_test/index.html) · [View source on GitHub](https://github.com/project-blinc/Blinc/blob/main/examples/blinc_app_examples/examples/image_layer_test.rs)
