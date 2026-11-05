"""Exercise the preview server's HTTP contract, including browser seek ranges."""
import functools
import http.client
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

if __name__=='__main__':unittest.main()
