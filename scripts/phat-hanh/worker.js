// Hir-Login update channel on Cloudflare (Worker `hirlogin-cap-nhat`, KV bound as KHO).
// Deployed by scripts/phat-hanh/dung-worker.mjs; filled by scripts/phat-hanh/windows.mjs (this PC)
// and .github/workflows/release-mac.yml (GitHub). Windows and Mac are separate channels, so one
// can be released (and tested) before the other.
//
//   GET  /<target>/<arch>/latest.json   Tauri updater manifest for that platform
//                                       (target = windows | darwin, arch = x86_64 | aarch64),
//                                       204 while that platform has no release here.
//   GET  /file/<name>                   an installer / update bundle
//   GET  /tai/windows | /tai/mac        the newest installer, for handing to new users
//   PUT  /upload/<key>                  store <key> (file:… | latest:… | tai:…); header
//                                       x-upload-key must match the UPLOAD_KEY secret

const KEYS = /^(file:[A-Za-z0-9._-]+|latest:(windows|darwin|linux)-(x86_64|aarch64)|tai:(windows|mac))$/;

export default {
  async fetch(req, env) {
    const url = new URL(req.url);
    const p = url.pathname.split('/').filter(Boolean).map(decodeURIComponent);

    if (req.method === 'GET' && p.length === 3 && p[2] === 'latest.json') {
      const v = await env.KHO.get('latest:' + p[0] + '-' + p[1]);
      if (!v) return new Response(null, { status: 204 });
      return new Response(v, { headers: { 'content-type': 'application/json', 'cache-control': 'no-store' } });
    }

    if (req.method === 'GET' && p.length === 2 && p[0] === 'file') {
      const { value, metadata } = await env.KHO.getWithMetadata('file:' + p[1], 'stream');
      if (!value) return new Response('Không có file này', { status: 404 });
      return new Response(value, {
        headers: {
          'content-type': (metadata && metadata.type) || 'application/octet-stream',
          'content-disposition': 'attachment; filename="' + p[1] + '"',
          'cache-control': 'public, max-age=31536000, immutable',
        },
      });
    }

    if (req.method === 'GET' && p.length === 2 && p[0] === 'tai') {
      const to = await env.KHO.get('tai:' + p[1]);
      if (!to) return new Response('Chưa có bản cài cho ' + p[1], { status: 404 });
      return Response.redirect(to, 302);
    }

    if (req.method === 'PUT' && p.length === 2 && p[0] === 'upload') {
      if (!env.UPLOAD_KEY || req.headers.get('x-upload-key') !== env.UPLOAD_KEY) return new Response('forbidden', { status: 403 });
      if (!KEYS.test(p[1])) return new Response('bad key', { status: 400 });
      const body = await req.arrayBuffer();
      if (body.byteLength > 25 * 1024 * 1024) return new Response('too large for KV (25 MB)', { status: 413 });
      const type = req.headers.get('content-type') || 'application/octet-stream';
      await env.KHO.put(p[1], body, { metadata: { type, size: body.byteLength, at: new Date().toISOString() } });
      return Response.json({ ok: true, key: p[1], size: body.byteLength });
    }

    if (req.method === 'GET' && p.length === 0) return new Response('Hir-Login update channel');
    return new Response('not found', { status: 404 });
  },
};
