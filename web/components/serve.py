#!/usr/bin/env python3
"""Serve the staged gallery with byte ranges for real browser media seeking."""
import argparse
import functools
import http.server
import os
import re
from email.utils import parsedate_to_datetime
from pathlib import Path

class GalleryHandler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, '.wasm':'application/wasm'}

    def end_headers(self):
        if getattr(self, '_file_response', False):
            self.send_header('Accept-Ranges', 'bytes')
        super().end_headers()

    def send_head(self):
        self._range = None
        filename = Path(self.translate_path(self.path))
        self._file_response = filename.is_file()
        value = self.headers.get('Range')
        if not value or not self._file_response:
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
    args=parser.parse_args()
    handler=functools.partial(GalleryHandler, directory=str(Path(args.directory).resolve()))
    with http.server.ThreadingHTTPServer((args.bind,args.port),handler) as server:
        print(f'Gallery: http://{args.bind}:{args.port}/components/',flush=True)
        try: server.serve_forever()
        except KeyboardInterrupt: pass
