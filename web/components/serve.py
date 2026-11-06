#!/usr/bin/env python3
"""Serve the staged gallery with byte ranges for real browser media seeking."""
import argparse
import functools
import http.server
import io
import json
import os
import re
import time
from email.utils import parsedate_to_datetime
from pathlib import Path
from urllib.parse import unquote, urlsplit

WATCHED = ('.html', '.css', '.js', '.mjs')
# Large generated trees that no edit of a page or script touches.
SKIPPED = {'previews', 'releases', 'source', 'docs', 'node_modules', 'tests', 'examples'}
RELOAD = b"""<script>(function(){var seen;function poll(){fetch('/__live',{cache:'no-store'}).then(function(r){return r.json()}).then(function(j){if(seen===undefined){seen=j.stamp}else if(j.stamp!==seen){location.reload();return}}).catch(function(){}).then(function(){setTimeout(poll,1000)})}poll()})()</script>"""

class GalleryHandler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, '.wasm':'application/wasm'}
    # Source files that stand in front of the staged copy, and whether pages reload when either changes.
    source = None
    live = False
    _stamp = (0.0, 0.0)

    def translate_path(self, path):
        staged = super().translate_path(path)
        if self.source is None:
            return staged
        relative = unquote(urlsplit(path).path).lstrip('/')
        candidate = (Path(self.source) / relative).resolve()
        try:
            candidate.relative_to(Path(self.source).resolve())
        except ValueError:
            return staged
        return str(candidate) if candidate.is_file() else staged

    def end_headers(self):
        if getattr(self, '_file_response', False):
            self.send_header('Accept-Ranges', 'bytes')
        # A preview server shows what is on disk now: browsers revalidate rather than reuse a copy.
        self.send_header('Cache-Control', 'no-cache')
        super().end_headers()

    def stamp(self):
        """The newest modification time of any watched page or script, cached briefly."""
        now = time.monotonic()
        cached, at = GalleryHandler._stamp
        if now - at < 0.4:
            return cached
        newest = 0.0
        for root in filter(None, [self.source, self.directory]):
            for folder, names, files in os.walk(root):
                names[:] = [n for n in names if n not in SKIPPED]
                for name in files:
                    if name.endswith(WATCHED):
                        try: newest = max(newest, os.stat(os.path.join(folder, name)).st_mtime)
                        except OSError: pass
        GalleryHandler._stamp = (newest, now)
        return newest

    def send_file(self, filename):
        try:
            source = filename.open('rb')
        except OSError:
            self.send_error(404); return None
        size = os.fstat(source.fileno()).st_size
        self.send_response(200); self.send_header('Content-Type', self.guess_type(str(filename)))
        self.send_header('Content-Length', str(size))
        self.send_header('Last-Modified', self.date_time_string(filename.stat().st_mtime))
        self.end_headers(); return source

    def send_head(self):
        self._range = None
        if self.live and urlsplit(self.path).path == '/__live':
            body = json.dumps({'stamp': self.stamp()}).encode()
            self._file_response = False
            self.send_response(200); self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body))); self.end_headers()
            return io.BytesIO(body)
        filename = Path(self.translate_path(self.path))
        if filename.is_dir() and urlsplit(self.path).path.endswith('/'):
            # A directory shows its index page, the source copy of it first.
            relative = unquote(urlsplit(self.path).path).lstrip('/')
            for root in filter(None, [self.source, self.directory]):
                index = Path(root) / relative / 'index.html'
                if index.is_file():
                    filename = index
                    break
        self._file_response = filename.is_file()
        if self.live and self._file_response and filename.suffix == '.html' and not self.headers.get('Range'):
            try:
                page = filename.read_bytes()
            except OSError:
                self.send_error(404); return None
            marker = page.rfind(b'</body>')
            page = page[:marker] + RELOAD + page[marker:] if marker >= 0 else page + RELOAD
            self.send_response(200); self.send_header('Content-Type', 'text/html')
            self.send_header('Content-Length', str(len(page)))
            self.send_header('Last-Modified', self.date_time_string(filename.stat().st_mtime))
            self.end_headers()
            return io.BytesIO(page)
        value = self.headers.get('Range')
        if not value or not self._file_response:
            if self._file_response and filename.suffix == '.html' and filename != Path(self.translate_path(self.path)):
                return self.send_file(filename)
            return super().send_head()
        if self.headers.get('If-Range'):
            try:
                modified_since = parsedate_to_datetime(self.headers['If-Range']).timestamp()
                if int(filename.stat().st_mtime) > modified_since:
                    return super().send_head()
            except (ValueError, TypeError, OverflowError):
                return super().send_head()
        try:
            source = filename.open('rb')
        except OSError:
            self.send_error(404); return None
        stat = os.fstat(source.fileno()); size = stat.st_size
        match = re.fullmatch(r'bytes=(\d*)-(\d*)', value.strip())
        start, end = 0, size - 1
        valid = bool(match and size)
        if valid:
            first, last = match.groups()
            if first:
                start = int(first); end = min(int(last), size - 1) if last else size - 1
                valid = start < size and start <= end
            else:
                count = int(last) if last else 0
                start = max(0, size - count); valid = count > 0
        if not valid:
            source.close(); self.send_response(416)
            self.send_header('Content-Range', f'bytes */{size}')
            self.send_header('Content-Length', '0'); self.end_headers(); return None
        source.seek(start); self._range = (start, end)
        self.send_response(206)
        self.send_header('Content-Type', self.guess_type(str(filename)))
        self.send_header('Content-Range', f'bytes {start}-{end}/{size}')
        self.send_header('Content-Length', str(end - start + 1))
        self.send_header('Last-Modified', self.date_time_string(stat.st_mtime))
        self.end_headers(); return source

    def copyfile(self, source, output):
        try:
            if self._range is None:
                return super().copyfile(source, output)
            start, end = self._range; remaining = end - start + 1
            while remaining:
                block = source.read(min(64 * 1024, remaining))
                if not block: break
                output.write(block); remaining -= len(block)
        except (BrokenPipeError, ConnectionResetError):
            pass

if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', default='target/component-site')
    parser.add_argument('--bind', default='127.0.0.1')
    parser.add_argument('--port', type=int, default=8094)
    parser.add_argument('--source', default=str(Path(__file__).resolve().parents[1]),
                        help='serve files that exist here (default: web/) in front of the staged copy')
    parser.add_argument('--no-source', action='store_true', help='serve only the staged copy')
    parser.add_argument('--live', action='store_true', help='reload open pages when a page or script changes')
    args=parser.parse_args()
    source=None if args.no_source else str(Path(args.source).resolve())
    handler=functools.partial(GalleryHandler, directory=str(Path(args.directory).resolve()))
    GalleryHandler.source=source; GalleryHandler.live=args.live
    with http.server.ThreadingHTTPServer((args.bind,args.port),handler) as server:
        print(f'Gallery: http://{args.bind}:{args.port}/components/',flush=True)
        if source: print(f'Source files in {source} are served over the staged copy',flush=True)
        if args.live: print('Live reload is on: open pages reload when a page or script changes',flush=True)
        try: server.serve_forever()
        except KeyboardInterrupt: pass
