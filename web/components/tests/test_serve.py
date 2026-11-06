"""Exercise the preview server's HTTP contract, including browser seek ranges."""
import functools
import http.client
import json
import os
import importlib.util
from pathlib import Path
import tempfile
import threading
import unittest

spec = importlib.util.spec_from_file_location('gallery_serve', Path(__file__).resolve().parents[1]/'serve.py')
module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
class QuietHandler(module.GalleryHandler):
    def log_message(self, *_): pass

class MediaRanges(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory=tempfile.TemporaryDirectory()
        cls.data=bytes(range(256))*8
        Path(cls.directory.name,'clip.mp4').write_bytes(cls.data)
        cls.server=module.http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(QuietHandler,directory=cls.directory.name))
        cls.thread=threading.Thread(target=cls.server.serve_forever,daemon=True);cls.thread.start()
    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown();cls.server.server_close();cls.thread.join();cls.directory.cleanup()
    def request(self,method='GET',**headers):
        connection=http.client.HTTPConnection(*self.server.server_address)
        connection.request(method,'/clip.mp4',headers=headers);response=connection.getresponse()
        result=(response.status,dict(response.getheaders()),response.read());connection.close();return result
    def test_complete_file_and_seek_ranges(self):
        status,headers,body=self.request()
        self.assertEqual((status,body),(200,self.data));self.assertEqual(headers['Accept-Ranges'],'bytes')
        for value,start,end in [('bytes=0-1023',0,1023),('bytes=1000-',1000,2047),('bytes=-7',2041,2047)]:
            with self.subTest(range=value):
                status,headers,body=self.request(Range=value)
                self.assertEqual(status,206);self.assertEqual(body,self.data[start:end+1])
                self.assertEqual(headers['Content-Range'],f'bytes {start}-{end}/2048')
                self.assertEqual(int(headers['Content-Length']),len(body))
    def test_head_and_unsatisfiable_ranges(self):
        status,headers,body=self.request('HEAD',Range='bytes=100-199')
        self.assertEqual((status,body),(206,b''));self.assertEqual(headers['Content-Length'],'100')
        status,headers,body=self.request(Range='bytes=3000-')
        self.assertEqual((status,body),(416,b''));self.assertEqual(headers['Content-Range'],'bytes */2048')
    def test_if_range_uses_the_requested_file_version(self):
        _,headers,_=self.request('HEAD')
        status,_,body=self.request(Range='bytes=0-3',**{'If-Range':headers['Last-Modified']})
        self.assertEqual((status,body),(206,self.data[:4]))
        status,_,body=self.request(Range='bytes=0-3',**{'If-Range':'Sun, 06 Nov 1994 08:49:37 GMT'})
        self.assertEqual((status,body),(200,self.data))

class SourceOverlay(unittest.TestCase):
    """Edited source files show at once, and open pages reload when one changes."""
    @classmethod
    def setUpClass(cls):
        cls.root=tempfile.TemporaryDirectory()
        cls.staged=Path(cls.root.name,'staged'); cls.source=Path(cls.root.name,'web')
        for folder in (cls.staged,cls.source,cls.staged/'components'): folder.mkdir(parents=True,exist_ok=True)
        (cls.staged/'index.html').write_text('<html><body>staged</body></html>')
        (cls.staged/'components'/'index.html').write_text('<html><body>catalog</body></html>')
        (cls.staged/'only-staged.txt').write_text('staged only')
        (cls.source/'index.html').write_text('<html><body>edited</body></html>')
        class Live(QuietHandler): pass
        Live.source=str(cls.source); Live.live=True
        cls.handler=Live
        cls.server=module.http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Live,directory=str(cls.staged)))
        cls.thread=threading.Thread(target=cls.server.serve_forever,daemon=True);cls.thread.start()
    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown();cls.server.server_close();cls.thread.join();cls.root.cleanup()
    def get(self,path):
        connection=http.client.HTTPConnection(*self.server.server_address)
        connection.request('GET',path);response=connection.getresponse()
        result=(response.status,dict(response.getheaders()),response.read());connection.close();return result
    def test_a_source_file_is_served_in_front_of_the_staged_copy(self):
        status,headers,body=self.get('/')
        self.assertEqual(status,200);self.assertIn(b'edited',body);self.assertNotIn(b'staged',body)
        status,_,body=self.get('/only-staged.txt')
        self.assertEqual((status,body),(200,b'staged only'))
    def test_a_directory_comes_from_the_staged_copy(self):
        status,_,body=self.get('/components/')
        self.assertEqual(status,200);self.assertIn(b'catalog',body)
    def test_responses_ask_browsers_to_revalidate(self):
        for path in ('/','/only-staged.txt'):
            self.assertEqual(self.get(path)[1]['Cache-Control'],'no-cache')
    def test_pages_carry_the_reload_script_and_the_stamp_follows_edits(self):
        _,headers,body=self.get('/')
        self.assertIn(b'/__live',body);self.assertEqual(int(headers['Content-Length']),len(body))
        before=json.loads(self.get('/__live')[2])['stamp']
        page=self.source/'index.html'; page.write_text('<html><body>edited again</body></html>')
        os.utime(page,(before+10,before+10)); module.GalleryHandler._stamp=(0.0,0.0)
        self.assertGreater(json.loads(self.get('/__live')[2])['stamp'],before)
    def test_files_outside_the_source_are_not_reached(self):
        status,_,body=self.get('/..%2f..%2fetc/hosts')
        self.assertNotEqual((status,body[:1]),(200,b'1'))

if __name__=='__main__':unittest.main()
