"""Regression fixtures for the DAW evidence checker, independent of actual DAW runs."""
from pathlib import Path
import math
import struct
import shutil
import subprocess
import sys
import tempfile
import unittest
import wave

HERE = Path(__file__).resolve().parent

class EvidenceChecks(unittest.TestCase):
    def test_generated_source_is_exact_four_second_stereo(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = subprocess.run([sys.executable, str(HERE/'prepare.py'), str(root)],capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual((root/'ravet-validation/public-fixture.txt').read_text(),'RAVET public synthetic validation\n')
            with wave.open(str(root/'ravet-validation/input.wav'),'rb') as audio:
                self.assertEqual((audio.getnchannels(),audio.getsampwidth(),audio.getframerate(),audio.getnframes()),(2,2,48000,192000))
                self.assertEqual(audio.readframes(3),struct.pack('<hh',8192,-16384)*3)

    def test_guarded_scripts_reject_before_host_mutations(self):
        lua = shutil.which('lua')
        if lua is None:
            self.skipTest('optional Lua interpreter is not installed')
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            fixture=root/'ravet-validation';fixture.mkdir()
            stub=root/'guard.lua'
            stub.write_text("""reaper=setmetatable({
 GetResourcePath=function() return arg[2] end,
 CountTracks=function() return tonumber(arg[3]) end
},{__index=function() error('host mutation reached') end})
local okay,message=pcall(dofile,arg[1])
assert(not okay)
assert(string.find(message,arg[4],1,true),message)
assert(io.open(arg[2]..'/ravet-validation/'..arg[5],'r')==nil)
""")
            for script,log in [('native-meters.lua','native-meter-host.log'),('performance-instrument.lua','performance-host.log')]:
                marker=fixture/'public-fixture.txt'
                if marker.exists():marker.unlink()
                for contents,tracks,expected in [(None,0,'prepare public fixtures first'),('incorrect',0,'public fixture marker'),('RAVET public synthetic validation\n',1,'new empty disposable project required')]:
                    if contents is not None:
                        marker.write_text(contents)
                    result=subprocess.run([lua,str(stub),str(HERE/script),str(root),str(tracks),expected,log],capture_output=True,text=True)
                    self.assertEqual(result.returncode,0,result.stderr)

    def test_known_renders_pass_and_bad_audio_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            def write(name, first, second=None):
                with wave.open(str(root/(name+'.wav')),'wb') as audio:
                    audio.setnchannels(2); audio.setsampwidth(2); audio.setframerate(48000)
                    second=first if second is None else second
                    audio.writeframes(struct.pack('<hh',*first)*96000+struct.pack('<hh',*second)*96000)
            write('wet',(12288,-24576)); write('dry',(8192,-16384)); write('automation',(4096,-8192),(16384,-32768))
            command=[sys.executable,str(HERE/'check-renders.py'),str(root)]
            result=subprocess.run(command,capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertIn('PASS automation 2.05-3.9s',result.stdout)
            write('wet',(8192,-16384))
            result=subprocess.run(command,capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0)
            self.assertIn('channel errors',result.stderr)

    def test_performance_renders_require_audio_frequency_and_complete_host_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            resource=Path(temporary)
            fixture=resource/'ravet-validation';fixture.mkdir()
            def write(label, frequency=440):
                frames=bytearray()
                for i in range(192000):
                    time=i/48000
                    amplitude=.02 if label=='automation' and time<1.5 else .04
                    value=round(32768*amplitude*math.sin(2*math.pi*frequency*time)) if .25<=time<2.25 else 0
                    frames.extend(struct.pack('<hh',value,value))
                with wave.open(str(fixture/(label+'.wav')),'wb') as audio:
                    audio.setnchannels(2);audio.setsampwidth(2);audio.setframerate(48000)
                    audio.writeframes(frames)
            for label in ['level','recalled','automation']:write(label)
            log=fixture/'performance-host.log'
            evidence='\n'.join('rendered '+label+'='+str(fixture/(label+'.wav')) for label in ['level','recalled','automation'])
            success='\nPASS REAPER VST3 MIDI/state/automation renders prepared\n'
            log.write_text(evidence+success)
            command=[sys.executable,str(HERE/'check-performance-render.py'),str(resource)]
            result=subprocess.run(command,capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertIn('PASS recalled 0.6-1.2s',result.stdout)
            write('level',880)
            result=subprocess.run(command,capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0)
            self.assertIn('expected A4 frequency',result.stderr)
            log.write_text(evidence+'\nFAIL incomplete host run\n')
            result=subprocess.run(command,capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0)
            self.assertIn('host fixture did not complete',result.stderr)

if __name__ == '__main__': unittest.main()
