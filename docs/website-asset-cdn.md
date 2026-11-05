# Website asset CDN plan

Status: proposed, 4 November 2026. No infrastructure or DNS changes made.

Use Cloudflare R2 Standard behind **assets.blinc.rs** for website assets. Keep the existing HTML, documentation and URLs on blinc.rs initially. This can improve delivery of recordings and the shared WASM runtime without coupling a hosting migration to the component gallery.

## What moves

| Content | Delivery | Cache policy |
| --- | --- | --- |
| GIF previews, PNG posters, photos, media, fonts, downloads | R2 custom domain, content-hashed keys | One year, immutable |
| WASM and its generated module payload | R2 custom domain, release-hashed keys | One year, immutable |
| Worker entry, host event routing and protocol scripts | Same origin as the page | Release-hashed |
| HTML and current build manifest | Existing website origin | Revalidate |

The current gallery baseline contains about 55 MiB of previews, 14 MiB of shared runtime and 5 MiB of source/sample assets before compression. The migration inventory must also include the existing example gallery and landing-page assets.

Cloudflare requires an R2 custom domain to serve cached public objects; r2.dev is intended for development. An explicit cache rule should cover WASM and other payloads, rather than relying on the default extension list. [R2 public buckets](https://developers.cloudflare.com/r2/buckets/public-buckets/), [default cache behavior](https://developers.cloudflare.com/cache/concepts/default-cache-behavior/).

## Generator and runtime changes

Introduce one optional build setting, `ASSET_BASE_URL`, defaulting to local paths. Resolve asset URLs centrally in the site generator and its manifest. Local development and pull-request previews must continue working without a Cloudflare account.

Use the existing artifact checksums to generate immutable object keys such as `components/previews/<content-hash>/preview.gif`. Preserve release-addressed runtime payloads. Never overwrite an object served with an immutable policy. A mutable query parameter on an overwritten path is insufficient as the deployment contract.

Keep `host.js`, `worker.js` and their routing modules on blinc.rs. `new Worker()` requires a same-origin entry script; moving the host module alone would change its relative worker URL. Pass the versioned payload base to that worker for CORS-enabled WASM/module imports. The media bridge must resolve its sample URLs through the same asset configuration. [Worker constructor restrictions](https://developer.mozilla.org/en-US/docs/Web/API/Worker/Worker).

Use correct MIME types, including `application/wasm`, JavaScript modules, GIF/PNG and fonts. Verify Brotli/gzip delivery for compressible payloads and `206`, `Content-Range`, `Accept-Ranges` and `Content-Length` for browser media seeking. Serve long-lived successful objects with `Cache-Control: public, max-age=31536000, immutable`; keep error responses on a short TTL.

Configure CORS for public GET/HEAD from production and explicitly supported preview origins, without credentials. Include Range requests and expose the response headers the media host needs. Add `Timing-Allow-Origin` if measuring cross-origin resource timings. If cross-origin isolation is introduced later, review CORP/COEP on all payloads together. [R2 CORS configuration](https://developers.cloudflare.com/r2/buckets/cors/).

## Delivery sequence and rollback

1. Build and verify the site locally, including every generated asset checksum.
2. Upload only versioned objects with a bucket-scoped CI token held in repository secrets.
3. Verify uploaded checksums, MIME/cache/CORS headers, sample downloads and media range requests.
4. Publish HTML/manifest references only after the object checks pass.
5. Retain previous releases for rollback and visitors with an older page open. Remove old assets through a retention policy after that window; avoid blanket purges or deleting an active release.

Start with recordings/images, then the shared runtime payload, then existing examples and landing-page assets. No change to public documentation routes is needed.

## Acceptance checks

Run isolated Chrome against two distinct origins. Check preview animation, background warmup, module/WASM loading, fonts, playback/seek, worker disposal and CSS/theme changes without loss of values. Verify both cold and warm requests, Cloudflare cache status, transfer size, TTFB and time to the first usable Live frame. Keep recordings paused offscreen.

CDN delivery does not reduce GIF decoding cost, WASM compilation time or bytes transferred on a cold visit. Smaller preview formats can be investigated separately while retaining the requested GIF exports.

## Cost and operations

R2 Standard charges for stored bytes and operations, with a free tier and no direct R2 egress charge. Do not assume a free bill from the small storage footprint: uncached reads, upload operations and optional paid services still matter. Set budget alerts and observe cache hit rate and request volume before wider rollout. Confirm current rates when provisioning. [R2 pricing](https://developers.cloudflare.com/r2/pricing/).

Before implementation, identify the Cloudflare account, zone ownership for blinc.rs, bucket name, preview origins and retention window. Those details are needed for provisioning, not for local gallery development.
