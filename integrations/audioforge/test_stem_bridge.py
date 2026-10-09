import importlib.util
import tempfile
import unittest
import wave
from pathlib import Path
spec=importlib.util.spec_from_file_location('stem_bridge',Path(__file__).with_name('stem_bridge.py'))
stem=importlib.util.module_from_spec(spec);spec.loader.exec_module(stem)

class StemInputTests(unittest.TestCase):
    def write(self,path,frames=80,width=2):
        with wave.open(str(path),'wb') as audio:
            audio.setnchannels(1);audio.setsampwidth(width);audio.setframerate(8000)
            audio.writeframes(bytes(frames*width))
    def test_pcm16_input_and_no_overwrite(self):
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/'input.wav';self.write(path)
            rate,channels,frames,values=stem.inspect_input(path)
            self.assertEqual((rate,channels,frames,len(values)),(8000,1,80,80))
            out=Path(temp)/'out';out.mkdir();(out/'keep.txt').write_text('keep')
            with self.assertRaises(ValueError):stem.separate(path,out)
            self.assertEqual((out/'keep.txt').read_text(),'keep')
    def test_rejects_unsupported_width_and_duration(self):
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/'input.wav';self.write(path,width=3)
            with self.assertRaises(ValueError):stem.inspect_input(path)
            self.write(path,frames=8000*61)
            with self.assertRaises(ValueError):stem.inspect_input(path)
    def test_rejects_truncated_samples(self):
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/'input.wav';self.write(path)
            path.write_bytes(path.read_bytes()[:-30])
            with self.assertRaises(ValueError):stem.inspect_input(path)

if __name__=='__main__':unittest.main()
